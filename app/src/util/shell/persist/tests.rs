// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::util::context_lifecycle;
use crate::util::shell::test_support::*;
use crate::util::shell::{
    ClosedWindowLayouts, MainWindow, SavedDockLayouts, WindowLayout, WindowMode, WorkspaceConfig,
    config, init, open_saved_or_default, open_window, save,
};
use gpui_kit::component::dock::DockLayout;
use gpui_kit::{AppContext as _, TestAppContext, WindowId};
use std::collections::HashMap;

/// Task 2.2 supersedes design.md decision 3's non-goal: a multi-context window's
/// dock arrangement is now saved too, but under its own composite key
/// (`context_lifecycle::dock_layout_key`), never under either bare context name -
/// so a later single-context window on `staging` alone still starts from nothing,
/// not a stale arrangement built for `staging` plus `other`.
#[gpui_kit::test]
async fn multi_context_dock_layout_is_saved_under_its_own_composite_key(cx: &mut TestAppContext) {
    let solo = connected_window(cx, "kind-dev").await;
    cx.update(|cx| {
        cx.set_global(SavedDockLayouts(
            crate::config::dock_layouts::DockLayouts::default(),
        ));
    });
    cx.run_until_parked();

    let multi = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["staging".to_string(), "other".to_string()], window, cx);
        main_window
    });
    cx.run_until_parked();

    for window in [solo, multi] {
        window
            .update(cx, |main_window, window, cx| {
                let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                dock_area.update(cx, |area, cx| {
                    area.set_center(DockLayout::tabs(), window, cx);
                });
            })
            .unwrap();
    }
    cx.run_until_parked();

    let multi_key =
        context_lifecycle::dock_layout_key(&["staging".to_string(), "other".to_string()]);
    cx.update(|cx| {
        assert!(
            cx.global::<SavedDockLayouts>().0.get("kind-dev").is_some(),
            "the single-context window saves its layout under its bare context name"
        );
        assert!(
            cx.global::<SavedDockLayouts>().0.get("staging").is_none(),
            "a multi-context window must not save under either bare context name"
        );
        assert!(cx.global::<SavedDockLayouts>().0.get("other").is_none());
        assert!(
            cx.global::<SavedDockLayouts>().0.get(&multi_key).is_some(),
            "the multi-context window saves under its own composite key"
        );
    });
}

#[gpui_kit::test]
async fn quitting_persists_open_window_geometry(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    let keymap_path = temp_workspace_path();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, path.clone(), &keymap_path);
        open_window(
            cx,
            WindowLayout {
                width: 900.0,
                height: 700.0,
                x: Some(10.0),
                y: Some(20.0),
                contexts: Vec::new(),
                panels: Vec::new(),
                resource_panel_width: None,
            },
        );
    });
    cx.run_until_parked();

    cx.update(|cx| save(cx, &path));

    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows.len(), 1);
    assert_eq!(saved.windows[0].width, 900.0);
    assert_eq!(saved.windows[0].height, 700.0);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&keymap_path);
}

#[gpui_kit::test]
async fn save_persists_geometry_of_a_window_already_closed(cx: &mut TestAppContext) {
    // Regression test: under `QuitMode::LastWindowClosed`, `on_app_quit`
    // fires after every window is already gone, so `cx.windows()` alone
    // (the pre-fix implementation) sees nothing and silently saves an
    // empty layout. `save` must also pick up geometry captured by
    // `open_window`'s `on_window_should_close` hook and stashed in
    // `ClosedWindowLayouts` before the window disappeared.
    let path = temp_workspace_path();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        cx.set_global(ClosedWindowLayouts(HashMap::from([(
            WindowId::from(1),
            WindowLayout {
                width: 900.0,
                height: 700.0,
                x: Some(10.0),
                y: Some(20.0),
                contexts: Vec::new(),
                panels: Vec::new(),
                resource_panel_width: None,
            },
        )])));
        save(cx, &path);
    });

    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows.len(), 1);
    assert_eq!(saved.windows[0].width, 900.0);
    assert_eq!(saved.windows[0].height, 700.0);

    let _ = std::fs::remove_file(&path);
}

#[gpui_kit::test]
async fn corrupt_workspace_file_yields_one_default_window(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    std::fs::write(&path, "not valid toml {{{").unwrap();

    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        open_saved_or_default(cx, &path);
    });
    cx.run_until_parked();

    let window_count = cx.update(|cx| cx.windows().len());
    assert_eq!(window_count, 1);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "not valid toml {{{"
    );

    let _ = std::fs::remove_file(&path);
}

