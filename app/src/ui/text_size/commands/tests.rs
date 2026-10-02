//! The text-size commands, driven by real keystrokes through the app's own
//! `shell::init` keymap.

use super::{DECREASE_COMMAND_ID, INCREASE_COMMAND_ID, RESET_COMMAND_ID};
use crate::command::{CommandRegistry, MenuSlot, build_items};
use crate::config::ui::{TextSize, Theme as ThemePreference};
use crate::keymap::{KeymapConfig, conflicts};
use crate::ui::text_size::current;
use crate::util::shell::MainWindow;
use gpui_kit::{Keystroke, TestAppContext, VisualTestContext, WindowHandle};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

const IDS: [&str; 3] = [INCREASE_COMMAND_ID, DECREASE_COMMAND_ID, RESET_COMMAND_ID];

fn temp_path(name: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "fernrohr-text-size-cmd-{name}-{}-{n}.toml",
        std::process::id()
    ))
}

/// The app as `main` starts it, with a picker window focused.
fn app(cx: &mut TestAppContext) -> WindowHandle<MainWindow> {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_path("workspace"), temp_path("keymap"));
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        crate::ui::theme::init(ThemePreference::Light, cx);
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

fn percent(vcx: &mut VisualTestContext) -> u16 {
    vcx.update(|_, cx| current(cx).percent())
}

#[gpui_kit::test]
async fn cmd_equals_steps_up_and_stops_at_the_maximum(cx: &mut TestAppContext) {
    let main = app(cx);
    let mut vcx = VisualTestContext::from_window(main.into(), cx);
    press(&mut vcx, "cmd-=");
    assert_eq!(percent(&mut vcx), 110);
    for _ in 0..TextSize::STEPS.len() {
        press(&mut vcx, "cmd-=");
    }
    assert_eq!(percent(&mut vcx), TextSize::MAX.percent());
}

#[gpui_kit::test]
async fn cmd_minus_steps_down_and_stops_at_the_minimum(cx: &mut TestAppContext) {
    let main = app(cx);
    let mut vcx = VisualTestContext::from_window(main.into(), cx);
    press(&mut vcx, "cmd--");
    assert_eq!(percent(&mut vcx), 90);
    for _ in 0..TextSize::STEPS.len() {
        press(&mut vcx, "cmd--");
    }
    assert_eq!(percent(&mut vcx), TextSize::MIN.percent());
}

#[gpui_kit::test]
async fn cmd_shift_0_resets_to_the_default(cx: &mut TestAppContext) {
    let main = app(cx);
    let mut vcx = VisualTestContext::from_window(main.into(), cx);
    press(&mut vcx, "cmd-=");
    press(&mut vcx, "cmd-=");
    assert_eq!(percent(&mut vcx), 120);
    press(&mut vcx, "cmd-shift-0");
    assert_eq!(percent(&mut vcx), TextSize::DEFAULT.percent());
}

/// The app's whole registry, as `shell::init` builds it.
fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    registry
}

#[test]
fn all_three_are_in_the_view_menu_and_the_palette() {
    let registry = registry();
    let view: Vec<&str> = registry
        .for_menu(MenuSlot::View)
        .iter()
        .map(|command| command.id)
        .collect();
    for id in IDS {
        assert!(view.contains(&id), "{id} in the View menu");
        let command = registry.get(id).expect("registered");
        assert_eq!(command.context, None, "{id} is global");
    }

    let mut only_these = CommandRegistry::new();
    super::register_commands(&mut only_these);
    assert_eq!(
        build_items(&only_these, &[]).len(),
        3,
        "all reach the palette"
    );
}

#[test]
fn their_default_keys_collide_with_no_other_command() {
    let registry = registry();
    let config = KeymapConfig::default();
    for id in IDS {
        let keys = registry.get(id).expect("registered").default_binding;
        let found = conflicts(&registry, &config, id, keys);
        assert!(found.same_scope.is_empty(), "{id} ({keys}): {found:?}");
        assert!(found.shadows.is_empty(), "{id} ({keys}): {found:?}");
    }
    // The key most apps use for Reset is Focus Resources here; the check above
    // would catch it.
    let taken = conflicts(&registry, &config, RESET_COMMAND_ID, "cmd-0");
    assert_eq!(taken.same_scope, vec!["resource.focus"]);
}
