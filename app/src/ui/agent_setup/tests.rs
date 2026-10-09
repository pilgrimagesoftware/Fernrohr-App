//! Copy MCP Setup Command without the mouse: the palette, the command's
//! name, Enter, then the harness picker's arrows and Enter.

use super::picker::{TITLE, row_selector};
use super::set_setup;
use crate::config::workspace::WindowLayout;
use crate::mcp::setup::{AgentSetup, HARNESSES};
use crate::util::shell::{TOGGLE_PALETTE_DEFAULT_BINDING, open_window};
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Keystroke, TestAppContext, VisualTestContext};
use std::path::{Path, PathBuf};

const EXE: &str = "/opt/Fernrohr/fernrohr";

/// The app as `main` starts it, `setup` in effect, one main window open and
/// focused.
fn app(cx: &mut TestAppContext, setup: AgentSetup) -> VisualTestContext {
    cx.executor().allow_parking();
    let workspace = crate::util::test_paths::temp_path("agent-setup-workspace");
    let keymap = crate::util::test_paths::temp_path("agent-setup-keymap");
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, workspace, &keymap);
        set_setup(setup, cx);
        open_window(cx, WindowLayout::default());
    });
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    VisualTestContext::from_window(window, cx)
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    let keys = Keystroke::parse(keys).expect("valid").unparse();
    vcx.simulate_keystrokes(&keys);
    vcx.run_until_parked();
}

/// Opens the palette and runs Copy MCP Setup Command, by typing its name.
fn run_command(vcx: &mut VisualTestContext) {
    press(vcx, TOGGLE_PALETTE_DEFAULT_BINDING);
    vcx.simulate_input("Copy MCP Setup Command");
    vcx.run_until_parked();
    press(vcx, "enter");
}

fn clipboard(vcx: &mut VisualTestContext) -> Option<String> {
    vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn picker_open(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.has_active_dialog(cx)
    }) && vcx
        .debug_bounds(row_selector(HARNESSES[0].id).leak())
        .is_some()
}

#[gpui_kit::test]
async fn the_palette_copies_codexs_command_from_the_keyboard(cx: &mut TestAppContext) {
    let mut vcx = app(
        cx,
        AgentSetup::Ready {
            exe: PathBuf::from(EXE),
        },
    );
    run_command(&mut vcx);
    assert!(picker_open(&mut vcx), "{TITLE} opened");
    // Codex is the second harness.
    press(&mut vcx, "down");
    press(&mut vcx, "enter");
    let codex = HARNESSES
        .iter()
        .find(|harness| harness.id == "codex")
        .unwrap();
    assert_eq!(clipboard(&mut vcx), Some(codex.command(Path::new(EXE))));
    assert!(!vcx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
async fn enter_at_once_copies_the_first_harness(cx: &mut TestAppContext) {
    let mut vcx = app(
        cx,
        AgentSetup::Ready {
            exe: PathBuf::from(EXE),
        },
    );
    run_command(&mut vcx);
    press(&mut vcx, "enter");
    assert_eq!(
        clipboard(&mut vcx),
        Some(HARNESSES[0].command(Path::new(EXE)))
    );
}

#[gpui_kit::test]
async fn escape_closes_the_picker_and_copies_nothing(cx: &mut TestAppContext) {
    let mut vcx = app(
        cx,
        AgentSetup::Ready {
            exe: PathBuf::from(EXE),
        },
    );
    let before = clipboard(&mut vcx);
    run_command(&mut vcx);
    assert!(picker_open(&mut vcx));
    press(&mut vcx, "escape");
    assert!(!vcx.update(|window, cx| window.has_active_dialog(cx)));
    assert_eq!(clipboard(&mut vcx), before);
}

#[gpui_kit::test]
async fn with_nothing_to_copy_the_command_opens_agent_access(cx: &mut TestAppContext) {
    let mut vcx = app(cx, AgentSetup::Unavailable);
    run_command(&mut vcx);
    let settings = vcx
        .update(|_, cx| crate::ui::settings::test_shown_section(cx))
        .expect("Settings opened");
    assert_eq!(settings, crate::ui::settings::Section::AgentAccess);
}
