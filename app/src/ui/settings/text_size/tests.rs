//! The Text Size row, driven by real keystrokes: the Settings window opened
//! with cmd-, and its buttons reached with Tab and pressed with Enter or Space.

use super::super::test_window;
use super::KEY_CONTEXT;
use crate::config::ui::Theme as ThemePreference;
use crate::ui::text_size::current;
use crate::util::shell::MainWindow;
use gpui_kit::component::Root;
use gpui_kit::{
    KeyDownEvent, KeyUpEvent, Keystroke, TestAppContext, VisualTestContext, WindowHandle,
};

fn temp_path(name: &str) -> std::path::PathBuf {
    crate::util::test_paths::temp_path(&format!("settings-text-size-{name}"))
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    let keys = Keystroke::parse(keys).expect("valid").unparse();
    cx.simulate_keystrokes(&keys);
    cx.run_until_parked();
}

/// Presses and releases `key`. A button clicks on the release, and
/// `simulate_keystrokes` only sends the press, so Enter and Space need this.
fn press_and_release(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).expect("valid");
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    cx.run_until_parked();
}

/// The app as `main` starts it, then Settings opened with cmd-, and its
/// Appearance section shown - focus at its top, as a user showing it would have.
fn settings(cx: &mut TestAppContext) -> VisualTestContext {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_path("workspace"), temp_path("keymap"));
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        crate::ui::theme::init(ThemePreference::Light, cx);
        crate::util::shell::init(cx, workspace, &keymap);
    });
    let main: WindowHandle<MainWindow> = cx.add_window(MainWindow::test_picker_window);
    main.update(cx, |main_window, window, cx| {
        main_window.test_focus(window, cx)
    })
    .unwrap();
    let mut vcx = VisualTestContext::from_window(main.into(), cx);
    press(&mut vcx, "cmd-,");
    let handle: WindowHandle<Root> = cx
        .update(|cx| test_window(cx))
        .expect("cmd-, opened Settings");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    // Text Size is in the Appearance section (#139).
    vcx.update(|window, cx| window.dispatch_action(Box::new(super::super::ShowAppearance), cx));
    vcx.run_until_parked();
    vcx
}

fn percent(vcx: &mut VisualTestContext) -> u16 {
    vcx.update(|_, cx| current(cx).percent())
}

fn in_stepper(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, _| {
        window
            .context_stack()
            .iter()
            .any(|context| context.contains(KEY_CONTEXT))
    })
}

/// Tab from the window's initial focus until focus enters the row - its
/// first button, −. Fails if Tab never gets there.
fn tab_to_stepper(vcx: &mut VisualTestContext) {
    for _ in 0..20 {
        press(vcx, "tab");
        if in_stepper(vcx) {
            return;
        }
    }
    panic!("Tab never reached the Text Size row");
}

#[gpui_kit::test]
async fn tab_reaches_each_button_and_enter_or_space_presses_it(cx: &mut TestAppContext) {
    let mut vcx = settings(cx);
    tab_to_stepper(&mut vcx);

    press_and_release(&mut vcx, "enter");
    assert_eq!(percent(&mut vcx), 90, "− with Enter");
    press(&mut vcx, "tab");
    press_and_release(&mut vcx, "space");
    press_and_release(&mut vcx, "space");
    assert_eq!(percent(&mut vcx), 110, "+ with Space, twice");
    press(&mut vcx, "tab");
    press_and_release(&mut vcx, "enter");
    assert_eq!(percent(&mut vcx), 100, "Reset with Enter");
    assert!(in_stepper(&mut vcx), "focus stays on the row");
}

#[gpui_kit::test]
async fn the_global_keys_work_in_settings_and_the_row_follows_them(cx: &mut TestAppContext) {
    let mut vcx = settings(cx);
    press(&mut vcx, "cmd-=");
    press(&mut vcx, "cmd-=");
    assert_eq!(percent(&mut vcx), 120);
    // The value label is redrawn with the new size, not left at 100%.
    let label = vcx.debug_bounds("text-size-value").expect("row drawn");
    press(&mut vcx, "cmd-shift-0");
    assert_eq!(percent(&mut vcx), 100);
    let reset = vcx.debug_bounds("text-size-value").expect("row drawn");
    assert!(
        reset.size.height < label.size.height,
        "the row redrew smaller"
    );
}
