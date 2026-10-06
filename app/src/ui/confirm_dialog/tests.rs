use super::{Confirmation, open};
use crate::ui::confirm_text::ConfirmText;
use crate::ui::typography::recorder::{RecordingTextSystem, with_recorded_text};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{
    AppContext as _, Context, FocusHandle, IntoElement, Keystroke, Render, TestAppContext,
    VisualTestContext, Window, div,
};
use std::cell::Cell;
use std::rc::Rc;

struct Host {
    focus: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::InteractiveElement as _;
        div().track_focus(&self.focus)
    }
}

/// A window over a focused host, with a confirmation open on it; `confirmed`
/// counts its confirms.
fn opened(cx: &mut TestAppContext) -> (VisualTestContext, Rc<Cell<usize>>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
    });
    let window = cx.add_window(|window, cx| {
        let host = cx.new(|cx| Host {
            focus: cx.focus_handle(),
        });
        let focus = host.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root::new(host, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let confirmed = Rc::new(Cell::new(0));
    let count = confirmed.clone();
    vcx.update(|window, cx| {
        let confirmation = Confirmation {
            title: "Delete Secret?".into(),
            body: ConfirmText::from("Delete Secret ")
                .name("db-password")
                .text("?"),
            confirm: "Delete".into(),
            id_prefix: "test-delete",
        };
        open(
            confirmation,
            move |_, _| count.set(count.get() + 1),
            window,
            cx,
        );
    });
    vcx.run_until_parked();
    (vcx, confirmed)
}

fn dialog_open(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.has_active_dialog(cx))
}

fn key_label(key: &str) -> String {
    Kbd::format(&Keystroke::parse(key).expect("valid keystroke"))
}

fn drawn(recorded: &RecordingTextSystem, text: &str) -> bool {
    recorded.families_of(text).is_some()
}

/// Both buttons show their key - Enter on the confirm button, Escape on
/// Cancel - beside their labels, and the name in the question is a run of its
/// own.
#[test]
fn both_buttons_show_their_keys() {
    with_recorded_text(|cx, recorded| {
        let (_vcx, _) = opened(cx);
        for text in ["Cancel", "Delete", "db-password"] {
            assert!(drawn(recorded, text), "{text:?} is drawn");
        }
        let (enter, escape) = (key_label("enter"), key_label("escape"));
        assert!(
            drawn(recorded, &enter),
            "the confirm key {enter:?} is drawn"
        );
        assert!(
            drawn(recorded, &escape),
            "the cancel key {escape:?} is drawn"
        );
        let families = recorded.families_of("db-password").expect("drawn");
        assert_eq!(families.len(), 3, "text, name, text: {families:?}");
    });
}

/// Enter confirms - the key the confirm button shows - and closes the dialog.
#[gpui_kit::test]
async fn enter_confirms(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened(cx);
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert_eq!(confirmed.get(), 1, "Enter ran the confirm once");
    assert!(!dialog_open(&mut vcx), "and closed the dialog");
}

/// Escape cancels: nothing runs, and the dialog closes.
#[gpui_kit::test]
async fn escape_cancels(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened(cx);
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert_eq!(confirmed.get(), 0, "Escape confirmed nothing");
    assert!(!dialog_open(&mut vcx), "and closed the dialog");
}

/// Clicks `id`'s centre.
fn click(vcx: &mut VisualTestContext, id: &'static str) {
    use gpui_kit::test::TestWindowExt as _;
    let at = vcx.update(|window, cx| {
        window.render_frame(cx);
        window.find(id).bounds().center()
    });
    vcx.simulate_click(at, gpui_kit::Modifiers::none());
    vcx.run_until_parked();
}

/// Enter and the confirm button act alike: each closes the dialog and runs
/// the action exactly once.
#[gpui_kit::test]
async fn enter_and_the_confirm_button_each_close_and_act_once(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened(cx);
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert!(!dialog_open(&mut vcx), "Enter closed it");
    assert_eq!(confirmed.get(), 1, "and acted once");

    let (mut vcx, confirmed) = opened(cx);
    click(
        &mut vcx,
        super::confirm_id("test-delete").to_string().leak(),
    );
    assert!(!dialog_open(&mut vcx), "the button closed it");
    assert_eq!(confirmed.get(), 1, "and acted once");
}

/// A dialog the action opens - a follow-up question, as the forward picker
/// leads to the stop confirmation - stays open, by Enter as by the button.
#[gpui_kit::test]
async fn a_dialog_the_action_opens_stays_open(cx: &mut TestAppContext) {
    for by_enter in [true, false] {
        let (mut vcx, _) = opened(cx);
        // Replace the open confirmation with one whose action opens another.
        vcx.update(|window, cx| {
            window.close_dialog(cx);
            let confirmation = Confirmation {
                title: "First?".into(),
                body: ConfirmText::from("First?"),
                confirm: "Go".into(),
                id_prefix: "first",
            };
            open(
                confirmation,
                |window, cx| {
                    let follow_up = Confirmation {
                        title: "Second?".into(),
                        body: ConfirmText::from("Second?"),
                        confirm: "Go".into(),
                        id_prefix: "second",
                    };
                    open(follow_up, |_, _| {}, window, cx);
                },
                window,
                cx,
            );
        });
        vcx.run_until_parked();
        if by_enter {
            vcx.simulate_keystrokes("enter");
            vcx.run_until_parked();
        } else {
            click(&mut vcx, super::confirm_id("first").to_string().leak());
        }
        assert!(
            dialog_open(&mut vcx),
            "the follow-up is open (by Enter: {by_enter})"
        );
    }
}
