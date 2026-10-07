// Not `use super::*`: `logs.rs` pulls in `gpui_kit::*`, whose own `test`
// attribute macro would shadow `core::prelude::v1::test` for the plain
// synchronous tests below.
use super::LogsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::{AppContext as _, TestAppContext};

mod previous;
mod restore;
mod selection;

/// `1-window-context-bar` bug 1's third root cause: `LogsPanel::sync` used to
/// re-sync to *any* `SelectedPod`, from any context - streaming whichever pod's
/// row was clicked last against its own scope's context, regardless of which
/// context actually held that pod. A selection published by a different
/// context must leave this panel exactly as it was: nothing selected, nothing
/// streamed.
///
/// `ConnectionState::Connecting` rather than `Connected`: the context check
/// runs (and must reject the foreign selection) before `sync` ever looks at
/// the connection, so nothing here needs a real client or spawns a stream
/// task for the test to outlive.
#[gpui_kit::test]
async fn a_logs_panel_ignores_a_selection_from_a_different_context(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "a", ConnectionState::Connecting);
    });

    let panel = cx.update(|cx| {
        cx.new(|cx| LogsPanel::new(PanelScope::new(NavTarget::Logs, "a".to_string()), cx))
    });

    cx.update(|cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "b".into(),
        })));
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, _| {
        assert!(
            panel.current.is_none(),
            "a selection from a different context must not be streamed here"
        );
    });
}

/// A click inside the panel focuses it - the focus its tab's underline follows.
/// Without `track_focus` on the body a click landed nowhere, and the panel
/// could never be marked as the focused one.
#[gpui_kit::test]
async fn a_click_focuses_the_logs_panel(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "a", ConnectionState::Connecting);
    });
    let (panel, cx) = cx.add_window_view(|_window, cx| {
        LogsPanel::new(PanelScope::new(NavTarget::Logs, "a".to_string()), cx)
    });
    cx.run_until_parked();
    let focused = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|window, cx| panel.read(cx).focus_handle.contains_focused(window, cx))
    };
    assert!(!focused(cx), "nothing has focused the panel yet");

    cx.simulate_click(
        gpui_kit::point(gpui_kit::px(200.), gpui_kit::px(200.)),
        gpui_kit::Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(focused(cx), "the click focuses the panel");
}
