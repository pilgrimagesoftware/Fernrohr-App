//! #247: a typed label panel starts over - by `ctrl-l`, its bar's reset
//! button, or the palette - stopping every stream, emptying its lines and its
//! selector field, and leaving the field focused for a new selector.

use super::{pod, wait_for_streams, window};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::test_cluster::FakeCluster;
use crate::util::logs::{LogsPanel, RESET_BUTTON, RESET_TOOLTIP, ShowLabelLogs};
use crate::util::shell::MainWindow;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

/// A window in `state` with the typed label panel opened as the palette
/// opens it, its field focused.
fn opened(
    state: ConnectionState,
    cx: &mut TestAppContext,
) -> (Entity<MainWindow>, Entity<LogsPanel>, VisualTestContext) {
    let (main, mut vcx) = window(state, cx);
    vcx.update(|window, cx| {
        let focus = main.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        window.dispatch_action(Box::new(ShowLabelLogs), cx)
    });
    vcx.run_until_parked();
    let panel = super::typed_panel(&main, &mut vcx);
    (main, panel, vcx)
}

/// A fake cluster holding `web-1`, and a typed panel streaming `app=web` from it.
fn streaming(cx: &mut TestAppContext) -> (FakeCluster, Entity<LogsPanel>, VisualTestContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "pods", pod("web-1", "web", 0));
    let (_main, panel, mut vcx) = opened(ConnectionState::Connected(client), cx);
    vcx.simulate_keystrokes("a p p = w e b enter");
    wait_for_streams(&mut vcx, &panel, &["web-1/app#0@uid-web-1"]);
    settle(&mut vcx, || !cluster.log_requests().is_empty());
    assert!(
        !cluster.log_requests().is_empty(),
        "web-1's log was asked for"
    );
    (cluster, panel, vcx)
}

/// Lets the fake cluster's runtime work - until `done`, or for a while.
fn settle(vcx: &mut VisualTestContext, done: impl Fn() -> bool) {
    for _ in 0..100 {
        vcx.run_until_parked();
        if done() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// The panel as it opened: no selector, error, filter, stream or line, and
/// the field empty and focused.
fn assert_started_over(vcx: &mut VisualTestContext, panel: &Entity<LogsPanel>) {
    let (selector, error, filter, streams, lines, field) = vcx.update(|window, cx| {
        let panel = panel.read(cx);
        (
            panel.test_selector(),
            panel.test_selector_error(),
            panel.test_container_filter(),
            panel.test_streams(),
            panel.test_line_count(cx),
            panel.test_selector_field(window, cx),
        )
    });
    assert_eq!(selector, None, "the selector is cleared");
    assert_eq!(error, None);
    assert_eq!(filter, None, "the container filter is back to all");
    assert!(streams.is_empty(), "every stream is dropped: {streams:?}");
    assert_eq!(lines, 0, "the view is empty");
    assert_eq!(
        field,
        Some((String::new(), true)),
        "the field is empty and focused"
    );
}

#[gpui_kit::test]
async fn ctrl_l_stops_every_stream_and_a_new_selector_starts_afresh(cx: &mut TestAppContext) {
    let (cluster, panel, mut vcx) = streaming(cx);
    vcx.update(|_, cx| panel.update(cx, |panel, cx| panel.test_append_line("a line", cx)));

    // From the selector field, where focus still is after Enter.
    vcx.simulate_keystrokes("ctrl-l");
    vcx.run_until_parked();
    assert_started_over(&mut vcx, &panel);

    // Pods that would have matched come and change: nothing streams them.
    let asked = cluster.log_requests().len();
    cluster.apply("/api/v1", "pods", pod("web-2", "web", 0));
    cluster.apply("/api/v1", "pods", pod("web-1", "web", 1));
    settle(&mut vcx, || false);
    assert_eq!(
        cluster.log_requests().len(),
        asked,
        "no log is asked for after the reset"
    );
    assert!(vcx.update(|_, cx| panel.read(cx).test_streams()).is_empty());

    vcx.simulate_keystrokes("a p p = w e b enter");
    wait_for_streams(
        &mut vcx,
        &panel,
        &["web-1/app#1@uid-web-1", "web-2/app#0@uid-web-2"],
    );
    settle(&mut vcx, || cluster.log_requests().len() > asked);
    assert!(
        cluster.log_requests().len() > asked,
        "the new selector streams"
    );
}

#[gpui_kit::test]
async fn the_bar_button_names_itself_and_starts_over(cx: &mut TestAppContext) {
    let (_cluster, panel, mut vcx) = streaming(cx);
    let at = vcx.update(|window, cx| {
        window.render_frame(cx);
        window.find(RESET_BUTTON).bounds().center()
    });
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    vcx.run_until_parked();
    let tooltip: &'static str = crate::ui::icon_tooltip::selector(RESET_TOOLTIP).leak();
    assert!(
        vcx.debug_bounds(tooltip).is_some(),
        "the button's tooltip shows"
    );

    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
    assert_started_over(&mut vcx, &panel);
}

/// Safe before the context connects: the selector, with no stream yet,
/// clears all the same.
#[gpui_kit::test]
async fn it_starts_over_while_still_connecting(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (_main, panel, mut vcx) = opened(ConnectionState::Connecting, cx);
    vcx.simulate_keystrokes("a p p = w e b enter");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| panel.read(cx).test_selector()),
        Some("app=web".to_string())
    );

    vcx.simulate_keystrokes("ctrl-l");
    vcx.run_until_parked();
    assert_started_over(&mut vcx, &panel);
}

/// The reset's key is its alone: no other command has it in any context -
/// stricter than `keymap::conflicts`, which counts two panel contexts as
/// separate whenever their text differs, as `TypedLabelLogs` and
/// `TypedLabelLogs && !Input` do though one panel has both.
#[test]
fn its_key_collides_with_nothing() {
    use crate::keymap::{KeymapConfig, conflicts};
    let mut registry = crate::command::CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    let command = registry
        .get("logs.reset_labels")
        .expect("the reset is registered");
    assert_eq!(command.context, Some("TypedLabelLogs"));
    let key = command.default_binding;
    let parsed = gpui_kit::Keystroke::parse(key).expect("the key parses");
    let sharing: Vec<&str> = registry
        .iter()
        .filter(|other| other.id != command.id)
        .filter(|other| {
            gpui_kit::Keystroke::parse(other.default_binding).ok() == Some(parsed.clone())
        })
        .map(|other| other.id)
        .collect();
    assert!(sharing.is_empty(), "{key} is also {sharing:?}'s");
    let found = conflicts(
        &registry,
        &KeymapConfig::default(),
        "logs.reset_labels",
        key,
    );
    assert!(
        !found.any_clash() && found.shadows.is_empty(),
        "{key} collides: {:?} / {} shadows / {} prefixes",
        found.same_scope,
        found.shadows.len(),
        found.prefixes.len()
    );
    // The check sees this key: a global command given it is shadowed by it.
    let other = conflicts(&registry, &KeymapConfig::default(), "nav.show_logs", key);
    assert_eq!(
        other.shadows.len(),
        1,
        "the reset shadows a global command on {key}"
    );
}
