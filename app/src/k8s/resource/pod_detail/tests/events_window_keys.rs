//! `pod-events-time-window` 2.2: the Events tab's window selector, by keyboard and
//! by click, in a pod detail panel with the app's own keymap - and the choice
//! saved to `ui.toml`, so a relaunch's new panels start on it.

use crate::config::ui::{PodEventsWindow, UiConfig};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::resource::pod_detail::{DetailView, PodDetailPanel, window_preference};
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, Focusable as _, Keystroke, Modifiers, TestAppContext,
    VisualTestContext,
};

fn temp_path() -> std::path::PathBuf {
    crate::util::test_paths::temp_path("pod-events-window")
}

fn panel_window(
    cx: &mut TestAppContext,
    ui: &std::path::Path,
) -> (VisualTestContext, Entity<PodDetailPanel>) {
    cx.executor().allow_parking();
    let stored: UiConfig = crate::config::load(ui);
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, temp_path(), &temp_path());
        window_preference::init(stored.pod_events_window, ui.to_path_buf(), cx);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let connection =
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting));
        let pod = PodRef {
            namespace: "shop".into(),
            name: "web-1".into(),
        };
        let scope = PanelScope::new(NavTarget::pod("shop", "web-1"), "demo".into());
        let panel = cx.new(|cx| {
            PodDetailPanel::with_connection(pod, scope, DetailView::Structured, connection, cx)
        });
        let focus = panel.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    (vcx, built.expect("the window built its panel"))
}

fn press(vcx: &mut VisualTestContext, key: &str) {
    vcx.simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
    vcx.run_until_parked();
}

fn window_of(vcx: &mut VisualTestContext, panel: &Entity<PodDetailPanel>) -> PodEventsWindow {
    vcx.update(|_, cx| panel.read(cx).events_window)
}

#[gpui_kit::test]
async fn the_window_changes_by_key_and_click_and_survives_a_relaunch(cx: &mut TestAppContext) {
    let ui = temp_path();
    let (mut vcx, panel) = panel_window(cx, &ui);
    assert_eq!(
        window_of(&mut vcx, &panel),
        PodEventsWindow::Hour1,
        "the default"
    );

    press(&mut vcx, "5");
    press(&mut vcx, "]");
    assert_eq!(
        window_of(&mut vcx, &panel),
        PodEventsWindow::Hours6,
        "] is longer"
    );
    press(&mut vcx, "[");
    press(&mut vcx, "[");
    assert_eq!(
        window_of(&mut vcx, &panel),
        PodEventsWindow::Minutes15,
        "[ is shorter"
    );

    // The tab draws once the pod is in; this stub connection never fetches it.
    vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.state = crate::k8s::resource::pod_detail::fetch::PodDetailState::Loaded(
                Box::new(super::fixtures::rich_pod()),
            );
            cx.notify();
        })
    });
    let button = vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find("pod-events-window-24 hours")
            .expect("the selector is drawn on the Events tab")
            .bounds()
            .center()
    });
    vcx.simulate_click(button, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(
        window_of(&mut vcx, &panel),
        PodEventsWindow::Hours24,
        "a click picks it"
    );

    let saved: UiConfig = crate::config::load(&ui);
    assert_eq!(
        saved.pod_events_window,
        PodEventsWindow::Hours24,
        "saved to ui.toml"
    );

    // The next launch reads it, and a new panel starts on it.
    let mut relaunch = TestAppContext::single();
    let (mut vcx, panel) = panel_window(&mut relaunch, &ui);
    assert_eq!(window_of(&mut vcx, &panel), PodEventsWindow::Hours24);
}
