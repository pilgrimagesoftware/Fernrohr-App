//! A human-readable rendering of a [`kube::Error`], shared by every panel that
//! surfaces a cluster API failure to the user - the log stream
//! (`util::logs::stream_container_logs`) and the pod-detail fetch
//! (`k8s::resource::pod_detail::fetch::fetch_pod`) today.
//!
//! `kube::Error::Api`'s own `Display` is `"ApiError: <message> (<Debug>)"`:
//! it tells a developer everything, and a user nothing readable - the shape
//! behind `1-window-context-bar` bug 2 (`ApiError: pods "clamav-plc9g" not
//! found ... code: 404` dumped straight into a panel). [`describe`] picks the
//! API's own reason and message out of that instead, and falls back to
//! `Display` for every other [`kube::Error`] variant.
//!
//! A 404 is deliberately not special-cased here: every caller already tells a
//! "this pod is gone" 404 apart from any other failure before it ever reaches
//! [`describe`], because "not found" is its own state (see
//! `pod_detail::fetch::PodDetailState::NotFound`), naming what's missing, not a
//! message this module could word better.

use crate::consts::{CREDENTIAL_PLUGIN_STDERR_LIMIT, ERROR_SOURCE_DEPTH_LIMIT};
use kube::client::AuthError;

/// The message half of a failure report: what a panel shows by default.
pub(crate) fn describe(error: &kube::Error) -> String {
    if let Some(failure) = credential_plugin_failure(error) {
        return failure;
    }
    match error {
        // kube's stand-in when the server's error body wasn't a `Status` (a 503
        // from an aggregated API's dead backend, say): its "reason" is kube's own
        // parse note, so the HTTP status and the body the server sent say more.
        kube::Error::Api(status) if status.reason == UNPARSED_ERROR_REASON => {
            format!("HTTP {}: {}", status.code, status.message.trim())
        }
        kube::Error::Api(status) => format!("{}: {}", status.reason, status.message),
        other => other.to_string(),
    }
}

/// The reason kube gives an error whose body it couldn't parse as a `Status`
/// (`kube_client::client::handle_api_errors`).
const UNPARSED_ERROR_REASON: &str = "Failed to parse error data";

/// The technical half: every field `Debug` can show, kept alongside the
/// readable message rather than discarded - a panel renders both, the second
/// as selectable/copyable detail for a report that needs more than the
/// summary.
/// A failed credential plugin's detail is its [`describe`] text: kube's
/// `Debug` of it carries what [`credential_plugin_failure`] leaves out.
pub(crate) fn detail(error: &kube::Error) -> String {
    match credential_plugin_failure(error) {
        Some(failure) => failure,
        None => format!("{error:?}"),
    }
}

