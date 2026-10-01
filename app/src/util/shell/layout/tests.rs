// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::*;
use crate::util::shell::{
    ClosedWindowLayouts, MainWindow, PanelDescriptor, WindowLayout, WindowMode, WorkspaceConfig,
    close_window, config, init, open_window, restorable_panels, restored_contexts,
    restored_resource_width, save,
};
use gpui_kit::{AppContext as _, TestAppContext};

#[test]
fn restorable_panels_skips_unknown_kinds() {
    let layout = WindowLayout {
        panels: vec![
            PanelDescriptor::Unknown,
            PanelDescriptor::Pods {
                cluster_context: "kind-dev".into(),
                namespace: crate::config::workspace::NamespaceScope::All,
                filter: String::new(),
                sort: crate::config::workspace::SortState {
                    column: "name".into(),
                    ascending: true,
                },
            },
            PanelDescriptor::Unknown,
        ],
        ..Default::default()
    };

    let kept = restorable_panels(&layout);

    assert_eq!(kept.len(), 1);
    assert!(matches!(kept[0], PanelDescriptor::Pods { .. }));
}

/// Tasks.md 2.1: an explicit `contexts` list wins outright, whatever the saved
/// panels say - a window that explicitly holds a context with no panels (yet)
/// must not lose it to derivation.
#[test]
fn restored_contexts_prefers_the_explicit_list_over_derivation() {
    let layout = WindowLayout {
        contexts: vec!["kind-dev".to_string(), "staging".to_string()],
        panels: vec![pods_panel_descriptor("kind-dev")],
        ..Default::default()
    };

    assert_eq!(
        restored_contexts(&layout),
        vec!["kind-dev".to_string(), "staging".to_string()]
    );
}

/// Tasks.md 2.1 / design.md decision 3: a legacy file with no `contexts` derives
/// the list from its saved panels' `cluster_context`s, first-seen order, with no
/// duplicates for two panels on the same context.
#[test]
fn restored_contexts_derives_from_panels_in_first_seen_order_when_absent() {
    let layout = WindowLayout {
        contexts: Vec::new(),
        panels: vec![
            pods_panel_descriptor("staging"),
            pods_panel_descriptor("kind-dev"),
            pods_panel_descriptor("staging"),
        ],
        ..Default::default()
    };

    assert_eq!(
        restored_contexts(&layout),
        vec!["staging".to_string(), "kind-dev".to_string()]
    );
}

/// A layout with neither an explicit list nor any restorable panel derives to
/// nothing - that window opens in `Picker` mode, not a workspace with no context.
#[test]
fn restored_contexts_is_empty_for_a_layout_with_no_panels() {
    assert!(restored_contexts(&WindowLayout::default()).is_empty());
}

/// Tasks.md 1.3: entering a workspace takes this window's hold, and closing the
/// window releases it - two windows on one context share the session until both
/// close, matching `ClusterRegistry`'s own hold/release tests one layer down.
#[gpui_kit::test]
async fn closing_a_window_releases_only_its_own_hold(cx: &mut TestAppContext) {
    let workspace = temp_workspace_path();
    let keymap = temp_workspace_path();
    // See `connected_window`'s doc comment: `enter_workspace` starts a real
    // connect whose completion wakes GPUI from a tokio thread.
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        // `init` registers the `on_window_closed` hook that releases a closed
        // window's holds - the thing under test.
        init(cx, workspace.clone(), &keymap);
    });

    fn open(cx: &mut TestAppContext) -> gpui_kit::WindowHandle<MainWindow> {
        cx.add_window(|window, cx| {
            let mut main_window = MainWindow {
                mode: WindowMode::Picker(
                    cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
                ),
                focus_handle: cx.focus_handle(),
            };
            main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
            main_window
        })
    }

    let window_a = open(cx);
    let window_b = open(cx);
    cx.run_until_parked();

    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        2,
        "both windows took a hold on entering their workspace"
    );

    window_a
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        1,
        "closing one window must release only its own hold"
    );

    window_b
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        0,
        "closing the last window must release the session entirely"
    );

    let _ = std::fs::remove_file(&workspace);
    let _ = std::fs::remove_file(&keymap);
}

/// A window closed while another stays open is forgotten; only the last one to close
/// is remembered (closing it quits the app), so relaunch reopens what was open at
/// quit - not every window ever closed this run.
#[gpui_kit::test]
async fn closing_a_window_while_others_stay_open_forgets_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    let keymap_path = temp_workspace_path();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, path.clone(), &keymap_path);
        open_window(cx, WindowLayout::default());
        open_window(cx, WindowLayout::default());
    });
    cx.run_until_parked();

    let windows = cx.update(|cx| cx.windows());
    assert_eq!(windows.len(), 2);
    windows[0]
        .update(cx, |_, window, cx| close_window(window, cx))
        .unwrap();
    cx.run_until_parked();
    let recorded = cx.update(|cx| {
        cx.try_global::<ClosedWindowLayouts>()
            .map_or(0, |closed| closed.0.len())
    });
    assert_eq!(
        recorded, 0,
        "a window closed while another is open is forgotten"
    );

    windows[1]
        .update(cx, |_, window, cx| close_window(window, cx))
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| save(cx, &path));
    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(
        saved.windows.len(),
        1,
        "only the last window closed is restored"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&keymap_path);
}

/// A saved width outside the divider's range is clamped, and a layout with none (an
/// older file) starts at the default.
#[test]
fn a_restored_resource_panel_width_is_clamped_or_defaulted() {
    use crate::consts::{RESOURCE_PANEL_MAX_WIDTH, RESOURCE_PANEL_MIN_WIDTH, RESOURCE_PANEL_WIDTH};
    let with = |width: Option<f32>| WindowLayout {
        resource_panel_width: width,
        ..Default::default()
    };
    assert_eq!(
        restored_resource_width(&with(Some(9999.0))),
        RESOURCE_PANEL_MAX_WIDTH
    );
    assert_eq!(
        restored_resource_width(&with(Some(10.0))),
        RESOURCE_PANEL_MIN_WIDTH
    );
    assert_eq!(restored_resource_width(&with(None)), RESOURCE_PANEL_WIDTH);
}

/// Fernrohr#51: a save records the frame's origin with the *content* size. On
/// macOS `window.bounds()` is the outer frame, 32px taller than the content
/// here, and `open_window` sizes the content area - so saving the frame's own
/// size grew the window by the title bar on every relaunch.
#[test]
fn restorable_bounds_keeps_the_frame_origin_and_the_content_size() {
    use gpui_kit::{Bounds, point, px, size};
    let frame = Bounds::new(point(px(300.), px(200.)), size(px(1000.), px(792.)));
    let content = size(px(1000.), px(760.));
    assert_eq!(
        super::restorable_bounds(frame, content),
        Bounds::new(point(px(300.), px(200.)), size(px(1000.), px(760.)))
    );
}
