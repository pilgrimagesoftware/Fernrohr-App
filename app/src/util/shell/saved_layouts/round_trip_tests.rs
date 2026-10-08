//! End-to-end round trip through the real commands and keymap (tasks 8.1,
//! 8.2): save a layout, reload it from disk the way a restart would, then
//! load it back with Replace and with Add. `tests.rs` covers `layouts.save`
//! alone and `load/tests.rs` covers `load_replace`/`load_add` alone, each
//! against a hand-built or single-window fixture; this file is the one place
//! that drives the whole `save -> reload from disk -> load` path a user
//! actually takes, including (8.2) a saved layout produced by a *second*,
//! throwaway window going through the real `layouts.save` command rather
//! than a dumped `DockAreaState`.
//!
//! Its own `Harness`/helpers, not a reuse of `tests.rs`'s or
//! `manage_tests.rs`'s: both of those build a single-context window, and
//! this file's 8.1 case needs two contexts with one panel each - a shape
//! neither existing harness's `harness(cx, picker, second_panel)` shape
//! supports without widening it for a single call site.

use crate::config::saved_layouts;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::confirm_dialog;
use crate::ui::nav::NavTarget;
use crate::ui::nav::OpenMode;
use crate::util::shell::test_support::{press, temp_layouts_dir, temp_workspace_path};
use crate::util::shell::{MainWindow, SavedLayoutsDir, WindowMode, init, restored_panel_keys};
use gpui_kit::component::Root;
use gpui_kit::component::dock::DockAreaState;
use gpui_kit::{
    AppContext as _, Entity, Pixels, Size, TestAppContext, VisualTestContext, WindowHandle,
};
use kube::core::GroupVersionKind;
use std::path::PathBuf;

struct Harness {
    main: Entity<MainWindow>,
    /// Kept alive for the window's own duration; no test reads it back.
    _window: WindowHandle<Root>,
    layouts_dir: PathBuf,
    vcx: VisualTestContext,
}

fn events_kind() -> DiscoveredKind {
    DiscoveredKind::events()
}

/// A kind distinct from Pods and Events, for the "change the arrangement"
/// step - any discovered kind would do; Services is what `saved_layouts::
/// tests.rs`'s own two-panel fixture already uses.
fn services_kind() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// A window on every context in `contexts` (each pre-seeded with its own
/// `ClusterRegistry` test session), opened on the workspace default (Pods on
/// the first context) plus one `open_target_in` call per `(target,
/// context)` pair in `extra_targets` - so a caller can place a second panel
/// on a *different* held context, which `test_workspace` alone cannot do.
/// Shares `layouts_dir` across every `harness` call in one test, rather than
/// each minting its own, so a layout saved from one window is visible to
/// another.
fn harness(
    cx: &mut TestAppContext,
    contexts: Vec<String>,
    layouts_dir: PathBuf,
    extra_targets: Vec<(NavTarget, String)>,
) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        cx.set_global(SavedLayoutsDir(layouts_dir.clone()));
        for context in &contexts {
            ClusterRegistry::insert_test_session(cx, context, ConnectionState::Connecting);
        }
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| {
            let mut main = MainWindow::test_workspace(contexts.clone(), window, cx);
            for (target, context) in &extra_targets {
                main.open_target_in(
                    target.clone(),
                    None,
                    Some(context.clone()),
                    Vec::new(),
                    OpenMode::Foreground,
                    window,
                    cx,
                );
            }
            main
        });
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its MainWindow");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| main.update(cx, |main, cx| main.focus_initial(window, cx)));
    vcx.run_until_parked();
    Harness {
        main,
        _window: window,
        layouts_dir,
        vcx,
    }
}

fn dialog_open(h: &mut Harness) -> bool {
    h.vcx
        .update(gpui_kit::component::WindowExt::has_active_dialog)
}

fn type_text(h: &mut Harness, text: &str) {
    h.vcx.simulate_input(text);
    h.vcx.run_until_parked();
}

/// Opens the picker via its default binding and delivers its first frame -
/// see `manage_tests.rs::open_picker`'s own doc comment for why the second
/// call is needed before a keystroke can reach the picker's own rows.
fn open_picker(h: &mut Harness) {
    press(&mut h.vcx, "secondary-shift-o");
    confirm_dialog::deliver_first_frame(&mut h.vcx);
}

fn open_targets(h: &mut Harness) -> Vec<NavTarget> {
    let main = h.main.clone();
    h.vcx
        .update(move |_window, cx| main.read(cx).test_open_targets())
}

fn resource_width(h: &mut Harness) -> Option<Pixels> {
    let main = h.main.clone();
    h.vcx
        .update(move |_window, cx| main.read(cx).resource_width())
}

fn window_size(h: &mut Harness) -> Size<Pixels> {
    h.vcx.update(|window, _cx| window.bounds().size)
}

/// Every panel currently in the dock, decoded to a `PanelKey` and formatted
/// (`Debug`, since `PanelKey` is `pub(super)` to `util::shell` and this
/// module has no reason to name the type itself), sorted so two layouts with
/// the same panels in a different tree order still compare equal.
fn panel_key_strings(h: &mut Harness) -> Vec<String> {
    let main = h.main.clone();
    let mut keys = h.vcx.update(move |_window, cx| {
        let WindowMode::Workspace { dock_area, .. } = &main.read(cx).mode else {
            panic!("a connected window is in workspace mode")
        };
        let dock = dock_area.read(cx).dump(cx);
        restored_panel_keys(&dock.center)
            .into_iter()
            .flatten()
            .map(|key| format!("{key:?}"))
            .collect::<Vec<_>>()
    });
    keys.sort();
    keys
}