/// Task 2.2: a live workspace window's own `contexts` - not an empty list - is
/// what `save` persists, so relaunch can reconnect them directly (the spec's
/// "Multi-context window restored" scenario).
#[gpui_kit::test]
async fn saving_persists_a_live_workspaces_contexts(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });

    let layout = WindowLayout {
        contexts: vec!["kind-dev".to_string(), "staging".to_string()],
        panels: vec![pods_panel_descriptor("kind-dev")],
        ..Default::default()
    };
    cx.update(|cx| open_window(cx, layout));
    cx.run_until_parked();

    cx.update(|cx| save(cx, &path));

    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows.len(), 1);
    assert_eq!(
        saved.windows[0].contexts,
        vec!["kind-dev".to_string(), "staging".to_string()],
        "the live window's own contexts are saved, not an empty list"
    );

    let _ = std::fs::remove_file(&path);
}

/// Task 2.2: `save` reads a `Picker`-mode window's contexts as empty - there is
/// nothing to reconnect yet, so relaunch must not invent a context for it.
#[gpui_kit::test]
async fn saving_a_picker_mode_window_persists_no_contexts(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        open_window(cx, WindowLayout::default());
    });
    cx.run_until_parked();

    cx.update(|cx| save(cx, &path));

    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows.len(), 1);
    assert!(saved.windows[0].contexts.is_empty());

    let _ = std::fs::remove_file(&path);
}

/// Task 2.2: a multi-context window's dock arrangement round-trips under its own
/// composite key (`context_lifecycle::dock_layout_key`), order-insensitively - a
/// window restored with the same two contexts in the opposite order still finds
/// the saved arrangement rather than defaulting to a fresh single Pods panel.
#[gpui_kit::test]
async fn multi_context_dock_arrangement_round_trips_order_insensitively(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        cx.set_global(SavedDockLayouts(
            crate::config::dock_layouts::DockLayouts::default(),
        ));
    });

    let first = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(
            vec!["kind-dev".to_string(), "staging".to_string()],
            window,
            cx,
        );
        main_window
    });
    cx.run_until_parked();

    // Empty the centre dock, which `LayoutChanged` saves under the pair's
    // composite key regardless of the order they were added in.
    first
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            dock_area.update(cx, |area, cx| {
                area.set_center(DockLayout::tabs(), window, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();

    // A second window on the *same pair*, contexts reversed.
    let second = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(
            vec!["staging".to_string(), "kind-dev".to_string()],
            window,
            cx,
        );
        main_window
    });
    cx.run_until_parked();

    let _ = second.read_with(cx, |main_window, _cx| {
        let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
            panic!("a connected window is in workspace mode")
        };
        assert!(
            open_panels.is_empty(),
            "the saved (emptied) multi-context arrangement was loaded, not a \
             fresh default Pods panel"
        );
    });
}

/// Fernrohr#51: `save` writes main windows only. A Settings window open at
/// quit used to be written as a second, context-less main window, which the
/// next launch reopened as an empty picker.
#[gpui_kit::test]
async fn saving_skips_windows_that_are_not_main_windows(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    let keymap_path = temp_workspace_path();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, path.clone(), &keymap_path);
        open_window(cx, WindowLayout::default());
        crate::ui::settings::open_or_focus(cx);
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2, "main plus Settings");

    cx.update(|cx| save(cx, &path));

    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows.len(), 1, "only the main window is saved");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&keymap_path);
}

/// Fernrohr#51: resizing a window saves the layout once it has been still for
/// `BOUNDS_SAVE_DEBOUNCE`, without waiting for quit - so a kill or crash
/// keeps it. A second resize inside that window restarts the wait, and the
/// last size is the one written.
#[gpui_kit::test]
async fn resizing_saves_the_layout_once_the_window_settles(cx: &mut TestAppContext) {
    use crate::consts::BOUNDS_SAVE_DEBOUNCE;
    use gpui_kit::{VisualTestContext, px, size};
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    let keymap_path = temp_workspace_path();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, path.clone(), &keymap_path);
        open_window(cx, WindowLayout::default());
    });
    cx.run_until_parked();
    let handle = cx.update(|cx| cx.windows()[0]);
    let vcx = VisualTestContext::from_window(handle, cx);
    let saved_size = || {
        let saved: WorkspaceConfig = config::load(&path);
        saved
            .windows
            .first()
            .map(|window| (window.width, window.height))
    };

    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.executor().advance_clock(BOUNDS_SAVE_DEBOUNCE / 2);
    vcx.run_until_parked();
    vcx.simulate_resize(size(px(820.), px(610.)));
    vcx.executor().advance_clock(BOUNDS_SAVE_DEBOUNCE / 2);
    vcx.run_until_parked();
    assert_eq!(saved_size(), None, "nothing written while still resizing");

    vcx.executor().advance_clock(BOUNDS_SAVE_DEBOUNCE);
    vcx.run_until_parked();
    assert_eq!(
        saved_size(),
        Some((820.0, 610.0)),
        "the settled size is written"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&keymap_path);
}

mod launch;
mod split_restore;
