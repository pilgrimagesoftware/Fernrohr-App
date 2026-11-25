use crate::k8s::cluster::watch_stream::{is_unauthorized, refusal};
use kube_runtime::watcher;

use kube::core::response::Status;

fn api_error(code: u16) -> kube::Error {
    kube::Error::Api(Box::new(Status {
        code,
        ..Default::default()
    }))
}

#[test]
fn watch_failed_401_is_unauthorized() {
    assert!(is_unauthorized(&watcher::Error::WatchFailed(api_error(
        401
    ))));
}

#[test]
fn initial_list_failed_401_is_unauthorized() {
    assert!(is_unauthorized(&watcher::Error::InitialListFailed(
        api_error(401)
    )));
}

#[test]
fn a_403_is_not_unauthorized() {
    assert!(!is_unauthorized(&watcher::Error::WatchFailed(api_error(
        403
    ))));
}

#[test]
fn no_resource_version_is_not_unauthorized() {
    assert!(!is_unauthorized(&watcher::Error::NoResourceVersion));
}

fn api_refusal(code: u16, message: &str) -> kube::Error {
    kube::Error::Api(Box::new(Status {
        code,
        message: message.into(),
        ..Default::default()
    }))
}

#[test]
fn a_403_on_the_initial_list_is_a_refusal_with_the_servers_message() {
    let message = "deployments.apps is forbidden: cannot list resource";
    assert_eq!(
        refusal(&watcher::Error::InitialListFailed(api_refusal(
            403, message
        ))),
        Some(message.to_string())
    );
    assert_eq!(
        refusal(&watcher::Error::WatchStartFailed(api_refusal(403, ""))),
        Some("Forbidden".to_string())
    );
}

#[test]
fn other_errors_are_not_refusals() {
    assert_eq!(
        refusal(&watcher::Error::InitialListFailed(api_error(401))),
        None
    );
    assert_eq!(
        refusal(&watcher::Error::InitialListFailed(api_error(500))),
        None
    );
    assert_eq!(refusal(&watcher::Error::NoResourceVersion), None);
}

/// Review follow-up: a watch failing on a credential plugin logs only what
/// `describe_any` keeps - not the plugin's stdout or the env it was given.
#[cfg(unix)]
#[test]
fn a_retry_note_keeps_a_credential_plugins_output_out() {
    use std::os::unix::process::ExitStatusExt as _;
    let plugin = kube::client::AuthError::AuthExecRun {
        cmd: r#"PLUGIN_ENV="env-marker" "cred-plugin""#.to_string(),
        out: Box::new(std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: br#"{"status":{"token":"stdout-marker"}}"#.to_vec(),
            stderr: b"login expired\n".to_vec(),
        }),
    };
    let error = kube_runtime::watcher::Error::WatchFailed(kube::Error::Auth(plugin));

    let note = super::retry_note("/api/v1/pods", 1, &error);

    for marker in ["stdout-marker", "env-marker"] {
        assert!(!note.contains(marker), "{marker} in {note:?}");
    }
    assert!(note.contains("login expired"), "{note:?}");
}