/// As [`panel_key_strings`], but decoding a `SavedLayout`'s own dock rather
/// than a live window's - the shape `4.3`'s own test already proved decodes
/// identically either way.
fn saved_panel_key_strings(dock: &DockAreaState) -> Vec<String> {
    let mut keys: Vec<String> = restored_panel_keys(&dock.center)
        .into_iter()
        .flatten()
        .map(|key| format!("{key:?}"))
        .collect();
    keys.sort();
    keys
}

/// 8.1: save a layout from a window with two contexts and one panel in
/// each, reload `SavedLayout`s from disk (simulating a restart), change the
/// window's arrangement, then load the reloaded layout back with Replace -
/// the window's panels (and their contexts), Resource panel width, and size
/// match what was saved, not the changed arrangement.
#[gpui_kit::test]
async fn saving_then_reloading_from_disk_then_replacing_restores_two_contexts(
    cx: &mut TestAppContext,
) {
    let layouts_dir = temp_layouts_dir();
    let mut h = harness(
        cx,
        vec!["demo".into(), "demo2".into()],
        layouts_dir,
        vec![(NavTarget::Kind(events_kind()), "demo2".to_string())],
    );
    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods(), NavTarget::Kind(events_kind())],
        "the window starts with one panel per context"
    );

    press(&mut h.vcx, "secondary-shift-s");
    type_text(&mut h, "Two Context Layout");
    press(&mut h.vcx, "enter");
    assert!(!dialog_open(&mut h), "saving closes the naming dialog");

    // Simulate a restart: read the layouts back from disk rather than from
    // any in-memory state the save just left behind.
    let (layouts, unreadable) = saved_layouts::load_all(&h.layouts_dir);
    assert!(unreadable.is_empty(), "no unreadable layout files");
    let reloaded = layouts
        .into_iter()
        .find(|layout| layout.name == "Two Context Layout")
        .expect("the saved layout reads back after a simulated restart");
    let expected_keys = saved_panel_key_strings(&reloaded.dock);
    let expected_width = reloaded.resource_panel_width;
    let expected_size = gpui_kit::size(
        gpui_kit::px(reloaded.window_width),
        gpui_kit::px(reloaded.window_height),
    );

    // Change the window's arrangement: a third panel, a different Resource
    // panel width, and a different window size.
    let main = h.main.clone();
    h.vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            main.open_target_in(
                NavTarget::Kind(services_kind()),
                None,
                Some("demo".to_string()),
                Vec::new(),
                OpenMode::Foreground,
                window,
                cx,
            );
            if let WindowMode::Workspace { resource_width, .. } = &mut main.mode {
                *resource_width = gpui_kit::px(111.0);
            }
        });
        window.resize(gpui_kit::size(gpui_kit::px(640.0), gpui_kit::px(480.0)));
    });
    h.vcx.run_until_parked();
    assert_eq!(
        open_targets(&mut h).len(),
        3,
        "the arrangement now differs from what was saved"
    );

    open_picker(&mut h);
    press(&mut h.vcx, "enter");

    assert_eq!(
        panel_key_strings(&mut h),
        expected_keys,
        "Replace restores exactly the saved panels, each in its saved context"
    );
    assert_eq!(
        resource_width(&mut h),
        expected_width.map(gpui_kit::px),
        "Replace applies the saved Resource panel width"
    );
    assert_eq!(
        window_size(&mut h),
        expected_size,
        "Replace applies the saved window size"
    );
}

/// 8.2: a saved layout produced by a *second*, throwaway window going
/// through the real `layouts.save` command (not a hand-built fixture),
/// holding a different panel (Events) plus one with the same content key
/// (Pods) as the window under test's single open panel. Loading it with Add
/// shows both the original and the new panel, no duplicate, with Resource
/// panel width and window size unchanged.
#[gpui_kit::test]
async fn loading_with_add_merges_a_layout_saved_through_the_real_save_command(
    cx: &mut TestAppContext,
) {
    let layouts_dir = temp_layouts_dir();

    // The saved layout's own source window: Pods (the workspace default,
    // the same content key the window under test will already have open)
    // plus Events (a different panel), saved for real.
    let mut saver = harness(
        cx,
        vec!["demo".into()],
        layouts_dir.clone(),
        vec![(NavTarget::Kind(events_kind()), "demo".to_string())],
    );
    press(&mut saver.vcx, "secondary-shift-s");
    type_text(&mut saver, "Mixed Layout");
    press(&mut saver.vcx, "enter");
    assert!(!dialog_open(&mut saver), "saving closes the naming dialog");

    // The window under test: one panel (Pods, the workspace default).
    let mut h = harness(cx, vec!["demo".into()], layouts_dir, Vec::new());
    assert_eq!(open_targets(&mut h), vec![NavTarget::pods()]);
    let width_before = resource_width(&mut h);
    let size_before = window_size(&mut h);

    open_picker(&mut h);
    press(&mut h.vcx, "secondary-enter");

    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods(), NavTarget::Kind(events_kind())],
        "Add shows the original panel plus the saved layout's new one, with no \
         duplicate for the matching Pods content key"
    );
    assert_eq!(
        resource_width(&mut h),
        width_before,
        "Add leaves the Resource panel width untouched"
    );
    assert_eq!(
        window_size(&mut h),
        size_before,
        "Add leaves the window's bounds untouched"
    );
}
