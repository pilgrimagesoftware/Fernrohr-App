//! `saved-panel-layouts` tasks section 6: the Settings window's Layouts
//! section - listing, the empty-store message, `settings.show_layouts`, and
//! removing a saved layout through the shared Irreversible confirmation
//! (`ui::saved_layout_delete`), end to end by keyboard.

use super::super::layouts;
use super::super::{Section, ShowLayouts};
use super::layout::{drawn, press_button, show, shown};
use super::{app, open, press};
use crate::config::saved_layouts::SavedLayout;
use crate::ui::confirm_dialog;
use gpui_kit::component::dock::DockAreaState;
use gpui_kit::{TestAppContext, VisualTestContext};
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

/// Opens Settings over a window whose saved layouts live under a fresh temp
/// directory - `util::shell::set_layouts_dir_for_test` is the crate-visible
/// override `util::shell`'s own saved-layout tests set directly on
/// `SavedLayoutsDir`, exposed so this section's tests don't touch the real
/// `state_dir()/layouts/` either.
fn harness(
    cx: &mut TestAppContext,
) -> (PathBuf, gpui_kit::WindowHandle<gpui_kit::component::Root>) {
    let main = app(cx);
    let dir = crate::util::test_paths::temp_path("settings-layouts-dir");
    cx.update(|cx| crate::util::shell::set_layouts_dir_for_test(dir.clone(), cx));
    let (handle, _section) = open(main, cx);
    (dir, handle)
}

/// 6.1: a fixture directory with two saved layouts shows both names - sorted
/// case-insensitively by `load_all`, so "Alpha" is row 0 and "Beta" row 1
/// (`layouts::row_name_selector`'s format, hardcoded here as literals since
/// `debug_bounds` needs a `&'static str`).
#[gpui_kit::test]
async fn layouts_section_lists_two_fixture_layouts_by_name(cx: &mut TestAppContext) {
    let (dir, handle) = harness(cx);
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    crate::config::saved_layouts::save(&dir, &fixture("Beta")).expect("seeds a layout");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    show(&mut vcx, Box::new(ShowLayouts));

    assert!(drawn(
        &mut vcx,
        handle.into(),
        "settings-layouts-row-name-0"
    ));
    assert!(drawn(
        &mut vcx,
        handle.into(),
        "settings-layouts-row-name-1"
    ));
}

