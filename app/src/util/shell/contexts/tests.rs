// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::*;
use crate::util::shell::{MainWindow, NavTarget, WindowLayout, WindowMode, open_window};
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};

/// Section 3.2: adding a context holds it, makes it the active one, and opens
/// its Pods panel.
#[gpui_kit::test]
async fn add_context_holds_switches_active_and_opens_pods(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    assert_eq!(
        window_title(&window, cx).as_deref(),
        Some("kind-dev - Fernrohr")
    );

    window
        .update(cx, |main_window, window, cx| {
            main_window.add_context("staging".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        window_title(&window, cx).as_deref(),
        Some("2 clusters - Fernrohr")
    );

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace {
                contexts,
                active,
                open_panels,
                ..
            } = &main_window.mode
            else {
                panic!("a connected window is in workspace mode")
            };
            assert_eq!(
                contexts,
                &vec!["kind-dev".to_string(), "staging".to_string()]
            );
            assert_eq!(*active, 1, "the added context becomes active");
            assert!(
                open_panels
                    .iter()
                    .any(|open| open.key.context_name == "staging"
                        && open.key.target == NavTarget::pods()),
                "a Pods panel opened for the added context"
            );
        })
        .unwrap();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "staging")),
        1
    );
}

/// Adding a context already in use is a no-op - the "+" popover's own filtering
/// keeps this from happening interactively, but `add_context` must not duplicate
/// a chip if it's ever asked to anyway.
#[gpui_kit::test]
async fn adding_an_already_used_context_is_a_no_op(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            main_window.add_context("kind-dev".to_string(), window, cx);
        })
        .unwrap();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { contexts, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            assert_eq!(contexts, &vec!["kind-dev".to_string()]);
        })
        .unwrap();
}

/// `cluster-connection`'s "Shared by two windows": a second window adding a
/// context the first window already holds must not start a second session.
#[gpui_kit::test]
async fn adding_a_context_another_window_holds_reuses_its_session(cx: &mut TestAppContext) {
    // Not two `connected_window` calls: `gpui_kit::init`/`crate::runtime::init`
    // are meant to run once per test, so the second window is built inline the
    // same way `closing_a_window_releases_only_its_own_hold` builds its second
    // window.
    // Kept alive for the test's duration: it's the window whose hold on
    // `kind-dev` the assertion below expects to still be counted.
    let _first = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let second = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["staging".to_string()], window, cx);
        main_window
    });
    cx.run_until_parked();

    let session_before = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());

    second
        .update(cx, |main_window, window, cx| {
            main_window.add_context("kind-dev".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    let session_after = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());
    assert_eq!(
        session_before, session_after,
        "adding an already-connected context must not start a second session"
    );
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        2,
        "both windows now hold kind-dev"
    );
}

/// Section 3.3: disconnecting a context closes its panels, drops it from
/// `contexts`, and releases this window's hold. Panels for the window's other
/// context are untouched.
#[gpui_kit::test]
async fn disconnect_context_closes_its_panels_and_releases_the_hold(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    window
        .update(cx, |main_window, window, cx| {
            main_window.add_context("staging".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        window_title(&window, cx).as_deref(),
        Some("2 clusters - Fernrohr")
    );

    window
        .update(cx, |main_window, window, cx| {
            main_window.disconnect_context("staging".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    // Back to one context, so the title names the survivor.
    assert_eq!(
        window_title(&window, cx).as_deref(),
        Some("kind-dev - Fernrohr")
    );

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace {
                contexts,
                active,
                open_panels,
                ..
            } = &main_window.mode
            else {
                panic!("kind-dev's own panel keeps this window in workspace mode")
            };
            assert_eq!(contexts, &vec!["kind-dev".to_string()]);
            assert_eq!(*active, 0);
            assert!(
                open_panels
                    .iter()
                    .all(|open| open.key.context_name != "staging"),
                "every staging panel closed"
            );
            assert!(
                open_panels
                    .iter()
                    .any(|open| open.key.context_name == "kind-dev"),
                "kind-dev's own panel is untouched"
            );
        })
        .unwrap();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "staging")),
        0,
        "this window was staging's only holder"
    );
}

/// Section 3.3's "Last context": disconnecting a window's only context returns
/// it to the cluster picker, and a context another window still holds stays up
/// for it (`cluster-connection`'s "One window lets go").
#[gpui_kit::test]
async fn disconnecting_the_last_context_returns_to_the_picker(cx: &mut TestAppContext) {
    // See the note in `adding_a_context_another_window_holds_reuses_its_session`
    // on why the second window is built inline rather than through a second
    // `connected_window` call.
    let first = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let second = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
        main_window
    });
    cx.run_until_parked();

    second
        .update(cx, |main_window, window, cx| {
            main_window.disconnect_context("kind-dev".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(window_title(&second, cx).as_deref(), Some("Fernrohr"));
    assert_eq!(
        window_title(&first, cx).as_deref(),
        Some("kind-dev - Fernrohr")
    );

    second
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Picker(_)),
                "the window's last context disconnected"
            );
        })
        .unwrap();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        1,
        "the other window still holds kind-dev"
    );

    first
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Workspace { .. }),
                "the other window's own workspace is unaffected"
            );
        })
        .unwrap();
}

/// Design.md decision 4: a chip click (or the Resource panel's own dropdown)
/// sets `active`, and every child - the Resource panel, the status bar, the
/// context bar - is pushed the same update through `sync_context_children`.
#[gpui_kit::test]
async fn set_active_context_switches_active_and_syncs_children(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    window
        .update(cx, |main_window, window, cx| {
            main_window.add_context("staging".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            main_window.set_active_context("kind-dev", window, cx);
        })
        .unwrap();

    window
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Workspace {
                active, status_bar, ..
            } = &main_window.mode
            else {
                panic!("a connected window is in workspace mode")
            };
            assert_eq!(*active, 0, "switched back to kind-dev");
            assert_eq!(
                status_bar.read(cx).items(cx).len(),
                2,
                "the status bar still lists both contexts - only `active` moved"
            );
        })
        .unwrap();
}

/// A request for a context this window doesn't use (a stale event racing a
/// disconnect) must not touch `active` at all.
#[gpui_kit::test]
async fn set_active_context_to_an_unused_context_is_a_no_op(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            main_window.set_active_context("never-added", window, cx);
        })
        .unwrap();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { active, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            assert_eq!(*active, 0);
        })
        .unwrap();
}

/// `window-title-and-menu` 4.2, the `app-shell` spec's "Restored windows come
/// back titled": a window restored holding several contexts is titled with
/// their count from its first frame - read before anything runs, so the title
/// comes from `open_window`'s `TitlebarOptions`, not a later re-title.
#[gpui_kit::test]
async fn a_restored_multi_context_window_is_titled_from_its_first_frame(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let layout = WindowLayout {
        contexts: vec!["kind-dev".to_string(), "staging".to_string()],
        panels: vec![
            pods_panel_descriptor("kind-dev"),
            pods_panel_descriptor("staging"),
        ],
        ..Default::default()
    };
    cx.update(|cx| open_window(cx, layout));
    let window = cx.update(|cx| cx.windows()[0]);
    let first_frame = VisualTestContext::from_window(window, cx).window_title();
    assert_eq!(first_frame.as_deref(), Some("2 clusters - Fernrohr"));

    cx.run_until_parked();
    let settled = VisualTestContext::from_window(window, cx).window_title();
    assert_eq!(settled.as_deref(), Some("2 clusters - Fernrohr"));
}
