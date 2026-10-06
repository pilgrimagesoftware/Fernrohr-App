//! `pending-chord-indicator` 3.2: the Shortcut Timeout row by keyboard - Tab
//! into its stepper, Space on − and + - and each change saved to `ui.toml`,
//! keeping the file's other preferences.

use super::super::test_window;
use super::KEY_CONTEXT;
use crate::config::ui::{ShortcutTimeout, Theme as ThemePreference, UiConfig};
use crate::ui::shortcut_timeout::current;
use crate::util::shell::MainWindow;
use gpui_kit::component::Root;
use gpui_kit::{
    KeyDownEvent, KeyUpEvent, Keystroke, TestAppContext, VisualTestContext, WindowHandle,
};

fn press(cx: &mut VisualTestContext, keys: &str) {
    let keys = Keystroke::parse(keys).expect("valid").unparse();
    cx.simulate_keystrokes(&keys);
    cx.run_until_parked();
}

/// A button clicks on the key's release, so Space needs both events.
fn press_space(cx: &mut VisualTestContext) {
    let keystroke = Keystroke::parse("space").expect("valid");
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    cx.run_until_parked();
}

/// Settings opened with cmd-, over a `ui.toml` holding a theme and a text size.
fn settings(cx: &mut TestAppContext, ui_path: &std::path::Path) -> VisualTestContext {
    cx.executor().allow_parking();
    let (workspace, keymap) = (
        crate::util::test_paths::temp_path("shortcut-timeout-workspace"),
        crate::util::test_paths::temp_path("shortcut-timeout-keymap"),
    );
    std::fs::write(ui_path, "theme = \"dark\"\ntext_size = 120\n").unwrap();
    let path = ui_path.to_path_buf();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::ui::theme::init(ThemePreference::Light, cx);
        crate::util::shell::init(cx, workspace, &keymap);
        crate::ui::shortcut_timeout::init(ShortcutTimeout::DEFAULT, path, cx);
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
    VisualTestContext::from_window(handle.into(), cx)
}

fn in_stepper(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, _| {
        window
            .context_stack()
            .iter()
            .any(|context| context.contains(KEY_CONTEXT))
    })
}

/// Tab until focus enters the row - its first button, −.
fn tab_to_stepper(vcx: &mut VisualTestContext) {
    for _ in 0..30 {
        press(vcx, "tab");
        if in_stepper(vcx) {
            return;
        }
    }
    panic!("Tab never reached the Shortcut Timeout row");
}

fn secs(vcx: &mut VisualTestContext) -> u8 {
    vcx.update(|_, cx| current(cx).secs())
}

#[gpui_kit::test]
async fn the_stepper_changes_the_timeout_by_keyboard_and_saves_it(cx: &mut TestAppContext) {
    let path = crate::util::test_paths::temp_path("shortcut-timeout-ui");
    let mut vcx = settings(cx, &path);
    assert_eq!(secs(&mut vcx), 3);

    tab_to_stepper(&mut vcx);
    press_space(&mut vcx);
    assert_eq!(secs(&mut vcx), 2, "− on Space");

    press(&mut vcx, "tab");
    press_space(&mut vcx);
    press_space(&mut vcx);
    press_space(&mut vcx);
    press_space(&mut vcx);
    assert_eq!(secs(&mut vcx), 6, "+ on Space, four times");

    let saved: UiConfig = crate::config::load(&path);
    assert_eq!(saved.shortcut_timeout_secs.secs(), 6);
    assert_eq!(
        saved.theme,
        ThemePreference::Dark,
        "the rest of the file kept"
    );
    assert_eq!(saved.text_size.percent(), 120);
}
