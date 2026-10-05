//! View Logs (`l`): before the pod loads it does nothing, and its key never
//! fires inside a text field. Opening Logs from a loaded panel is the window's
//! test (`util::shell::pod_detail_logs_tests`).

use super::panel::{registered_bindings, stub_panel};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::resource::pod_detail::commands::{PANEL_KEY_CONTEXT, ViewLogs};
use crate::k8s::resource::pods::SelectedPod;
use crate::keymap::KeymapConfig;
use crate::ui::nav::ShowLogs;
use gpui_kit::{
    Context, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    TestAppContext, VisualTestContext, Window, div,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts every `ShowLogs` that reaches the app - the window would turn each
/// into an open (or focus) of the Logs panel.
fn count_show_logs(cx: &mut TestAppContext) -> Arc<AtomicUsize> {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        cx.bind_keys(registered_bindings(&KeymapConfig::default(), cx));
        cx.on_action(move |_: &ShowLogs, _| {
            seen.fetch_add(1, Ordering::SeqCst);
        });
    });
    count
}

/// 2.1: before the pod has loaded there is nothing to show logs for.
#[gpui_kit::test]
async fn l_before_the_pod_loads_does_nothing(cx: &mut TestAppContext) {
    let shown = count_show_logs(cx);
    let window = stub_panel(cx, ConnectionState::Connecting);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |panel, window, cx| {
            panel.focus_handle.clone().focus(window, cx)
        })
        .unwrap();

    vcx.simulate_keystrokes("l");
    vcx.run_until_parked();

    assert_eq!(shown.load(Ordering::SeqCst), 0);
    assert!(
        vcx.update(|_, cx| cx.try_global::<SelectedPod>().and_then(|s| s.0.clone()))
            .is_none()
    );
}

/// A text field inside the panel's context: the key context stack a filter or
/// editor in the panel would give.
struct FieldInPanel {
    panel: FocusHandle,
    field: FocusHandle,
}

impl Render for FieldInPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(PANEL_KEY_CONTEXT)
            .track_focus(&self.panel)
            .child(div().key_context("Input").track_focus(&self.field))
    }
}

/// The k9s review's lesson: `l` is bound outside text fields, so typing an
/// `l` into one never opens Logs; on the panel itself it does.
#[gpui_kit::test]
async fn l_does_not_fire_inside_a_text_field(cx: &mut TestAppContext) {
    let fired = Arc::new(AtomicUsize::new(0));
    let seen = fired.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.bind_keys(registered_bindings(&KeymapConfig::default(), cx));
        cx.on_action(move |_: &ViewLogs, _| {
            seen.fetch_add(1, Ordering::SeqCst);
        });
    });
    let window = cx.add_window(|_, cx| FieldInPanel {
        panel: cx.focus_handle(),
        field: cx.focus_handle(),
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let focus = |vcx: &mut VisualTestContext, field: bool| {
        window
            .update(vcx, |view, window, cx| {
                let handle = if field { &view.field } else { &view.panel };
                handle.clone().focus(window, cx);
            })
            .unwrap();
        vcx.run_until_parked();
    };

    focus(&mut vcx, true);
    vcx.simulate_keystrokes("l");
    vcx.run_until_parked();
    assert_eq!(fired.load(Ordering::SeqCst), 0, "typed, not View Logs");

    focus(&mut vcx, false);
    vcx.simulate_keystrokes("l");
    vcx.run_until_parked();
    assert_eq!(fired.load(Ordering::SeqCst), 1, "on the panel, View Logs");
}
