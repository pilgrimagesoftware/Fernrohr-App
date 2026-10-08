//! `layouts.save` through a real window and the app's keymap (tasks 2.1-2.3):
//! opening the dialog (and not opening it from the cluster picker), capturing
//! both of a window's panels and its bounds, and the case-insensitive
//! overwrite confirmation. The secret-safety regression guard (2.4) lives
//! beside `object_detail`'s own reveal tests instead - see
//! `k8s::resource::object_detail::tests::secrets`.

use super::ERROR_SELECTOR;
use crate::config::saved_layouts::SavedLayout;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::confirm_dialog;
use crate::ui::nav::NavTarget;
use crate::util::shell::SaveLayout;
use crate::util::shell::test_support::{press, temp_layouts_dir, temp_workspace_path};
use crate::util::shell::{MainWindow, SavedLayoutsDir, init};
use gpui_kit::component::Root;
use gpui_kit::component::dock::DockAreaState;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext, WindowHandle};
use kube::core::GroupVersionKind;
use std::path::PathBuf;

fn services() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

struct Harness {
    /// Under a `Root`, as `open_window` builds it, so the dialog has a layer.
    window: WindowHandle<Root>,
    layouts_dir: PathBuf,
    vcx: VisualTestContext,
}

/// A window on `demo`, in `Workspace` mode unless `picker` asks for the
/// cluster picker instead - either way with its own temp layouts directory,
/// so no test ever touches the real `state_dir()/layouts/`.
fn harness(cx: &mut TestAppContext, picker: bool, second_panel: bool) -> Harness {
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
        let main = cx.new(|cx| {
            if picker {
                MainWindow::test_picker_window(window, cx)
            } else {
                let mut main = MainWindow::test_workspace(vec!["demo".into()], window, cx);
                if second_panel {
                    main.open_target(NavTarget::Kind(services()), window, cx);
                }
                main
            }
        });
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its MainWindow");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    // The same initial focus `open_window` gives a real window: the picker's
    // own handle in `Picker` mode, or the displayed panel - inside the
    // `Workspace`-tagged body - in `Workspace` mode, so `layouts.save`'s
    // `Workspace`-scoped binding is only active where it really would be.
    vcx.update(|window, cx| main.update(cx, |main, cx| main.focus_initial(window, cx)));
    vcx.run_until_parked();
    Harness {
        window,
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

fn drawn(h: &mut Harness, selector: String) -> bool {
    let _ = h
        .vcx
        .update_window(h.window.into(), |_, window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector.leak()).is_some()
}

fn saved(h: &Harness) -> Vec<SavedLayout> {
    crate::config::saved_layouts::load_all(&h.layouts_dir).0
}

fn window_content_size(h: &mut Harness) -> (f32, f32) {
    h.vcx
        .update_window(h.window.into(), |_, window, _cx| {
            let size = window.viewport_size();
            (f32::from(size.width), f32::from(size.height))
        })
        .unwrap()
}

#[gpui_kit::test]
async fn the_default_binding_opens_the_naming_dialog_in_a_workspace_window(
    cx: &mut TestAppContext,
) {
    let mut h = harness(cx, false, false);
    assert!(!dialog_open(&mut h));
    press(&mut h.vcx, "secondary-shift-s");
    assert!(dialog_open(&mut h), "the naming dialog opens");
}

#[gpui_kit::test]
async fn the_default_binding_does_nothing_from_the_cluster_picker(cx: &mut TestAppContext) {
    let mut h = harness(cx, true, false);
    press(&mut h.vcx, "secondary-shift-s");
    assert!(
        !dialog_open(&mut h),
        "the picker has no Save handler, so the key does nothing"
    );
}

/// The Window menu's Save Panel Layout is enabled in a workspace and disabled
/// from the cluster picker: a menu item is enabled exactly while a handler
/// for its action is on the focus path.
#[gpui_kit::test]
async fn save_is_available_in_a_workspace_and_not_from_the_cluster_picker(cx: &mut TestAppContext) {
    let mut workspace = harness(cx, false, false);
    assert!(
        workspace
            .vcx
            .update(|window, cx| window.is_action_available(&SaveLayout, cx))
    );
    let mut picker = harness(cx, true, false);
    assert!(
        !picker
            .vcx
            .update(|window, cx| window.is_action_available(&SaveLayout, cx))
    );
}

#[gpui_kit::test]
async fn an_empty_name_is_refused(cx: &mut TestAppContext) {
    let mut h = harness(cx, false, false);
    press(&mut h.vcx, "secondary-shift-s");
    press(&mut h.vcx, "enter");
    assert!(
        dialog_open(&mut h),
        "an empty name doesn't close the dialog"
    );
    assert!(drawn(&mut h, ERROR_SELECTOR.into()));
    assert!(saved(&h).is_empty());
}

#[gpui_kit::test]
async fn saving_captures_both_panels_and_the_window_bounds(cx: &mut TestAppContext) {
    let mut h = harness(cx, false, true);
    let (width, height) = window_content_size(&mut h);

    press(&mut h.vcx, "secondary-shift-s");
    type_text(&mut h, "Two Panels");
    press(&mut h.vcx, "enter");
    assert!(!dialog_open(&mut h), "saving closes the dialog");

    let layouts = saved(&h);
    assert_eq!(layouts.len(), 1);
    let layout = &layouts[0];
    assert_eq!(layout.name, "Two Panels");
    assert_eq!(layout.contexts, vec!["demo".to_string()]);
    assert_eq!(layout.window_width, width);
    assert_eq!(layout.window_height, height);

    let dock_json = serde_json::to_string(&layout.dock).expect("the dock serializes");
    assert!(
        dock_json.contains("\"panel_name\":\"Pods\""),
        "the Pods panel's content key is in the dock: {dock_json}"
    );
    assert!(
        dock_json.contains("\"panel_name\":\"ObjectList\""),
        "the Services panel's content key is in the dock: {dock_json}"
    );
    assert!(dock_json.contains("\"context_name\":\"demo\""));
}

#[gpui_kit::test]
async fn saving_over_a_name_differing_only_by_case_asks_first(cx: &mut TestAppContext) {
    let mut h = harness(cx, false, false);
    let existing = SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: "My Layout".into(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts: vec!["demo".into()],
        dock: DockAreaState::default(),
        resource_panel_width: Some(240.0),
        window_width: 800.0,
        window_height: 600.0,
    };
    crate::config::saved_layouts::save(&h.layouts_dir, &existing).expect("seeds a saved layout");

    press(&mut h.vcx, "secondary-shift-s");
    type_text(&mut h, "my layout");
    press(&mut h.vcx, "enter");
    assert!(
        dialog_open(&mut h),
        "a case-only-different name asks before overwriting"
    );

    // Irreversible: focus starts on Cancel, so a bare Enter declines.
    confirm_dialog::deliver_first_frame(&mut h.vcx);
    press(&mut h.vcx, "enter");
    assert!(!dialog_open(&mut h));

    let layouts = saved(&h);
    assert_eq!(layouts.len(), 1, "declining creates nothing new");
    assert_eq!(layouts[0].name, "My Layout");
    assert_eq!(
        layouts[0].updated_at, "1970-01-01T00:00:00Z",
        "declining leaves the existing layout's updated_at unchanged"
    );
}

#[gpui_kit::test]
async fn confirming_the_overwrite_replaces_the_existing_layout(cx: &mut TestAppContext) {
    let mut h = harness(cx, false, false);
    let existing = SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: "My Layout".into(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts: vec!["demo".into()],
        dock: DockAreaState::default(),
        resource_panel_width: Some(240.0),
        window_width: 800.0,
        window_height: 600.0,
    };
    crate::config::saved_layouts::save(&h.layouts_dir, &existing).expect("seeds a saved layout");

    press(&mut h.vcx, "secondary-shift-s");
    type_text(&mut h, "my layout");
    press(&mut h.vcx, "enter");
    confirm_dialog::deliver_first_frame(&mut h.vcx);
    press(&mut h.vcx, "secondary-backspace");
    assert!(!dialog_open(&mut h));

    let layouts = saved(&h);
    assert_eq!(layouts.len(), 1, "still one file - overwritten in place");
    assert_eq!(
        layouts[0].name, "my layout",
        "the name as typed and confirmed wins, like any other save"
    );
    assert_eq!(
        layouts[0].created_at, "1970-01-01T00:00:00Z",
        "an overwrite keeps the original created_at"
    );
    assert_ne!(
        layouts[0].updated_at, "1970-01-01T00:00:00Z",
        "an overwrite bumps updated_at"
    );
    assert_eq!(layouts[0].contexts, vec!["demo".to_string()]);
}

#[gpui_kit::test]
async fn cancel_closes_the_dialog_without_saving(cx: &mut TestAppContext) {
    let mut h = harness(cx, false, false);
    press(&mut h.vcx, "secondary-shift-s");
    type_text(&mut h, "Abandoned");
    press(&mut h.vcx, "escape");
    assert!(!dialog_open(&mut h));
    assert!(saved(&h).is_empty());
}
