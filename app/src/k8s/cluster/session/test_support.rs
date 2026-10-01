//! Shared test-only helpers for `session`'s submodule test suites.

use gpui_kit::TestAppContext;
use kube::{Client, Config};

pub(super) fn test_client(cx: &mut TestAppContext) -> Client {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let _guard = handle.enter();
    Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
}
