//! The Agent access section from the keyboard: reached from the sidebar,
//! each command copied by Tab and Space, every copy button's tooltip naming
//! what it copies, and the warning and unavailable states in place of
//! commands.

use super::super::agent_access::{
    COMMANDS_SELECTOR, SNIPPET_COPY_ID, SNIPPET_TOOLTIP, UNAVAILABLE_SELECTOR, UNSTABLE_SELECTOR,
    copy_id,
};
use super::super::{Section, ShowAgentAccess};
use super::layout::{drawn, press_button, show, shown};
use super::{app, open};
use crate::mcp::setup::{AgentSetup, HARNESSES, Unstable, opencode_snippet};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ElementId, Modifiers, TestAppContext, VisualTestContext, WindowHandle,
};
use std::path::{Path, PathBuf};

const EXE: &str = "/Applications/Fernrohr.app/Contents/MacOS/fernrohr";

/// Settings opened with `setup` in effect, showing Agent access.
fn opened(cx: &mut TestAppContext, setup: AgentSetup) -> (WindowHandle<Root>, VisualTestContext) {
    let main = app(cx);
    cx.update(|cx| crate::ui::agent_setup::set_setup(setup, cx));
    let (handle, _) = open(main, cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    press_button(&mut vcx, handle.into(), Section::AgentAccess.button_id());
    assert_eq!(shown(cx, handle), Section::AgentAccess);
    (handle, vcx)
}

fn ready() -> AgentSetup {
    AgentSetup::Ready {
        exe: PathBuf::from(EXE),
    }
}

fn clipboard(vcx: &mut VisualTestContext) -> Option<String> {
    vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn has(vcx: &mut VisualTestContext, handle: WindowHandle<Root>, id: &str) -> bool {
    vcx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(ElementId::Name(id.to_string().into()))
            .is_some()
    })
    .unwrap()
}

#[gpui_kit::test]
async fn each_harness_command_is_copied_from_the_keyboard(cx: &mut TestAppContext) {
    let (handle, mut vcx) = opened(cx, ready());
    assert!(drawn(&mut vcx, handle.into(), COMMANDS_SELECTOR));
    for harness in &HARNESSES {
        press_button(&mut vcx, handle.into(), &copy_id(harness.id));
        assert_eq!(
            clipboard(&mut vcx),
            Some(harness.command(Path::new(EXE))),
            "{}",
            harness.name
        );
    }
}

#[gpui_kit::test]
async fn claude_codes_button_copies_a_user_scoped_command_for_this_executable(
    cx: &mut TestAppContext,
) {
    let (handle, mut vcx) = opened(cx, ready());
    press_button(&mut vcx, handle.into(), &copy_id("claude-code"));
    assert_eq!(
        clipboard(&mut vcx),
        Some(format!(
            "claude mcp add --scope user fernrohr -- '{EXE}' mcp"
        ))
    );
}

#[gpui_kit::test]
async fn opencodes_config_snippet_is_copyable_too(cx: &mut TestAppContext) {
    let (handle, mut vcx) = opened(cx, ready());
    press_button(&mut vcx, handle.into(), SNIPPET_COPY_ID);
    assert_eq!(clipboard(&mut vcx), Some(opencode_snippet(Path::new(EXE))));
}

#[gpui_kit::test]
async fn every_copy_button_names_what_it_copies(cx: &mut TestAppContext) {
    let (handle, mut vcx) = opened(cx, ready());
    let buttons: Vec<(String, &'static str)> = HARNESSES
        .iter()
        .map(|harness| (copy_id(harness.id), harness.copy_tooltip))
        .chain([(SNIPPET_COPY_ID.to_string(), SNIPPET_TOOLTIP)])
        .collect();
    for (id, tooltip) in buttons {
        let at = vcx
            .update_window(handle.into(), |_, window, cx| {
                window.render_frame(cx);
                window
                    .try_find(ElementId::Name(id.clone().into()))
                    .unwrap_or_else(|| panic!("{id} is drawn"))
                    .bounds()
                    .center()
            })
            .unwrap();
        // Away first, so the last button's tooltip is gone and can't cover
        // this one.
        vcx.simulate_mouse_move(
            gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
            None,
            Modifiers::none(),
        );
        vcx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        vcx.run_until_parked();
        vcx.simulate_mouse_move(at, None, Modifiers::none());
        vcx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        vcx.run_until_parked();
        let selector: &'static str = crate::ui::icon_tooltip::selector(tooltip).leak();
        assert!(
            drawn(&mut vcx, handle.into(), selector),
            "{id} shows {tooltip:?}"
        );
    }
}

#[gpui_kit::test]
async fn an_unstable_path_warns_and_offers_no_command(cx: &mut TestAppContext) {
    let setup = AgentSetup::Unstable {
        exe: PathBuf::from("/src/app/target/debug/fernrohr"),
        why: Unstable::BuildTree,
    };
    let (handle, mut vcx) = opened(cx, setup);
    assert!(drawn(&mut vcx, handle.into(), UNSTABLE_SELECTOR));
    assert!(!drawn(&mut vcx, handle.into(), COMMANDS_SELECTOR));
    assert!(!has(&mut vcx, handle, &copy_id("claude-code")));
}

#[gpui_kit::test]
async fn without_an_endpoint_the_section_says_so_and_offers_nothing(cx: &mut TestAppContext) {
    let (handle, mut vcx) = opened(cx, AgentSetup::Unavailable);
    assert!(drawn(&mut vcx, handle.into(), UNAVAILABLE_SELECTOR));
    assert!(!drawn(&mut vcx, handle.into(), COMMANDS_SELECTOR));
    assert!(!has(&mut vcx, handle, &copy_id("codex")));
}

#[gpui_kit::test]
async fn the_palette_command_shows_the_section(cx: &mut TestAppContext) {
    let (handle, mut vcx) = opened(cx, ready());
    show(&mut vcx, Box::new(super::super::ShowKeyboardShortcuts));
    show(&mut vcx, Box::new(ShowAgentAccess));
    assert_eq!(shown(cx, handle), Section::AgentAccess);
}