/// What may be shown of a kubeconfig credential plugin's failure, when
/// `error` or anything in its `source()` chain is one - `None` otherwise.
///
/// kube's own text for it can't be shown, logged or reported: a failed run's
/// `Display` and `Debug` carry the plugin's whole captured output - stdout is
/// where a credential goes - and the `Command`'s `Debug`, which lists the env
/// the kubeconfig gives it; a parse failure can quote the output. This keeps
/// the exit status and the plugin's own stderr - where it says what went wrong,
/// such as an expired login - scrubbed of anything credential-like, and capped.
/// The failure reaches the client as `kube::Error::Service` wrapping the auth
/// layer's error, or as `kube::Error::Auth`: the chain walk finds either.
pub(crate) fn credential_plugin_failure(
    error: &(dyn std::error::Error + 'static),
) -> Option<String> {
    std::iter::successors(Some(error), |error| error.source())
        .take(ERROR_SOURCE_DEPTH_LIMIT)
        .find_map(|error| error.downcast_ref::<AuthError>())
        .and_then(plugin_failure)
}

fn plugin_failure(error: &AuthError) -> Option<String> {
    match error {
        AuthError::AuthExecRun { out, .. } => {
            let stderr = crate::util::redact::redact(String::from_utf8_lossy(&out.stderr).trim());
            let stderr = match stderr.char_indices().nth(CREDENTIAL_PLUGIN_STDERR_LIMIT) {
                Some((cut, _)) => format!("{}\u{2026}", &stderr[..cut]),
                None => stderr,
            };
            Some(if stderr.is_empty() {
                format!("The context's credential plugin failed ({}).", out.status)
            } else {
                format!(
                    "The context's credential plugin failed ({}): {stderr}",
                    out.status
                )
            })
        }
        AuthError::AuthExecParse(_) => {
            Some("The context's credential plugin's output isn't a credential.".to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{describe, detail};
    use kube::core::response::Status;

    fn api_error(reason: &str, message: &str, code: u16) -> kube::Error {
        kube::Error::Api(Box::new(Status {
            reason: reason.to_string(),
            message: message.to_string(),
            code,
            ..Default::default()
        }))
    }

    #[test]
    fn describe_reads_an_api_errors_reason_and_message() {
        let error = api_error("Forbidden", "pods is forbidden", 403);
        assert_eq!(describe(&error), "Forbidden: pods is forbidden");
    }

    /// A body kube couldn't parse reads as its HTTP status and what the server
    /// sent, not as kube's own "Failed to parse error data".
    #[test]
    fn describe_reads_an_unparsed_error_as_its_http_status_and_body() {
        let error = api_error("Failed to parse error data", "service unavailable\n", 503);
        assert_eq!(describe(&error), "HTTP 503: service unavailable");
    }

    #[test]
    fn describe_falls_back_to_display_for_non_api_errors() {
        let error = kube::Error::LinesCodecMaxLineLengthExceeded;
        assert_eq!(describe(&error), error.to_string());
    }

    #[test]
    fn detail_keeps_the_full_debug_rendering() {
        let error = api_error("NotFound", "pods \"gone\" not found", 404);
        assert_eq!(detail(&error), format!("{error:?}"));
    }

    /// Security pass: a failed credential plugin's run, the way kube reports
    /// it - a token on stdout, a secret in its env, a token echoed on stderr.
    #[cfg(unix)]
    fn failed_plugin() -> kube::client::AuthError {
        use std::os::unix::process::ExitStatusExt as _;
        kube::client::AuthError::AuthExecRun {
            cmd: r#"PLUGIN_SECRET="env-secret" "cred-plugin" "get-token""#.to_string(),
            out: Box::new(std::process::Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: br#"{"status":{"token":"stdout-token"}}"#.to_vec(),
                stderr: b"login expired; token=stderr-token\n".to_vec(),
            }),
        }
    }

    /// Whatever shape the failure arrives in - as the auth layer's error
    /// inside `Service`, or as `Auth` - neither the panel's message, its
    /// detail, nor a connect failure's reason carries the plugin's stdout,
    /// env or a token from its stderr; its explanation and exit status stay.
    #[cfg(unix)]
    #[test]
    fn a_failed_credential_plugin_shows_only_its_scrubbed_stderr() {
        let shapes = [
            kube::Error::Service(Box::new(failed_plugin())),
            kube::Error::Auth(failed_plugin()),
        ];
        for error in &shapes {
            let chain = crate::k8s::cluster::connection::error_chain(error);
            for text in [describe(error), detail(error), chain] {
                for secret in ["stdout-token", "env-secret", "stderr-token"] {
                    assert!(!text.contains(secret), "{secret} in {text:?}");
                }
                assert!(text.contains("login expired"), "{text:?}");
                assert!(text.contains("exit status: 1"), "{text:?}");
            }
        }
    }

    /// A plugin with nothing to say still names its exit status.
    #[cfg(unix)]
    #[test]
    fn a_silent_credential_plugin_names_its_exit_status() {
        use std::os::unix::process::ExitStatusExt as _;
        let error = kube::Error::Auth(kube::client::AuthError::AuthExecRun {
            cmd: "cred-plugin".to_string(),
            out: Box::new(std::process::Output {
                status: std::process::ExitStatus::from_raw(2 << 8),
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
        });
        assert_eq!(
            describe(&error),
            "The context's credential plugin failed (exit status: 2)."
        );
    }

    /// A cyclic `source()` chain ends the credential-plugin search instead of
    /// looping forever.
    #[test]
    fn a_cyclic_error_chain_has_no_credential_plugin_failure() {
        #[derive(Debug)]
        struct Cyclic;
        impl std::fmt::Display for Cyclic {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("cyclic")
            }
        }
        impl std::error::Error for Cyclic {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(self)
            }
        }
        assert_eq!(super::credential_plugin_failure(&Cyclic), None);
    }
}
