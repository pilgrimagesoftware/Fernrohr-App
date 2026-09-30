//! References in the rendered panel: a followable one is a link that
//! dispatches `FollowReference` when clicked, one with no viewer is plain text
//! that does nothing. `resource-links` 2.2 and 3.2's click half.

use super::fixtures::rich_pod;
use super::panel::stub_panel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::object_ref::ObjectRef;
use crate::ui::link::FollowReference;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use std::cell::RefCell;
use std::rc::Rc;

/// Every `FollowReference` that reaches the app, in order. The panel is the
/// window's root, so nothing in the element tree handles the action and it
/// bubbles to the app-level handler registered here - the same place it would
/// reach `MainWindow` in the real app.
fn record_follows(cx: &mut TestAppContext) -> Rc<RefCell<Vec<FollowReference>>> {
    let followed = Rc::new(RefCell::new(Vec::new()));
    let sink = followed.clone();
    cx.update(|cx| {
        cx.on_action(move |action: &FollowReference, _cx| {
            sink.borrow_mut().push(action.clone());
        });
    });
    followed
}

fn reference_id(field: &'static str, index: u64) -> ElementId {
    ElementId::NamedInteger(field.into(), index)
}

#[gpui_kit::test]
async fn a_namespace_reference_is_a_link_and_an_owner_with_no_viewer_is_not(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let followed = record_follows(cx);
    let window = stub_panel(cx, ConnectionState::Connecting);
    window
        .update(cx, |panel, _window, cx| {
            panel.test_set_loaded(rich_pod(), cx)
        })
        .unwrap();
    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(reference_id("Namespace", 0), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        *followed.borrow(),
        vec![FollowReference {
            context_name: "kind-dev".into(),
            target: ObjectRef::cluster_scoped("", "Namespace", "staging"),
        }],
        "clicking the namespace follows it, from the panel's own context"
    );

    // The owner is a ReplicaSet, which has no viewer yet: it is on screen,
    // and clicking it follows nothing.
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(reference_id("Controlled By", 0)).is_some());
        window.click(reference_id("Controlled By", 0), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        followed.borrow().len(),
        1,
        "a reference with no viewer is plain text"
    );
}
