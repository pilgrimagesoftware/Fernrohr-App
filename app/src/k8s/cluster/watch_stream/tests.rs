// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for these plain synchronous tests.
use crate::k8s::resource::pods::watch::is_unauthorized;
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
