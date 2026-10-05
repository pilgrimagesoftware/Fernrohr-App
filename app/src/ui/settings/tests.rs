//! Window-level tests for the Settings window and its Keyboard Shortcuts
//! section, driven by real keystrokes through the app's own `shell::init`.

use super::{SettingsWindow, ShortcutsSection, test_window};
use crate::command::CommandRegistry;
use crate::keymap::LiveKeymap;
use crate::ui::panel::focus::FocusNextPanel;
use crate::util::shell::MainWindow;
use gpui_kit::component::Root;
use gpui_kit::{
    Action, App, Entity, KeyContext, Keystroke, TestAppContext, VisualTestContext, WindowHandle,
};

fn temp_path(name: &str) -> std::path::PathBuf {
    crate::util::test_paths::temp_path(&format!("settings-{name}"))
}

/// The app as `main` starts it, with a picker window focused.
fn app(cx: &mut TestAppContext) -> WindowHandle<MainWindow> {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_path("workspace"), temp_path("keymap"));
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, workspace, &keymap);
    });
    let window = cx.add_window(MainWindow::test_picker_window);
    window
        .update(cx, |main_window, window, cx| {
            main_window.test_focus(window, cx)
        })
        .unwrap();
    window
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    let keys = Keystroke::parse(keys).expect("valid").unparse();
    cx.simulate_keystrokes(&keys);
    cx.run_until_parked();
}

/// Opens Settings with `cmd-,` from `main` and returns its window and section.
fn open(
    main: WindowHandle<MainWindow>,
    cx: &mut TestAppContext,
) -> (WindowHandle<Root>, Entity<ShortcutsSection>) {
    let mut vcx = VisualTestContext::from_window(main.into(), cx);
    press(&mut vcx, "cmd-,");
    let handle = cx
        .update(|cx| test_window(cx))
        .expect("cmd-, opened Settings");
    let section = handle
        .update(cx, |root, _window, cx| {
            root.view()
                .clone()
                .downcast::<SettingsWindow>()
                .expect("the Settings window's view")
                .read(cx)
                .shortcuts()
        })
        .unwrap();
    (handle, section)
}

fn resolves(cx: &App, key: &str, action: &dyn Action) -> bool {
    let keystroke = Keystroke::parse(key).expect("a valid keystroke");
    let (matched, _) = cx
        .key_bindings()
        .borrow()
        .bindings_for_input(std::slice::from_ref(&keystroke), &[] as &[KeyContext]);
    matched
        .iter()
        .any(|binding| binding.action().partial_eq(action))
}

/// `keys` as a recording stores them on this platform - `cmd` reads `super`
/// off macOS.
fn spelled(keys: &str) -> String {
    Keystroke::parse(keys).expect("a valid keystroke").unparse()
}

/// Close Window's key: `cmd-w` on macOS, `ctrl-w` elsewhere.
const CLOSE_KEY: &str = if cfg!(target_os = "macos") {
    "cmd-w"
} else {
    "ctrl-w"
};

fn override_of(cx: &App, id: &str) -> Option<String> {
    cx.global::<LiveKeymap>().config().bindings.get(id).cloned()
}

#[gpui_kit::test]
async fn cmd_comma_opens_one_settings_window(cx: &mut TestAppContext) {
    let main = app(cx);
    let before = cx.update(|cx| cx.windows().len());
    let (handle, _section) = open(main, cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    press(&mut vcx, "cmd-,");
    assert_eq!(
        cx.update(|cx| cx.windows().len()),
        before + 1,
        "a second cmd-, focuses the open window"
    );
    let in_app_menu = cx.update(|cx| {
        cx.global::<CommandRegistry>()
            .for_menu(crate::command::TopMenu::App)
            .iter()
            .any(|command| command.id == "settings.open")
    });
    assert!(in_app_menu, "Settings… is in the App menu");
}

#[gpui_kit::test]
async fn every_registered_command_has_a_row(cx: &mut TestAppContext) {
    let main = app(cx);
    let (_handle, section) = open(main, cx);
    cx.update(|cx| {
        let ids: Vec<&str> = section
            .read(cx)
            .visible_rows(cx)
            .iter()
            .map(|row| row.id)
            .collect();
        let registered: Vec<&str> = cx
            .global::<CommandRegistry>()
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, registered);
    });
}

/// `k9s-remaining-keybindings` 8.2, the editor half: each command the change
/// added has a row to rebind it, with no editor code of its own - the rows are
/// the registry's.
#[gpui_kit::test]
async fn every_new_k9s_command_has_an_editor_row(cx: &mut TestAppContext) {
    let main = app(cx);
    let (_handle, section) = open(main, cx);
    cx.update(|cx| {
        let ids: Vec<&str> = section
            .read(cx)
            .visible_rows(cx)
            .iter()
            .map(|row| row.id)
            .collect();
        for id in [
            "pods.delete",
            "pods.kill",
            "pods.shell",
            "pods.port_forward",
            "services.port_forward",
            "object_detail.edit",
            "object_detail.save_edit",
            "object_detail.cancel_edit",
            "logs.toggle_previous",
            "namespaces.jump_all",
            "namespaces.jump_9",
            "global.show_key_hints",
        ] {
            assert!(
                ids.contains(&id),
                "{id} has a row in the keybindings editor"
            );
        }
    });
}

