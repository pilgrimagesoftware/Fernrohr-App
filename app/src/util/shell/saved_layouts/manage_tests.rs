//! `layouts.manage` and the saved layouts picker it opens (tasks 3.2-3.5):
//! the default binding reaching it from both window modes, and the picker's
//! rename/delete commands through the app's real keymap and confirmation
//! dialog. The picker's own list rendering and hover-vs-keyboard selection
//! are `ui::picker::saved_layouts`'s own tests - this file only proves the
//! real-keystroke route from a live `MainWindow` through the registered
//! commands, which needs the full app init (`init`) those tests don't.

use crate::config::saved_layouts::SavedLayout;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::confirm_dialog;
use crate::util::shell::test_support::{press, temp_layouts_dir, temp_workspace_path};
use crate::util::shell::{MainWindow, SavedLayoutsDir, init};
use gpui_kit::component::Root;
use gpui_kit::component::dock::DockAreaState;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext, WindowHandle};
use std::path::PathBuf;

struct Harness {
    /// Kept alive for the window's own duration; no test reads it back.
    _window: WindowHandle<Root>,
    layouts_dir: PathBuf,
    vcx: VisualTestContext,
}

fn fixture(name: &str) -> SavedLayout {
    SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: name.to_string(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts: vec!["demo".into()],
        dock: DockAreaState::default(),
        resource_panel_width: Some(240.0),
        window_width: 800.0,
        window_height: 600.0,
    }
}

/// A window on `demo`, in `Workspace` mode unless `picker` asks for the
/// cluster picker instead - either way with its own temp layouts directory
/// (mirrors `saved_layouts::tests::harness`).
fn harness(cx: &mut TestAppContext, picker: bool) -> Harness {
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
                MainWindow::test_workspace(vec!["demo".into()], window, cx)
            }
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

fn saved(h: &Harness) -> Vec<SavedLayout> {
    crate::config::saved_layouts::load_all(&h.layouts_dir).0
}

/// Opens the picker via its default binding, then delivers the dialog's
/// first frame (`confirm_dialog::deliver_first_frame` is misnamed for this
/// call site - it is a generic "draw a frame and the next one, as the
/// platform would after any dialog opens" helper, not something specific to
/// a confirmation - but it is the one the codebase already has). Tests have
/// no frame loop: without this, the picker's own content is never actually
/// painted, so it never enters the window's real dispatch tree, and a
/// keystroke aimed at it (`r`, `backspace`, arrow navigation) silently hits
/// nothing - `dialog_open` still reports `true` throughout, since that only
/// reads a state flag, not whether the dialog's content has ever rendered.
/// Anything that only checks `dialog_open` doesn't need this; anything that
/// goes on to act on the picker's own rows does.
fn open_picker(h: &mut Harness) {
    press(&mut h.vcx, "secondary-shift-o");
    confirm_dialog::deliver_first_frame(&mut h.vcx);
}

/// 3.2: the default binding opens the picker from a window in cluster-picker
/// mode - `layouts.manage`'s `context: None` makes it reachable even without
/// a connected workspace.
#[gpui_kit::test]
async fn the_default_binding_opens_the_picker_from_the_cluster_picker(cx: &mut TestAppContext) {
    let mut h = harness(cx, true);
    assert!(!dialog_open(&mut h));
    press(&mut h.vcx, "secondary-shift-o");
    assert!(
        dialog_open(&mut h),
        "the picker opens over the cluster picker"
    );
}

/// The same binding also opens the picker from a connected workspace window.
#[gpui_kit::test]
async fn the_default_binding_opens_the_picker_from_a_workspace(cx: &mut TestAppContext) {
    let mut h = harness(cx, false);
    press(&mut h.vcx, "secondary-shift-o");
    assert!(dialog_open(&mut h));
}

/// 3.4: `r`, typing a new name, and Enter renames the selected layout.
#[gpui_kit::test]
async fn renaming_the_selected_layout(cx: &mut TestAppContext) {
    let mut h = harness(cx, false);
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture("Alpha")).expect("seeds a layout");

    open_picker(&mut h);
    press(&mut h.vcx, "r");
    type_text(&mut h, "Renamed");
    press(&mut h.vcx, "enter");

    let layouts = saved(&h);
    assert_eq!(
        layouts.len(),
        1,
        "a rename updates the file, not add a new one"
    );
    assert_eq!(layouts[0].name, "Renamed");
}

/// 3.4: a rename to a name already in use, differing only by case, is
/// rejected - both original names stay as they were.
#[gpui_kit::test]
async fn a_colliding_rename_is_rejected(cx: &mut TestAppContext) {
    let mut h = harness(cx, false);
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture("Alpha")).expect("seeds a layout");
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture("Beta")).expect("seeds a layout");

    open_picker(&mut h);
    // The list is sorted case-insensitively by name, so "Alpha" is row 0 -
    // the picker's default selection.
    press(&mut h.vcx, "r");
    type_text(&mut h, "beta");
    press(&mut h.vcx, "enter");

    let mut names: Vec<String> = saved(&h).into_iter().map(|layout| layout.name).collect();
    names.sort();
    assert_eq!(
        names,
        vec!["Alpha".to_string(), "Beta".to_string()],
        "a case-only-different collision is rejected; both names are kept"
    );
}

/// 3.5: backspace opens the delete confirmation at `Severity::Irreversible`
/// - focus starts on Cancel, so a bare Enter declines and the layout stays.
#[gpui_kit::test]
async fn backspace_then_enter_cancels_the_delete(cx: &mut TestAppContext) {
    let mut h = harness(cx, false);
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture("Alpha")).expect("seeds a layout");

    open_picker(&mut h);
    press(&mut h.vcx, "backspace");
    // The confirmation is itself a second dialog, with its own first frame
    // to deliver before `enter` can reach its (Cancel-focused) content.
    confirm_dialog::deliver_first_frame(&mut h.vcx);
    press(&mut h.vcx, "enter");

    let layouts = saved(&h);
    assert_eq!(
        layouts.len(),
        1,
        "Enter on an Irreversible confirmation cancels"
    );
    assert_eq!(layouts[0].name, "Alpha");
}

/// 3.5: backspace, then the confirm shortcut, removes the saved layout - the
/// row (and its file) are gone afterward.
#[gpui_kit::test]
async fn backspace_then_confirming_removes_the_layout(cx: &mut TestAppContext) {
    let mut h = harness(cx, false);
    crate::config::saved_layouts::save(&h.layouts_dir, &fixture("Alpha")).expect("seeds a layout");

    open_picker(&mut h);
    press(&mut h.vcx, "backspace");
    confirm_dialog::deliver_first_frame(&mut h.vcx);
    press(&mut h.vcx, "secondary-backspace");

    assert!(saved(&h).is_empty(), "the layout is gone from the store");
}
