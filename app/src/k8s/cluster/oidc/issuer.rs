//! Talking to the OIDC issuer: discovery for its token endpoint, then the
//! OAuth2 `refresh_token` grant.
//!
//! Requests go through a `kube::Client` pointed at the issuer, so they use the
//! app's own TLS stack - the platform's roots, or the CA the kubeconfig gives
//! for the issuer (`idp-certificate-authority`, `-data`), loaded by `kube`
//! exactly as it loads a cluster's.

use super::Tokens;
use crate::consts::OIDC_ISSUER_TIMEOUT;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::{Request, Uri, header};
use kube::client::Body;
use kube::config::{Cluster, Context, KubeConfigOptions, Kubeconfig, NamedCluster, NamedContext};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use std::collections::HashMap;

const ISSUER_URL: &str = "idp-issuer-url";
const CLIENT_ID: &str = "client-id";
const CLIENT_SECRET: &str = "client-secret";
const REFRESH_TOKEN: &str = super::REFRESH_TOKEN;
const CA_FILE: &str = "idp-certificate-authority";
const CA_DATA: &str = "idp-certificate-authority-data";

/// `application/x-www-form-urlencoded`'s unreserved characters: everything
/// else in a value is percent-encoded.
const FORM: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Whether `config` has what [`refresh`] needs: an issuer, a refresh token
/// and a client id.
pub(super) fn can_refresh(config: &HashMap<String, String>) -> bool {
    [ISSUER_URL, REFRESH_TOKEN, CLIENT_ID]
        .iter()
        .all(|key| config.get(*key).is_some_and(|value| !value.is_empty()))
}

/// New tokens for the provider `config`, by its refresh token. The error says
/// what went wrong without any token in it.
pub(super) async fn refresh(config: &HashMap<String, String>) -> Result<Tokens, String> {
    let field = |key: &str| config.get(key).filter(|value| !value.is_empty());
    let (Some(issuer), Some(refresh_token), Some(client_id)) =
        (field(ISSUER_URL), field(REFRESH_TOKEN), field(CLIENT_ID))
    else {
        return Err(format!(
            "the kubeconfig lacks the {ISSUER_URL}, {REFRESH_TOKEN} and {CLIENT_ID} to renew it - log in again"
        ));
    };
    let ca = (field(CA_FILE).cloned(), field(CA_DATA).cloned());

    let issuer = trusted_url(issuer.trim_end_matches('/'))?;
    let discovery = format!(
        "{}/.well-known/openid-configuration",
        issuer.to_string().trim_end_matches('/')
    );
    let endpoint = token_endpoint(&discovery, &ca).await?;
    let endpoint = trusted_url(&endpoint)?;

    let secret = field(CLIENT_SECRET).map(String::as_str);
    let form = |with_client: bool| {
        let mut pairs = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
        ];
        if with_client {
            pairs.push(("client_id", client_id.as_str()));
            if let Some(secret) = secret {
                pairs.push(("client_secret", secret));
            }
        }
        pairs
            .into_iter()
            .map(|(key, value)| format!("{key}={}", utf8_percent_encode(value, FORM)))
            .collect::<Vec<_>>()
            .join("&")
    };

    // As OAuth2 clients auto-detect it: the client's credentials in a Basic
    // header first, then, if the issuer refuses that, in the form.
    let mut attempts = Vec::new();
    if let Some(secret) = secret {
        let basic = STANDARD.encode(format!(
            "{}:{}",
            utf8_percent_encode(client_id, FORM),
            utf8_percent_encode(secret, FORM)
        ));
        attempts.push((Some(format!("Basic {basic}")), form(false)));
    }
    attempts.push((None, form(true)));

    let mut refusal = String::new();
    for (authorization, body) in attempts {
        let (status, body) = post_form(&endpoint, authorization, body, &ca).await?;
        if status.is_success() {
            return parse_tokens(&body);
        }
        refusal = describe_refusal(status, &body);
    }
    Err(format!(
        "the OIDC issuer refused to renew it ({refusal}) - log in again"
    ))
}