/// Recording Close Window's key records it (and asks, since Close Window has
/// it) instead of closing the window; Escape cancels.
#[gpui_kit::test]
async fn recording_a_bound_key_does_not_run_it(cx: &mut TestAppContext) {
    let main = app(cx);
    let (handle, section) = open(main, cx);
    cx.update(|cx| section.update(cx, |s, cx| s.select("panel.focus_next", cx)));
    let before = cx.update(|cx| override_of(cx, "panel.focus_next"));
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);

    press(&mut vcx, "enter");
    assert_eq!(cx.update(|cx| section.read(cx).test_mode()), "recording");
    // Which action each keystroke ran, as GPUI reports it after dispatch.
    let fired = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let _observer = cx.update(|cx| {
        let fired = fired.clone();
        cx.observe_keystrokes(move |event, _window, _cx| {
            if let Some(action) = &event.action {
                fired.borrow_mut().push(action.name());
            }
        })
    });
    press(&mut vcx, CLOSE_KEY);
    assert!(
        fired.borrow().is_empty(),
        "Close Window's key ran nothing while recording: {:?}",
        fired.borrow()
    );
    assert_eq!(
        cx.update(|cx| section.read(cx).test_mode()),
        "confirming",
        "the key is Close Window's, so the editor asks"
    );

    press(&mut vcx, "escape");
    assert_eq!(cx.update(|cx| section.read(cx).test_mode()), "browsing");
    assert_eq!(
        cx.update(|cx| override_of(cx, "panel.focus_next")),
        before,
        "declined: unchanged"
    );
    assert!(
        cx.update(|cx| resolves(cx, "cmd-]", &FocusNextPanel)),
        "the key still works"
    );
}

/// A recorded key works at once, with no restart, and is saved.
#[gpui_kit::test]
async fn a_recorded_key_works_immediately(cx: &mut TestAppContext) {
    let main = app(cx);
    let (handle, section) = open(main, cx);
    cx.update(|cx| section.update(cx, |s, cx| s.select("panel.focus_next", cx)));
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);

    press(&mut vcx, "enter");
    press(&mut vcx, "cmd-shift-j");
    cx.update(|cx| {
        assert_eq!(section.read(cx).test_mode(), "browsing");
        assert_eq!(
            override_of(cx, "panel.focus_next"),
            Some(spelled("cmd-shift-j"))
        );
        assert!(
            resolves(cx, "cmd-shift-j", &FocusNextPanel),
            "the new key works"
        );
        assert!(
            !resolves(cx, "cmd-]", &FocusNextPanel),
            "the old one doesn't"
        );
    });
}

/// Confirming a conflict applies the key anyway.
#[gpui_kit::test]
async fn confirming_a_conflict_applies_the_key(cx: &mut TestAppContext) {
    let main = app(cx);
    let (handle, section) = open(main, cx);
    cx.update(|cx| section.update(cx, |s, cx| s.select("panel.focus_next", cx)));
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);

    press(&mut vcx, "enter");
    press(&mut vcx, "cmd-n");
    assert_eq!(cx.update(|cx| section.read(cx).test_mode()), "confirming");
    press(&mut vcx, "enter");
    cx.update(|cx| {
        assert_eq!(section.read(cx).test_mode(), "browsing");
        assert_eq!(override_of(cx, "panel.focus_next"), Some(spelled("cmd-n")));
    });
}

/// Backspace removes the selected command's key; cmd-backspace resets it.
#[gpui_kit::test]
async fn remove_and_reset_from_the_keyboard(cx: &mut TestAppContext) {
    let main = app(cx);
    let (handle, section) = open(main, cx);
    cx.update(|cx| section.update(cx, |s, cx| s.select("panel.focus_next", cx)));
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);

    press(&mut vcx, "backspace");
    cx.update(|cx| {
        assert_eq!(override_of(cx, "panel.focus_next").as_deref(), Some(""));
        assert!(!resolves(cx, "cmd-]", &FocusNextPanel), "removed: no key");
        let palette = crate::command::build_items(cx.global::<CommandRegistry>(), &[]);
        assert!(
            palette.len() == cx.global::<CommandRegistry>().available(&[]).len(),
            "still in the palette"
        );
    });
    press(&mut vcx, "cmd-backspace");
    cx.update(|cx| {
        assert_eq!(override_of(cx, "panel.focus_next"), None);
        assert!(
            resolves(cx, "cmd-]", &FocusNextPanel),
            "reset: the default again"
        );
    });
}

/// ↓/↑ move the selection; `/` moves focus to the filter.
#[gpui_kit::test]
async fn arrows_move_the_selection_and_slash_filters(cx: &mut TestAppContext) {
    let main = app(cx);
    let (handle, section) = open(main, cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let first = cx.update(|cx| section.read(cx).test_selected(cx));

    press(&mut vcx, "down");
    let second = cx.update(|cx| section.read(cx).test_selected(cx));
    assert_ne!(first, second, "down moved the selection");
    press(&mut vcx, "up");
    assert_eq!(cx.update(|cx| section.read(cx).test_selected(cx)), first);

    press(&mut vcx, "/");
    let filter_focused = handle
        .update(cx, |_, window, cx| {
            section.read(cx).test_filter_focus(cx).is_focused(window)
        })
        .unwrap();
    assert!(filter_focused, "/ focused the filter field");
}
