//! #188 end to end through the connect path: a context whose user logs in
//! through the oidc auth-provider reaches the API server with its id-token as
//! a Bearer token - renewed against the issuer first when it has expired,
//! which before the fix went out as it was and drew a 401.

use super::resolve_with_fresh_tokens;
use crate::k8s::cluster::oidc::test_support::{
    FakeServer, TempDir, fake_issuer, from_now, jwt, oidc_kubeconfig,
};
use kube::config::Kubeconfig;

/// A fake API server answering `/version`.
fn api_server() -> FakeServer {
    FakeServer::start(|request, _| {
        match request.path.as_str() {
        "/version" => (
            200,
            r#"{"major":"1","minor":"36","gitVersion":"v1.36.0","gitCommit":"","gitTreeState":"","buildDate":"","goVersion":"","compiler":"","platform":""}"#.to_string(),
        ),
        _ => (404, "{}".to_string()),
    }
    })
}

/// The `Authorization` the API server got on its first request, after
/// resolving `kubeconfig_text` and asking for the server's version.
async fn authorization_seen(kubeconfig_text: &str, api: &FakeServer, dir: &TempDir) -> String {
    let file = dir.write("config", kubeconfig_text);
    let kubeconfig = Kubeconfig::read_from(&file).expect("the fixture parses");
    let config = resolve_with_fresh_tokens(kubeconfig, Some("dex"), std::slice::from_ref(&file))
        .await
        .expect("the context resolves");
    let client = kube::Client::try_from(config).expect("a client");
    client
        .apiserver_version()
        .await
        .expect("the API server answers");
    api.requests()[0]
        .headers
        .get("authorization")
        .cloned()
        .unwrap_or_default()
}

#[tokio::test]
async fn a_valid_id_token_is_sent_as_a_bearer_token() {
    let api = api_server();
    let issuer = fake_issuer(
        "fake-refresh",
        jwt(from_now(3600), "unused"),
        "fake-refresh-2",
    );
    let valid = jwt(from_now(3600), "valid");
    let dir = TempDir::new("bearer");

    let seen = authorization_seen(
        &oidc_kubeconfig(
            &api.url(),
            &format!("{}/dex", issuer.url()),
            &valid,
            "fake-refresh",
        ),
        &api,
        &dir,
    )
    .await;

    assert_eq!(seen, format!("Bearer {valid}"));
    assert!(issuer.requests().is_empty(), "a good token isn't renewed");
}

#[tokio::test]
async fn an_expired_id_token_is_renewed_before_it_is_sent() {
    let api = api_server();
    let renewed = jwt(from_now(3600), "renewed");
    let issuer = fake_issuer("fake-refresh", renewed.clone(), "fake-refresh-2");
    let dir = TempDir::new("renew-connect");

    let seen = authorization_seen(
        &oidc_kubeconfig(
            &api.url(),
            &format!("{}/dex", issuer.url()),
            &jwt(from_now(-3600), "expired"),
            "fake-refresh",
        ),
        &api,
        &dir,
    )
    .await;

    assert_eq!(
        seen,
        format!("Bearer {renewed}"),
        "the renewed token, not the expired one"
    );
}
