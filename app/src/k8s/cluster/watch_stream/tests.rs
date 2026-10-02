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
