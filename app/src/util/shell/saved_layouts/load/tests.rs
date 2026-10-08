//! Loading a saved layout through the real picker UI and keymap (tasks 4.1,
//! 4.2), plus the shared-fixture decode check task 4.3 asks for. Mirrors
//! `manage_tests.rs`'s own harness (real `init`, a temp `SavedLayoutsDir`),
//! but keeps its own: these tests also build a `SavedLayout` from a second,
//! throwaway window's *real* dock dump (`fixture_layout`), which
//! `manage_tests.rs`'s hand-built fixtures (an empty `DockAreaState::
//! default()`) don't need.
//!
//! Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
//! next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
//! `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::config::saved_layouts::SavedLayout;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::confirm_dialog;
use crate::ui::nav::NavTarget;
use crate::util::shell::test_support::{press, temp_layouts_dir, temp_workspace_path};
use crate::util::shell::{MainWindow, SavedLayoutsDir, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext as _, Entity, Pixels, Size, TestAppContext, VisualTestContext, WindowHandle,
};
use std::path::PathBuf;

struct Harness {
    main: Entity<MainWindow>,
    /// Kept alive for the window's own duration; no test reads it back.
    _window: WindowHandle<Root>,
    layouts_dir: PathBuf,
    vcx: VisualTestContext,
}

/// A connected single-context window on `demo`, with the app's real bindings
/// and a temp `layouts/` directory - the window every test here loads a
/// saved layout into. Mirrors `manage_tests.rs`'s own `harness`, minus the
/// cluster-picker-mode branch nothing here needs.
fn harness(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    let layouts_dir = temp_layouts_dir();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        cx.set_global(SavedLayoutsDir(layouts_dir.clone()));
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
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

/// A `SavedLayout` with real panel content: a throwaway window on `contexts`
/// (reusing `harness`'s own pre-seeded `ClusterRegistry` test sessions, so
/// `contexts` must already hold a session this process set up) opens Pods
/// (every workspace's own default) then `extra_targets` in order, and its
/// dock is dumped the same way `layouts.save` does
/// (`util::shell::saved_layouts::capture_layout`) - real content, not a
/// hand-built `DockAreaState`, so `restored_panel_keys` decodes exactly what
/// a saved layout from the real Save path would.
fn fixture_layout(
    cx: &mut TestAppContext,
    name: &str,
    contexts: Vec<String>,
    extra_targets: Vec<NavTarget>,
) -> SavedLayout {
    let window = cx.add_window(|window, cx| {
        let mut main = MainWindow::test_workspace(contexts.clone(), window, cx);
        for target in extra_targets {
            main.open_target(target, window, cx);
        }
        main
    });
    cx.run_until_parked();
    let dock = window
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            dock_area.read(cx).dump(cx)
        })
        .unwrap();
    SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: name.to_string(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts,
        dock,
        resource_panel_width: Some(333.0),
        window_width: 999.0,
        window_height: 555.0,
    }
}

fn events_kind() -> DiscoveredKind {
    DiscoveredKind::events()
}

fn dialog_open(h: &mut Harness) -> bool {
    h.vcx
        .update(gpui_kit::component::WindowExt::has_active_dialog)
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

/// 4.1: loading with Replace swaps the window's dock, Resource panel width
/// and bounds for the saved layout's - not a merge with what was open before.
#[gpui_kit::test]
async fn loading_with_replace_swaps_the_arrangement(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let fixture = fixture_layout(
        cx,
        "Replacement",
        vec!["demo".into()],
        vec![NavTarget::Kind(events_kind())],
    );
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture).expect("seeds a layout");

    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods()],
        "the window starts with just its own default arrangement"
    );

    open_picker(&mut h);
    press(&mut h.vcx, "enter");

    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods(), NavTarget::Kind(events_kind())],
        "Replace swaps in the saved layout's own panels"
    );
    assert_eq!(
        resource_width(&mut h),
        Some(gpui_kit::px(333.0)),
        "Replace applies the saved Resource panel width"
    );
    assert_eq!(
        window_size(&mut h),
        gpui_kit::size(gpui_kit::px(999.0), gpui_kit::px(555.0)),
        "Replace applies the saved window size"
    );
}

