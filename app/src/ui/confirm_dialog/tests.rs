use super::{Confirmation, Severity, open};
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

/// A window over a focused host, with a recoverable confirmation open on it;
/// `confirmed` counts its confirms.
fn opened(cx: &mut TestAppContext) -> (VisualTestContext, Rc<Cell<usize>>) {
    opened_with(cx, Severity::Recoverable)
}

/// [`opened`], at `severity`, with the app's keymap bound - the irreversible
/// shortcut is a registered command.
fn opened_with(
    cx: &mut TestAppContext,
    severity: Severity,
) -> (VisualTestContext, Rc<Cell<usize>>) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
        let mut registry = crate::command::CommandRegistry::new();
        super::register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
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
            severity,
        };
        open(
            confirmation,
            move |_, _| count.set(count.get() + 1),
            window,
            cx,
        );
    });
    super::deliver_first_frame(&mut vcx);
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

/// Space down and up: a `Button` clicks on the key's release.
fn press_space(vcx: &mut VisualTestContext) {
    let space = Keystroke::parse("space").expect("valid");
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    vcx.run_until_parked();
}

/// Irreversible: the confirm button shows its deliberate shortcut, not Enter.
#[test]
fn an_irreversible_confirm_shows_its_shortcut() {
    with_recorded_text(|cx, recorded| {
        let (_vcx, _) = opened_with(cx, Severity::Irreversible);
        let shortcut = key_label("secondary-backspace");
        assert!(drawn(recorded, &shortcut), "{shortcut:?} is drawn");
        assert!(
            !drawn(recorded, &key_label("enter")),
            "and Enter, which cancels, is not shown as the confirm key"
        );
    });
}

/// Irreversible: the dialog opens on Cancel, so Enter cancels.
#[gpui_kit::test]
async fn irreversible_enter_on_open_cancels(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened_with(cx, Severity::Irreversible);
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert_eq!(confirmed.get(), 0, "Enter confirmed nothing");
    assert!(!dialog_open(&mut vcx), "and closed the dialog");
}

/// Irreversible: Tab from Cancel reaches the confirm button, and Enter or
/// Space presses it.
#[gpui_kit::test]
async fn irreversible_tab_then_enter_confirms(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened_with(cx, Severity::Irreversible);
    vcx.simulate_keystrokes("tab enter");
    vcx.run_until_parked();
    assert_eq!(confirmed.get(), 1, "Tab then Enter confirmed once");
    assert!(!dialog_open(&mut vcx), "and closed the dialog");
}

#[gpui_kit::test]
async fn irreversible_tab_then_space_confirms(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened_with(cx, Severity::Irreversible);
    vcx.simulate_keystrokes("tab");
    press_space(&mut vcx);
    assert_eq!(confirmed.get(), 1, "Tab then Space confirmed once");
    assert!(!dialog_open(&mut vcx), "and closed the dialog");
}

/// Irreversible: the deliberate shortcut confirms from Cancel.
#[gpui_kit::test]
async fn irreversible_shortcut_confirms(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened_with(cx, Severity::Irreversible);
    vcx.simulate_keystrokes("secondary-backspace");
    vcx.run_until_parked();
    assert_eq!(confirmed.get(), 1, "the shortcut confirmed once");
    assert!(!dialog_open(&mut vcx), "and closed the dialog");
}

/// Irreversible: Escape cancels.
#[gpui_kit::test]
async fn irreversible_escape_cancels(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened_with(cx, Severity::Irreversible);
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert_eq!(confirmed.get(), 0, "Escape confirmed nothing");
    assert!(!dialog_open(&mut vcx), "and closed the dialog");
}

/// Recoverable too, Enter presses the focused button: Tab to Cancel, then
/// Enter, cancels.
#[gpui_kit::test]
async fn enter_on_a_focused_cancel_cancels(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened(cx);
    vcx.simulate_keystrokes("tab enter");
    vcx.run_until_parked();
    assert_eq!(confirmed.get(), 0, "Enter on Cancel confirmed nothing");
    assert!(!dialog_open(&mut vcx), "and closed the dialog");
}

/// Irreversible, closed before its first frame: its move to Cancel doesn't
/// carry over to the next dialog, which still opens on Cancel - Enter cancels.
#[gpui_kit::test]
async fn an_irreversible_dialog_closed_early_moves_no_focus(cx: &mut TestAppContext) {
    let (mut vcx, confirmed) = opened_with(cx, Severity::Irreversible);
    vcx.simulate_keystrokes("escape");
    let count = confirmed.clone();
    vcx.update(|window, cx| {
        let confirmation = Confirmation {
            title: "Delete Secret?".into(),
            body: ConfirmText::from("Delete it?"),
            confirm: "Delete".into(),
            id_prefix: "test-delete-again",
            severity: Severity::Irreversible,
        };
        open(
            confirmation,
            move |_, _| count.set(count.get() + 1),
            window,
            cx,
        );
    });
    super::deliver_first_frame(&mut vcx);
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert_eq!(confirmed.get(), 0, "Enter confirmed nothing");
    assert!(!dialog_open(&mut vcx), "and closed the dialog");
}