/// `url` parsed, if it's one the refresh token may be sent to: HTTPS, or
/// plain HTTP only to this machine.
fn trusted_url(url: &str) -> Result<Uri, String> {
    let uri: Uri = url
        .parse()
        .map_err(|_| "the OIDC issuer URL in the kubeconfig isn't a valid URL".to_string())?;
    let loopback = matches!(
        uri.host(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    match uri.scheme_str() {
        Some("https") => Ok(uri),
        Some("http") if loopback => Ok(uri),
        _ => {
            Err("the OIDC issuer URL isn't HTTPS, so the refresh token won't be sent to it".into())
        }
    }
}

/// The token endpoint from the issuer's discovery document.
async fn token_endpoint(discovery: &str, ca: &Ca) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Metadata {
        token_endpoint: String,
    }
    let uri = trusted_url(discovery)?;
    let request = Request::get(path_and_query(&uri))
        .body(Body::empty())
        .map_err(|error| error.to_string())?;
    let (status, body) = send(&uri, request, ca).await?;
    if !status.is_success() {
        return Err(format!(
            "the OIDC issuer's discovery document couldn't be fetched (HTTP {status})"
        ));
    }
    serde_json::from_slice::<Metadata>(&body)
        .map(|metadata| metadata.token_endpoint)
        .map_err(|_| "the OIDC issuer's discovery document names no token endpoint".to_string())
}

async fn post_form(
    endpoint: &Uri,
    authorization: Option<String>,
    form: String,
    ca: &Ca,
) -> Result<(http::StatusCode, Vec<u8>), String> {
    let mut request = Request::post(path_and_query(endpoint))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ACCEPT, "application/json");
    if let Some(authorization) = authorization {
        request = request.header(header::AUTHORIZATION, authorization);
    }
    let request = request
        .body(Body::from(form.into_bytes()))
        .map_err(|error| error.to_string())?;
    send(endpoint, request, ca).await
}

fn parse_tokens(body: &[u8]) -> Result<Tokens, String> {
    #[derive(serde::Deserialize)]
    struct Response {
        id_token: Option<String>,
        refresh_token: Option<String>,
    }
    let response: Response = serde_json::from_slice(body)
        .map_err(|_| "the OIDC issuer's token response wasn't JSON".to_string())?;
    let id_token = response
        .id_token
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "the OIDC issuer's token response had no id_token".to_string())?;
    Ok(Tokens {
        id_token,
        refresh_token: response.refresh_token.filter(|token| !token.is_empty()),
    })
}

/// An issuer's refusal as its status and OAuth2 `error` code (`invalid_grant`
/// for a used or revoked refresh token) - never the body itself.
fn describe_refusal(status: http::StatusCode, body: &[u8]) -> String {
    #[derive(serde::Deserialize)]
    struct OAuthError {
        error: String,
    }
    match serde_json::from_slice::<OAuthError>(body) {
        Ok(OAuthError { error }) => format!("HTTP {status}: {error}"),
        Err(_) => format!("HTTP {status}"),
    }
}

/// The issuer's CA as the kubeconfig gives it: a file, inline data, or
/// neither for the platform's roots.
type Ca = (Option<String>, Option<String>);

/// Sends `request` to `uri`'s host through a kube client built for it, and
/// reads the whole response.
async fn send(
    uri: &Uri,
    request: Request<Body>,
    ca: &Ca,
) -> Result<(http::StatusCode, Vec<u8>), String> {
    let host = host_of(uri)?;
    let client = client_for(&host, ca).await?;
    let response = client
        .send(request)
        .await
        .map_err(|error| format!("the OIDC issuer at {host} couldn't be reached ({error})"))?;
    let status = response.status();
    let body = response
        .into_body()
        .collect_bytes()
        .await
        .map_err(|error| format!("the OIDC issuer's response couldn't be read ({error})"))?;
    Ok((status, body.to_vec()))
}

/// `scheme://authority` of `uri` - what a client is built for; the request
/// carries the path.
fn host_of(uri: &Uri) -> Result<String, String> {
    match (uri.scheme_str(), uri.authority()) {
        (Some(scheme), Some(authority)) => Ok(format!("{scheme}://{authority}")),
        _ => Err("the OIDC issuer URL has no host".to_string()),
    }
}

fn path_and_query(uri: &Uri) -> String {
    uri.path_and_query()
        .map_or("/", |path| path.as_str())
        .to_string()
}

/// A client for `host`, trusting `ca` if given. Built through a one-cluster
/// kubeconfig, so `kube` reads the CA file or data just as it does a
/// cluster's `certificate-authority`.
async fn client_for(host: &str, ca: &Ca) -> Result<kube::Client, String> {
    const NAME: &str = "oidc-issuer";
    let kubeconfig = Kubeconfig {
        clusters: vec![NamedCluster {
            name: NAME.into(),
            cluster: Some(Cluster {
                server: Some(host.to_string()),
                certificate_authority: ca.0.clone(),
                certificate_authority_data: ca.1.clone(),
                ..Default::default()
            }),
            ..Default::default()
        }],
        contexts: vec![NamedContext {
            name: NAME.into(),
            context: Some(Context {
                cluster: NAME.into(),
                ..Default::default()
            }),
            ..Default::default()
        }],
        current_context: Some(NAME.into()),
        ..Default::default()
    };
    let mut config =
        kube::Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default())
            .await
            .map_err(|error| format!("the OIDC issuer's CA couldn't be loaded ({error})"))?;
    config.connect_timeout = Some(OIDC_ISSUER_TIMEOUT);
    config.read_timeout = Some(OIDC_ISSUER_TIMEOUT);
    config.default_retry = false;
    kube::Client::try_from(config)
        .map_err(|error| format!("a client for the OIDC issuer couldn't be built ({error})"))
}
