// `use super::*` here would re-import `gpui_kit`'s `test` attribute macro
// (this file's `use gpui_kit::*` brings it in), shadowing the builtin
// `#[test]` and sending a plain sync test into `#[gpui_kit::test]`'s
// async-runtime expansion instead - hence the explicit imports below rather
// than a glob (mirrors `ui/picker/render/tests.rs`).
use super::{EMPTY_SELECTOR, UNREADABLE_SELECTOR};
use crate::config::saved_layouts::SavedLayout;
use crate::ui::picker::saved_layouts::SavedLayoutsPicker;
use crate::util::shell::MainWindow;
use gpui_kit::component::dock::DockAreaState;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use std::path::PathBuf;

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
    dir: PathBuf,
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

fn drawn(
    vcx: &mut VisualTestContext,
    window: gpui_kit::WindowHandle<SavedLayoutsPicker>,
    selector: &'static str,
) -> bool {
    let _ = vcx.update_window(window.into(), |_, window, cx| window.render_frame(cx));
    vcx.debug_bounds(selector).is_some()
}

/// 3.1: a fixture directory with two saved layouts shows both names.
#[gpui_kit::test]
async fn two_fixture_layouts_are_listed(cx: &mut TestAppContext) {
    let dir = crate::util::test_paths::temp_path("saved-layouts-render-two");
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    crate::config::saved_layouts::save(&dir, &fixture("Beta")).expect("seeds a layout");
    let (window, mut vcx) = picker(cx, dir).await;

    let names = window
        .update(&mut vcx, |picker, _, _| picker.test_layout_names())
        .unwrap();
    assert_eq!(names, vec!["Alpha".to_string(), "Beta".to_string()]);
}

/// 3.1: an empty directory shows an explicit message rather than a blank list.
#[gpui_kit::test]
async fn an_empty_directory_shows_an_explicit_message(cx: &mut TestAppContext) {
    let dir = crate::util::test_paths::temp_path("saved-layouts-render-empty");
    let (window, mut vcx) = picker(cx, dir).await;

    assert!(drawn(&mut vcx, window, EMPTY_SELECTOR));
}

/// 3.1: one unreadable file alongside readable layouts is reported by its
/// filename, without hiding the layouts that did parse.
#[gpui_kit::test]
async fn an_unreadable_file_is_reported_by_name(cx: &mut TestAppContext) {
    let dir = crate::util::test_paths::temp_path("saved-layouts-render-unreadable");
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    std::fs::write(dir.join("broken.json"), "{not json").expect("writes a corrupt file");
    let (window, mut vcx) = picker(cx, dir).await;

    let names = window
        .update(&mut vcx, |picker, _, _| picker.test_layout_names())
        .unwrap();
    assert_eq!(names, vec!["Alpha".to_string()]);
    assert!(drawn(&mut vcx, window, UNREADABLE_SELECTOR));
}
