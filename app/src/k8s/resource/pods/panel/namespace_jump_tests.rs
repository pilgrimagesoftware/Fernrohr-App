//! Namespace quick-jump on a Pods panel, in a window with the app's keymap:
//! `alt-2` scopes the list to the second namespace in its namespace list,
//! `alt-9` - past the end - leaves the scope as it was, and `alt-0` shows all.

use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::resource::pods::test_window::open;
use gpui_kit::{AppContext as _, TestAppContext};

#[gpui_kit::test]
async fn a_position_jumps_to_that_namespace_and_past_the_end_does_nothing(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.vcx.update(|_, cx| {
        let list = cx.new(|_| NamespaceList::with_names(&["alpha", "beta", "gamma"]));
        harness.panel.update(cx, |panel, _| panel.namespaces = list);
    });
    let scope = |harness: &mut crate::k8s::resource::pods::test_window::Harness| {
        harness
            .vcx
            .update(|_, cx| harness.panel.read(cx).scope.namespaces.clone())
    };

    harness.press("alt-2");
    assert_eq!(scope(&mut harness), ["beta"], "the 2nd namespace");

    harness.press("alt-9");
    assert_eq!(scope(&mut harness), ["beta"], "past the end: unchanged");

    harness.press("alt-0");
    assert!(scope(&mut harness).is_empty(), "0 is all namespaces");
}
