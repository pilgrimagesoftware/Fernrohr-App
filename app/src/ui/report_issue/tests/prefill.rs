//! #177: an error's Report… opens Report Issue with the error's text filled
//! in - reached by focus moving to it, as Tab moves it, and pressed with Enter.

use super::super::{LastForm, ReportError, register_handler};
use crate::ui::panel_title::{REPORT_ERROR_BUTTON, error_content};
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext, VisualTestContext};

#[test]
fn a_reports_subject_is_the_errors_first_line_and_its_body_both() {
    let report = ReportError::of(
        "Couldn't connect to dev",
        Some("unable to run auth exec: No such file or directory"),
    );
    assert_eq!(report.subject, "Couldn't connect to dev");
    assert_eq!(
        report.description,
        "Couldn't connect to dev\n\n```\nunable to run auth exec: No such file or directory\n```\n"
    );

    let long = ReportError::of(&"x".repeat(200), None);
    assert_eq!(long.subject, format!("{}\u{2026}", "x".repeat(80)));
    assert_eq!(long.description, format!("{}\n", "x".repeat(200)));
}

/// A panel showing a connection failure.
struct Failure;

impl gpui_kit::Render for Failure {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        error_content(
            "Couldn't connect to dev".to_string(),
            Some("unable to run auth exec: No such file or directory".to_string()),
            cx,
        )
    }
}

#[gpui_kit::test]
async fn tab_and_enter_on_report_open_it_filled_in(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        register_handler(cx);
    });
    let window = cx.update(|cx| {
        gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| Failure)
        })
        .expect("test window opens")
        .0
    });
    window
        .update(cx, |_, window, _| window.activate_window())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.run_until_parked();

    let id = ElementId::Name(REPORT_ERROR_BUTTON.into());
    let mut focused = false;
    // Tab's own move - this bare window has no keymap binding Tab to it.
    for _ in 0..10 {
        vcx.update(|window, cx| window.focus_next(cx));
        vcx.run_until_parked();
        focused = vcx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.try_find(id.clone()).and_then(|b| b.focused()) == Some(true)
            })
            .unwrap();
        if focused {
            break;
        }
    }
    assert!(focused, "Tab reaches Report…");
    // A key down *and* up, as a real press is: the click fires on release.
    let enter = gpui_kit::Keystroke::parse("enter").unwrap();
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: enter });
    vcx.run_until_parked();

    assert!(
        window
            .update(cx, |_, window, cx| window.has_active_dialog(cx))
            .unwrap(),
        "Report Issue opened"
    );
    let (subject, description) = cx.update(|cx| {
        let form = cx.global::<LastForm>();
        (
            form.subject.read(cx).value().to_string(),
            form.description.read(cx).value().to_string(),
        )
    });
    assert_eq!(subject, "Couldn't connect to dev");
    assert!(
        description.contains("unable to run auth exec: No such file or directory"),
        "{description}"
    );
    window
        .update(cx, |_, window, cx| window.close_all_dialogs(cx))
        .unwrap();
}
