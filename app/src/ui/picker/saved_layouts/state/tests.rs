// Named imports, not `use super::*`: a glob would re-import `gpui_kit`'s
// `test` attribute macro, shadowing the builtin `#[test]` (see
// `ui/picker/render/tests.rs`'s own note).
use crate::config::saved_layouts::SavedLayout;
use crate::ui::picker::saved_layouts::SavedLayoutsPicker;
use crate::util::shell::MainWindow;
use gpui_kit::component::dock::DockAreaState;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

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

/// A picker over `dir`'s saved layouts, with a throwaway `MainWindow` behind
/// it to satisfy the constructor's seam for section 4 - nothing here reads
/// it back.
async fn picker(
    cx: &mut TestAppContext,
    dir: std::path::PathBuf,
) -> (
    gpui_kit::WindowHandle<SavedLayoutsPicker>,
    VisualTestContext,
) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let main_window_handle = cx.add_window(MainWindow::test_picker_window);
    let main_window: Entity<MainWindow> = cx
        .update(|cx| main_window_handle.entity(cx))
        .expect("the window built its MainWindow");
    let window = cx.add_window(move |window, cx| {
        SavedLayoutsPicker::new(dir, main_window.downgrade(), window, cx)
    });
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    (window, vcx)
}

/// Renaming clamps the selection to the last row once the renamed-away
/// position no longer exists - exercised indirectly through the delete path,
/// since both resize the list the same way.
#[gpui_kit::test]
async fn reload_clamps_the_selection_to_the_shrunk_list(cx: &mut TestAppContext) {
    let dir = crate::util::test_paths::temp_path("saved-layouts-state-reload");
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    crate::config::saved_layouts::save(&dir, &fixture("Beta")).expect("seeds a layout");
    let (window, mut vcx) = picker(cx, dir.clone()).await;

    window
        .update(&mut vcx, |picker, _, cx| {
            picker.selected_index = Some(1);
            crate::config::saved_layouts::remove(&dir, "Beta").expect("removes the second layout");
            picker.reload(cx);
        })
        .unwrap();

    let (names, selected) = window
        .update(&mut vcx, |picker, _, _| {
            (picker.test_layout_names(), picker.test_selected_name())
        })
        .unwrap();
    assert_eq!(names, vec!["Alpha".to_string()]);
    assert_eq!(
        selected,
        Some("Alpha".to_string()),
        "the selection moves to the row that's still there rather than pointing past the list"
    );
}

/// Reloading an emptied directory clears the selection rather than leaving
/// it pointing at a row that no longer exists.
#[gpui_kit::test]
async fn reload_clears_the_selection_when_the_list_empties(cx: &mut TestAppContext) {
    let dir = crate::util::test_paths::temp_path("saved-layouts-state-reload-empty");
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    let (window, mut vcx) = picker(cx, dir.clone()).await;

    window
        .update(&mut vcx, |picker, _, cx| {
            crate::config::saved_layouts::remove(&dir, "Alpha").expect("removes the only layout");
            picker.reload(cx);
        })
        .unwrap();

    let selected = window
        .update(&mut vcx, |picker, _, _| picker.test_selected_name())
        .unwrap();
    assert_eq!(selected, None);
}
