//! Scrubbing credentials out of text that came from upstream - a Kubernetes
//! API `Status` message, chiefly - before it reaches an MCP client or a log
//! line (`agent-mcp`: Credential and error safety).
//!
//! It is a backstop, not the main defence: `error::ToolError` already passes
//! on nothing from an error but an API `Status`'s code, reason and message.
//! What it catches is a credential echoed inside that message: an
//! `Authorization` header value, a `Bearer`/`Basic` token, a `key=value` pair
//! whose key names a secret, or a bare JWT.

/// What every scrubbed value is replaced with.
pub(super) const REDACTED: &str = "[redacted]";

/// Words after which the next word is a credential: `Bearer <token>`,
/// `Authorization: <value>`.
const PREFIXES: &[&str] = &["bearer", "basic", "authorization", "authorization:"];

/// Key names (lowercase, compared on the part before `=` or `:`) whose value
/// is a credential.
const SECRET_KEYS: &[&str] = &[
    "token",
    "access_token",
    "access-token",
    "id_token",
    "id-token",
    "refresh_token",
    "refresh-token",
    "password",
    "passwd",
    "secret",
    "client-secret",
    "client_secret",
    "client-key-data",
    "client-certificate-data",
    "authorization",
];

/// `text` with every credential-looking value replaced by [`REDACTED`].
/// Whitespace between words is kept as it was.
pub(super) fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut redact_next = false;
    let mut rest = text;
    while !rest.is_empty() {
        let space = rest.len() - rest.trim_start().len();
        out.push_str(&rest[..space]);
        rest = &rest[space..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = &rest[..end];
        rest = &rest[end..];
        if word.is_empty() {
            continue;
        }

        let bare = word
            .trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']'))
            .to_ascii_lowercase();
        if PREFIXES.contains(&bare.as_str()) {
            // `Authorization: Bearer <token>`: the scheme is kept, the word after it isn't.
            out.push_str(word);
            redact_next = true;
        } else if redact_next {
            out.push_str(REDACTED);
            redact_next = false;
        } else if let Some(kept) = secret_pair_key(word) {
            out.push_str(kept);
            out.push_str(REDACTED);
        } else if looks_like_jwt(&bare) {
            out.push_str(REDACTED);
        } else {
            out.push_str(word);
        }
    }
    out
}

/// For `key=value` or `key:value` where `key` names a secret, the part to keep
/// - the key and its separator.
fn secret_pair_key(word: &str) -> Option<&str> {
    let split = word.find(['=', ':'])?;
    let key = word[..split]
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '{' | '('))
        .to_ascii_lowercase();
    let has_value = split + 1 < word.len();
    (has_value && SECRET_KEYS.contains(&key.as_str())).then(|| &word[..=split])
}

/// A JWT: three dot-separated base64url segments, the first a JSON header
/// (`{"` encodes as `eyJ`).
fn looks_like_jwt(word: &str) -> bool {
    let segments: Vec<&str> = word.split('.').collect();
    segments.len() == 3
        && word.starts_with("eyj")
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const JWT: &str = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJhZG1pbiJ9.c2lnbmF0dXJl";

    #[test]
    fn a_bearer_token_is_scrubbed() {
        assert_eq!(
            redact("request had Authorization: Bearer abc123def"),
            format!("request had Authorization: Bearer {REDACTED}")
        );
        assert_eq!(
            redact("using bearer s3cr3t now"),
            format!("using bearer {REDACTED} now")
        );
    }

    #[test]
    fn secret_key_value_pairs_keep_their_key() {
        assert_eq!(
            redact("failed: token=abc password:hunter2 user=alice"),
            format!("failed: token={REDACTED} password:{REDACTED} user=alice")
        );
        assert_eq!(
            redact(r#"{"client-key-data":"LS0tLS1CRUdJTi"}"#),
            format!(r#"{{"client-key-data":{REDACTED}"#)
        );
    }

    #[test]
    fn a_bare_jwt_is_scrubbed_wherever_it_appears() {
        assert_eq!(
            redact(&format!("invalid token {JWT}, expired")),
            format!("invalid token {REDACTED} expired")
        );
    }

    #[test]
    fn ordinary_api_messages_pass_through_unchanged() {
        for message in [
            r#"pods "api-7d9f" not found"#,
            r#"deployments.apps "web" is forbidden: User "dev" cannot patch resource "deployments""#,
            "the server could not find the requested resource",
            "namespace: team-a, name: api.v1.example",
        ] {
            assert_eq!(redact(message), message);
        }
    }

    #[test]
    fn whitespace_is_preserved() {
        assert_eq!(redact("  a\n\tb  "), "  a\n\tb  ");
    }
}