/// 4.2: loading with Add into a window with one panel open shows both the
/// original and the saved layout's panels, with Resource panel width and
/// window bounds unchanged.
#[gpui_kit::test]
async fn loading_with_add_shows_both_original_and_saved_panels(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let fixture = fixture_layout(
        cx,
        "Addition",
        vec!["demo".into()],
        vec![NavTarget::Kind(events_kind())],
    );
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture).expect("seeds a layout");

    let width_before = resource_width(&mut h);
    let size_before = window_size(&mut h);

    open_picker(&mut h);
    press(&mut h.vcx, "secondary-enter");

    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods(), NavTarget::Kind(events_kind())],
        "Add lays the saved layout's panels alongside the existing one"
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

/// 4.2: loading with Add a saved layout containing a panel whose content key
/// matches one already open creates no duplicate.
#[gpui_kit::test]
async fn loading_with_add_does_not_duplicate_an_already_open_panel(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    // No `extra_targets`: this fixture's own window opens only Pods, the same
    // content key every fresh workspace already has open.
    let fixture = fixture_layout(cx, "JustPods", vec!["demo".into()], Vec::new());
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture).expect("seeds a layout");

    open_picker(&mut h);
    press(&mut h.vcx, "secondary-enter");

    assert_eq!(
        open_targets(&mut h),
        vec![NavTarget::pods()],
        "the matching content key is focused, not duplicated"
    );
    assert!(!dialog_open(&mut h), "Add closes the picker dialog");
}

/// 4.3: the same `PanelKey` values decode whether the source is a
/// `dock-layouts.json`-shaped store (`config::dock_layouts::DockLayouts`, the
/// automatic restore's own) or a `SavedLayout.dock` - both are the identical
/// `DockAreaState` dump type (design.md D3), not a parallel shape, so one
/// decoder (`restored_panel_keys`) serves both without widening its
/// `pub(super)` visibility (task 4.3's own requirement).
#[test]
fn the_same_panel_key_decodes_from_either_stores_dock_shape() {
    use crate::util::shell::restored_panel_keys;
    use gpui_kit::component::dock::{DockAreaState, PanelInfo, PanelState};

    let state = PanelState {
        panel_name: "Pods".to_string(),
        children: Vec::new(),
        info: PanelInfo::Panel(serde_json::json!({
            "context_name": "kind-dev",
            "namespaces": ["kube-system"],
        })),
    };

    let mut dock_layouts = crate::config::dock_layouts::DockLayouts::default();
    dock_layouts.insert(
        "kind-dev".to_string(),
        DockAreaState {
            center: state.clone(),
            ..Default::default()
        },
    );
    let saved_layout = SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: "Fixture".to_string(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts: vec!["kind-dev".to_string()],
        dock: DockAreaState {
            center: state,
            ..Default::default()
        },
        resource_panel_width: Some(240.0),
        window_width: 800.0,
        window_height: 600.0,
    };

    let from_dock_layouts =
        restored_panel_keys(&dock_layouts.get("kind-dev").expect("just inserted").center);
    let from_saved_layout = restored_panel_keys(&saved_layout.dock.center);

    assert_eq!(
        from_dock_layouts, from_saved_layout,
        "both stores' dock shape decodes to the same PanelKey values"
    );
    let key = from_saved_layout[0]
        .as_ref()
        .expect("a Pods panel is keyed");
    assert_eq!(key.target, NavTarget::pods());
    assert_eq!(key.context_name, "kind-dev");
    assert_eq!(key.namespaces, vec!["kube-system".to_string()]);
}
