//! #188: an OIDC auth-provider id-token is renewed against a fake issuer when
//! it has expired, the rotated tokens written back to the kubeconfig as
//! `kubectl` does - and left alone when it is still good.

use super::test_support::{FakeIssuer, TempDir, fake_issuer, from_now, jwt, oidc_kubeconfig};
use super::*;
use kube::config::Kubeconfig;

const OLD_REFRESH: &str = "fake-refresh-old";
const NEW_REFRESH: &str = "fake-refresh-new";

/// The `dex-user`'s auth-provider config in `kubeconfig`.
fn provider_config(kubeconfig: &Kubeconfig) -> HashMap<String, String> {
    oidc_config(kubeconfig, "dex-user")
        .expect("dex-user has an oidc provider")
        .clone()
}

#[test]
fn a_token_is_renewed_when_expired_unreadable_or_about_to_expire() {
    let now = Timestamp::now();
    let at = |offset: i64| expiry(&jwt(now.as_second() + offset, "t"));
    assert!(needs_refresh(at(-3600), now), "expired an hour ago");
    assert!(needs_refresh(at(30), now), "expires within the margin");
    assert!(!needs_refresh(at(3600), now), "good for an hour");
    assert!(needs_refresh(None, now), "missing or not a JWT");
    assert_eq!(expiry("not-a-jwt"), None);
    assert_eq!(expiry("a.!!!.c"), None);
}

#[tokio::test]
async fn an_expired_id_token_is_renewed_and_written_back() {
    let new_id = jwt(from_now(3600), "renewed");
    let issuer = fake_issuer(OLD_REFRESH, new_id.clone(), NEW_REFRESH);
    let dir = TempDir::new("renew");
    let file = dir.write(
        "config",
        &oidc_kubeconfig(
            "https://127.0.0.1:6443",
            &format!("{}/dex", issuer.url()),
            &jwt(from_now(-3600), "expired"),
            OLD_REFRESH,
        ),
    );
    let mut kubeconfig = Kubeconfig::read_from(&file).expect("the fixture parses");

    refresh_expired(&mut kubeconfig, Some("dex"), std::slice::from_ref(&file))
        .await
        .expect("the expired token renews");

    let config = provider_config(&kubeconfig);
    assert_eq!(
        config[ID_TOKEN], new_id,
        "the connection uses the new token"
    );
    assert_eq!(
        config[REFRESH_TOKEN], NEW_REFRESH,
        "and keeps the rotated refresh token"
    );

    let saved = provider_config(&Kubeconfig::read_from(&file).expect("still a kubeconfig"));
    assert_eq!(saved[ID_TOKEN], new_id, "written back, as kubectl does");
    assert_eq!(saved[REFRESH_TOKEN], NEW_REFRESH);
    assert_eq!(
        saved["client-id"], "fernrohr",
        "the rest of the provider kept"
    );
    let other = Kubeconfig::read_from(&file).unwrap();
    assert!(
        other
            .auth_infos
            .iter()
            .any(|user| user.name == "other-user"),
        "other users kept"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the kubeconfig stays owner-only");
    }

    let requests = issuer.requests();
    assert_eq!(requests[0].path, "/dex/.well-known/openid-configuration");
    let grant = requests
        .iter()
        .find(|request| request.method == "POST")
        .unwrap();
    assert_eq!(grant.path, "/dex/token");
    assert_eq!(grant.form("grant_type").as_deref(), Some("refresh_token"));
    assert_eq!(grant.form("refresh_token").as_deref(), Some(OLD_REFRESH));
}

#[tokio::test]
async fn a_valid_id_token_is_left_alone() {
    let issuer = fake_issuer(OLD_REFRESH, jwt(from_now(3600), "unused"), NEW_REFRESH);
    let valid = jwt(from_now(3600), "valid");
    let dir = TempDir::new("valid");
    let text = oidc_kubeconfig(
        "https://127.0.0.1:6443",
        &format!("{}/dex", issuer.url()),
        &valid,
        OLD_REFRESH,
    );
    let file = dir.write("config", &text);
    let mut kubeconfig = Kubeconfig::read_from(&file).unwrap();

    refresh_expired(&mut kubeconfig, None, std::slice::from_ref(&file))
        .await
        .expect("nothing to renew");

    assert_eq!(provider_config(&kubeconfig)[ID_TOKEN], valid);
    assert!(issuer.requests().is_empty(), "the issuer isn't asked");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        text,
        "the file isn't touched"
    );
}

#[tokio::test]
async fn a_refused_refresh_says_why_without_the_token() {
    let issuer = fake_issuer("some-other-refresh", jwt(from_now(3600), "x"), NEW_REFRESH);
    let mut kubeconfig: Kubeconfig = serde_yaml_ng::from_str(&oidc_kubeconfig(
        "https://127.0.0.1:6443",
        &format!("{}/dex", issuer.url()),
        &jwt(from_now(-60), "expired"),
        OLD_REFRESH,
    ))
    .unwrap();

    let error = refresh_expired(&mut kubeconfig, Some("dex"), &[])
        .await
        .expect_err("the issuer refuses a refresh token it doesn't know");

    assert!(error.contains("invalid_grant"), "{error}");
    assert!(error.contains("log in again"), "{error}");
    assert!(!error.contains(OLD_REFRESH), "no token in the reason");
    let attempts = issuer
        .requests()
        .iter()
        .filter(|r| r.method == "POST")
        .count();
    assert_eq!(attempts, 2, "a Basic header, then the client in the form");
}