/// 6.1: an empty store shows the explicit message rather than a blank list.
#[gpui_kit::test]
async fn empty_store_shows_the_explicit_message(cx: &mut TestAppContext) {
    let (_dir, handle) = harness(cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    show(&mut vcx, Box::new(ShowLayouts));

    assert!(drawn(&mut vcx, handle.into(), layouts::EMPTY_SELECTOR));
}

/// A file that fails to parse is named, the same way the saved layouts
/// picker reports one, alongside the layouts that did parse.
#[gpui_kit::test]
async fn an_unreadable_file_is_reported_by_name(cx: &mut TestAppContext) {
    let (dir, handle) = harness(cx);
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    std::fs::write(dir.join("broken.json"), "{not json").expect("writes a corrupt file");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    show(&mut vcx, Box::new(ShowLayouts));

    assert!(drawn(
        &mut vcx,
        handle.into(),
        "settings-layouts-row-name-0"
    ));
    assert!(drawn(&mut vcx, handle.into(), layouts::UNREADABLE_SELECTOR));
}

/// 6.2: dispatching `settings.show_layouts` shows `Section::Layouts`.
#[gpui_kit::test]
async fn dispatching_show_layouts_shows_the_section(cx: &mut TestAppContext) {
    let (_dir, handle) = harness(cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    assert_eq!(shown(cx, handle), Section::KeyboardShortcuts);

    show(&mut vcx, Box::new(ShowLayouts));
    assert_eq!(shown(cx, handle), Section::Layouts);
}

/// 6.3: Tab to a row's Remove control and activate it; the confirmation
/// opens at `Severity::Irreversible` (focus on Cancel), so a bare Enter
/// declines and the layout stays.
#[gpui_kit::test]
async fn tab_to_remove_then_enter_cancels_the_delete(cx: &mut TestAppContext) {
    let (dir, handle) = harness(cx);
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    show(&mut vcx, Box::new(ShowLayouts));

    press_button(&mut vcx, handle.into(), &layouts::remove_button_id(0));
    confirm_dialog::deliver_first_frame(&mut vcx);
    press(&mut vcx, "enter");

    let saved = crate::config::saved_layouts::load_all(&dir).0;
    assert_eq!(
        saved.len(),
        1,
        "Enter on an Irreversible confirmation cancels"
    );
    assert_eq!(saved[0].name, "Alpha");
}

/// 6.3/6.4: entirely by keyboard - Tab from the sidebar button into the
/// section, through the row's Remove control, then the confirm shortcut -
/// removes the saved layout from the section and from `layouts/`.
#[gpui_kit::test]
async fn removing_a_layout_entirely_by_keyboard(cx: &mut TestAppContext) {
    let (dir, handle) = harness(cx);
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);

    press_button(&mut vcx, handle.into(), Section::Layouts.button_id());
    assert_eq!(
        shown(cx, handle),
        Section::Layouts,
        "Tab and Space on the sidebar button"
    );

    press_button(&mut vcx, handle.into(), &layouts::remove_button_id(0));
    confirm_dialog::deliver_first_frame(&mut vcx);
    press(&mut vcx, "secondary-backspace");

    assert!(
        crate::config::saved_layouts::load_all(&dir).0.is_empty(),
        "the layout is gone from the store"
    );
    assert!(
        drawn(&mut vcx, handle.into(), layouts::EMPTY_SELECTOR),
        "the section shows the empty message once its only layout is removed"
    );
}

/// Rendering reads only the cache, never the directory: once the list has
/// loaded, deleting the whole directory and redrawing still shows it.
#[gpui_kit::test]
async fn rendering_reads_the_cache_not_the_directory(cx: &mut TestAppContext) {
    let (dir, handle) = harness(cx);
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    show(&mut vcx, Box::new(ShowLayouts));
    assert!(drawn(
        &mut vcx,
        handle.into(),
        "settings-layouts-row-name-0"
    ));

    std::fs::remove_dir_all(&dir).expect("removes the layouts directory");
    handle
        .update(&mut vcx, |_, window, _| window.refresh())
        .unwrap();
    vcx.run_until_parked();
    assert!(
        drawn(&mut vcx, handle.into(), "settings-layouts-row-name-0"),
        "still drawn from the cache"
    );
    assert!(!drawn(&mut vcx, handle.into(), layouts::EMPTY_SELECTOR));
}

/// Before the first background read lands, the section says it's loading
/// rather than claiming there are no saved layouts.
#[gpui_kit::test]
async fn the_section_says_it_is_loading_until_the_first_read_lands(cx: &mut TestAppContext) {
    let (dir, handle) = harness(cx);
    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("seeds a layout");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    // Dispatched without letting the background read run yet.
    vcx.update(|window, cx| window.dispatch_action(Box::new(ShowLayouts), cx));
    assert!(drawn(&mut vcx, handle.into(), layouts::LOADING_SELECTOR));
    assert!(!drawn(&mut vcx, handle.into(), layouts::EMPTY_SELECTOR));

    vcx.run_until_parked();
    assert!(drawn(
        &mut vcx,
        handle.into(),
        "settings-layouts-row-name-0"
    ));
    assert!(!drawn(&mut vcx, handle.into(), layouts::LOADING_SELECTOR));
}

/// A layout saved elsewhere while the section is shown appears without
/// showing the section again: a save bumps `SavedLayoutsChanged`.
#[gpui_kit::test]
async fn a_layout_saved_elsewhere_appears_while_the_section_is_shown(cx: &mut TestAppContext) {
    let (dir, handle) = harness(cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    show(&mut vcx, Box::new(ShowLayouts));
    assert!(drawn(&mut vcx, handle.into(), layouts::EMPTY_SELECTOR));

    crate::config::saved_layouts::save(&dir, &fixture("Alpha")).expect("saves a layout");
    vcx.update(|_, cx| crate::util::shell::note_layouts_changed(cx));
    vcx.run_until_parked();
    assert!(drawn(
        &mut vcx,
        handle.into(),
        "settings-layouts-row-name-0"
    ));
}
