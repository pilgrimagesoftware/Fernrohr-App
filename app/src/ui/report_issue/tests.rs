//! The Report Issue window in a test app with the real keymap bound, so the
//! keyboard routes run through the bindings a user's keys do.

use gpui_kit::component::input::TextareaState;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::window::{self, ReportIssueWindow};
use super::{
    KEY_CONTEXT, MenuSlot, REPORT_ISSUE_COMMAND_ID, ReportError, register_commands,
    register_handler,
};
use crate::command::CommandRegistry;

mod keyboard;
mod layout;
mod prefill;
mod route;
mod window_tests;

#[test]
fn report_issue_is_in_the_help_menu_and_the_window_commands_in_its_context() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let command = registry
        .get(REPORT_ISSUE_COMMAND_ID)
        .expect("report_issue is registered");
    assert_eq!(command.title, "Report Issue");
    assert_eq!(command.menu, Some(MenuSlot::Help));
    for id in [
        "report_issue.send",
        "report_issue.close",
        "report_issue.add_screenshots",
    ] {
        let command = registry
            .get(id)
            .unwrap_or_else(|| panic!("{id} is registered"));
        assert_eq!(command.context, Some(KEY_CONTEXT), "{id}");
    }
}

/// The real keymap, Report Issue's handlers, and a light theme.
pub(super) fn setup(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        register_handler(cx);
    });
}

/// The open Report Issue window and its view.
pub(super) struct Opened {
    pub(super) view: Entity<ReportIssueWindow>,
    pub(super) vcx: VisualTestContext,
}

/// Opens the window - prefilled from `prefill` when given - and draws it.
pub(super) fn open(cx: &mut TestAppContext, prefill: Option<ReportError>) -> Opened {
    cx.update(|cx| window::open_or_raise(prefill, cx));
    cx.run_until_parked();
    let (handle, view) = cx.update(window::test_view).expect("the window opened");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    cx.run_until_parked();
    Opened { view, vcx }
}

/// Types `subject` and `description` into the window's fields.
pub(super) fn fill(opened: &mut Opened, subject: &str, description: &str) {
    let view = opened.view.clone();
    opened.vcx.update(|window, cx| {
        let (subject_input, description_input) = {
            let view = view.read(cx);
            (view.subject.clone(), view.description.clone())
        };
        subject_input.update(cx, |input, cx| {
            input.set_value(subject.to_string(), window, cx)
        });
        description_input.update(cx, |input: &mut TextareaState, cx| {
            input.set_value(description.to_string(), window, cx)
        });
    });
    opened.vcx.run_until_parked();
}

/// Whether the Report Issue window is still open.
pub(super) fn is_open(cx: &mut TestAppContext) -> bool {
    cx.update(window::test_view).is_some()
}

/// Waits - `gh` runs as a real child process - until `done`.
pub(super) fn wait_until(cx: &mut TestAppContext, done: impl Fn(&mut TestAppContext) -> bool) {
    for _ in 0..400 {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("never finished");
}
