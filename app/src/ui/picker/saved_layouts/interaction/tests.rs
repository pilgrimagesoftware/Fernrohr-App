// Named imports rather than `use super::*`: a glob would re-import
// `gpui_kit`'s `test` attribute macro (via this file's own `use gpui_kit::*`),
// shadowing the builtin `#[test]` (mirrors `ui/picker/interaction/tests.rs`).
use crate::config::saved_layouts::SavedLayout;
use crate::ui::picker::saved_layouts::SavedLayoutsPicker;
use crate::util::shell::MainWindow;
use gpui_kit::component::dock::DockAreaState;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

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

/// 3.3: arrow-key navigation selects row 2, then a *real* mouse hover over
/// row 1 - simulated with `simulate_mouse_move` rather than poking
/// `CommandState` directly, so it actually goes through `Command`'s own
/// `on_hover` -> `select` -> our `on_select` guard the way a real hover does
/// (and the way a direct `set_selected_index` call cannot: it skips the
/// mouse event that would otherwise flip `window.last_input_was_keyboard()`
/// back to `false`, which is what the guard itself keys on) - still leaves
/// row 2 selected.
#[gpui_kit::test]
async fn hover_never_changes_the_keyboard_selection(cx: &mut TestAppContext) {
    let dir = crate::util::test_paths::temp_path("saved-layouts-interaction-hover");
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    crate::config::saved_layouts::save(&dir, &fixture("Beta")).expect("seeds a layout");
    let (window, mut vcx) = picker(cx, dir).await;

    window
        .update(&mut vcx, |picker, window, cx| {
            let focus = picker.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    vcx.run_until_parked();

    vcx.simulate_keystrokes("down");
    vcx.run_until_parked();

    let selected = window
        .update(&mut vcx, |picker, _, _| picker.test_selected_name())
        .unwrap();
    assert_eq!(
        selected,
        Some("Beta".to_string()),
        "arrow-key navigation selects the second row"
    );

    let row_zero = vcx
        .debug_bounds("saved-layouts-row-0")
        .expect("row 0 is drawn");
    vcx.simulate_mouse_move(row_zero.center(), None, Modifiers::none());
    vcx.run_until_parked();

    let selected = window
        .update(&mut vcx, |picker, _, _| picker.test_selected_name())
        .unwrap();
    assert_eq!(
        selected,
        Some("Beta".to_string()),
        "hover must not move the keyboard selection"
    );
}

/// A click selects the row it lands on, independent of keyboard navigation.
#[gpui_kit::test]
async fn a_click_selects_the_row(cx: &mut TestAppContext) {
    let dir = crate::util::test_paths::temp_path("saved-layouts-interaction-click");
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    crate::config::saved_layouts::save(&dir, &fixture("Beta")).expect("seeds a layout");
    let (window, mut vcx) = picker(cx, dir).await;

    window
        .update(&mut vcx, |picker, window, cx| {
            picker.handle_row_click(1, window, cx);
        })
        .unwrap();

    let selected = window
        .update(&mut vcx, |picker, _, _| picker.test_selected_name())
        .unwrap();
    assert_eq!(selected, Some("Beta".to_string()));
}

/// Escape in the rename field discards it without touching either name.
#[gpui_kit::test]
async fn cancelling_a_rename_leaves_both_names_unchanged(cx: &mut TestAppContext) {
    let dir = crate::util::test_paths::temp_path("saved-layouts-interaction-cancel-rename");
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    let (window, mut vcx) = picker(cx, dir.clone()).await;

    window
        .update(&mut vcx, |picker, window, cx| {
            picker.start_rename(0, "Alpha".to_string(), window, cx);
        })
        .unwrap();
    vcx.run_until_parked();

    window
        .update(&mut vcx, |picker, _, cx| {
            picker.cancel_rename(cx);
        })
        .unwrap();

    let names = window
        .update(&mut vcx, |picker, _, _| picker.test_layout_names())
        .unwrap();
    assert_eq!(names, vec!["Alpha".to_string()]);
    let on_disk = crate::config::saved_layouts::load_all(&dir).0;
    assert_eq!(on_disk.len(), 1);
    assert_eq!(on_disk[0].name, "Alpha");
}