/// Discovery must name the configured issuer: a document claiming another
/// gets neither the refresh token nor the client secret.
#[tokio::test]
async fn a_discovery_document_naming_another_issuer_is_refused() {
    let issuer = FakeIssuer {
        refresh_token: OLD_REFRESH,
        new_id_token: jwt(from_now(3600), "x"),
        new_refresh_token: NEW_REFRESH,
        claimed_issuer: Some("https://issuer.invalid/dex".into()),
        token_delay: std::time::Duration::ZERO,
    }
    .start();
    let mut kubeconfig: Kubeconfig = serde_yaml_ng::from_str(&oidc_kubeconfig(
        "https://127.0.0.1:6443",
        &format!("{}/dex", issuer.url()),
        &jwt(from_now(-60), "expired"),
        OLD_REFRESH,
    ))
    .unwrap();

    let error = refresh_expired(&mut kubeconfig, Some("dex"), &[])
        .await
        .expect_err("a mismatched issuer is refused");

    assert!(error.contains("different issuer"), "{error}");
    let requests = issuer.requests();
    assert!(
        requests.iter().all(|request| request.method == "GET"),
        "nothing is posted to its token endpoint"
    );
    assert!(
        requests
            .iter()
            .all(|request| !request.body.contains(OLD_REFRESH)),
        "the refresh token never leaves"
    );
}

/// A trailing slash on either side is the same issuer.
#[tokio::test]
async fn an_issuer_with_a_trailing_slash_still_matches() {
    let renewed = jwt(from_now(3600), "renewed");
    let issuer = fake_issuer(OLD_REFRESH, renewed.clone(), NEW_REFRESH);
    let mut kubeconfig: Kubeconfig = serde_yaml_ng::from_str(&oidc_kubeconfig(
        "https://127.0.0.1:6443",
        &format!("{}/dex/", issuer.url()),
        &jwt(from_now(-60), "expired"),
        OLD_REFRESH,
    ))
    .unwrap();

    refresh_expired(&mut kubeconfig, Some("dex"), &[])
        .await
        .expect("the same issuer, a trailing slash aside");

    assert_eq!(provider_config(&kubeconfig)[ID_TOKEN], renewed);
}

#[tokio::test]
async fn a_refresh_token_is_never_sent_to_a_plain_http_issuer_elsewhere() {
    let mut kubeconfig: Kubeconfig = serde_yaml_ng::from_str(&oidc_kubeconfig(
        "https://127.0.0.1:6443",
        "http://issuer.invalid/dex",
        &jwt(from_now(-60), "expired"),
        OLD_REFRESH,
    ))
    .unwrap();

    let error = refresh_expired(&mut kubeconfig, Some("dex"), &[])
        .await
        .expect_err("plain HTTP off this machine is refused");

    assert!(error.contains("isn't HTTPS"), "{error}");
}

#[tokio::test]
async fn an_unreadable_token_without_refresh_settings_is_left_for_the_server() {
    let mut kubeconfig: Kubeconfig = serde_yaml_ng::from_str(
        &oidc_kubeconfig("https://127.0.0.1:6443", "", "opaque-token", "")
            .replace("        client-id: fernrohr\n", ""),
    )
    .unwrap();

    refresh_expired(&mut kubeconfig, Some("dex"), &[])
        .await
        .expect("nothing it could renew, so nothing fails");

    assert_eq!(provider_config(&kubeconfig)[ID_TOKEN], "opaque-token");
}

#[test]
fn tokens_are_saved_to_the_file_that_defines_the_user() {
    let dir = TempDir::new("files");
    let first = dir.write(
        "first",
        "apiVersion: v1\nkind: Config\nusers:\n- name: someone-else\n  user:\n    token: x\n",
    );
    let first_text = std::fs::read_to_string(&first).unwrap();
    let second = dir.write(
        "second",
        &oidc_kubeconfig(
            "https://127.0.0.1:6443",
            "https://issuer.invalid",
            "old",
            "old",
        ),
    );
    let tokens = Tokens {
        id_token: "fake-new-id".into(),
        refresh_token: Some("fake-new-refresh".into()),
    };
    let files = [first.clone(), second.clone()];

    let file = persist::defining_file(&files, "dex-user").expect("the second file defines it");
    assert_eq!(file, second);
    persist::save(&file, "dex-user", &tokens).expect("saved");

    assert_eq!(std::fs::read_to_string(&first).unwrap(), first_text);
    let saved = provider_config(&Kubeconfig::read_from(&second).unwrap());
    assert_eq!(saved[ID_TOKEN], "fake-new-id");
    assert_eq!(saved[REFRESH_TOKEN], "fake-new-refresh");
    assert_eq!(
        persist::stored(&second, "dex-user").unwrap()[ID_TOKEN],
        "fake-new-id"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&second).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "a 0600 kubeconfig stays 0600 when rewritten");
    }
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        2,
        "no temp file left behind"
    );
    assert_eq!(persist::defining_file(&files, "nobody"), None);
}

#[test]
fn tokens_never_print() {
    let tokens = Tokens {
        id_token: "secret-id".into(),
        refresh_token: Some("secret-refresh".into()),
    };
    let printed = format!("{tokens:?}");
    assert!(!printed.contains("secret"), "{printed}");
}
