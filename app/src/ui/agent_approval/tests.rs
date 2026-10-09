//! The approval dialog from the keyboard, as `keyboard-first.md` asks: real
//! keystrokes through the app's keymap, both tiers.

use super::{Question, open_in, withdraw};
use crate::mcp::approval::{ApprovalRequest, Asking};
use crate::ui::typography::recorder::{RecordingTextSystem, with_recorded_text};
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{
    AppContext as _, Context, FocusHandle, IntoElement, Render, TestAppContext, VisualTestContext,
    Window, div,
};
use tokio::sync::oneshot::{self, error::TryRecvError};

struct Host {
    focus: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::InteractiveElement as _;
        div().track_focus(&self.focus)
    }
}

fn request(irreversible: bool) -> ApprovalRequest {
    ApprovalRequest {
        title: "Delete 2 Pods?".into(),
        confirm: "Delete".into(),
        tool: "delete_pods".into(),
        context: "dev".into(),
        namespace: "team-a".into(),
        kind: "Pod".into(),
        targets: vec!["api-7d9f".into(), "web-55c2".into()],
        parameters: vec![("Replicas".into(), "2 → 3".into())],
        asking: Asking::Action { irreversible },
    }
}

/// A window with the app's keymap bound and `request` asked in it.
fn asked(
    cx: &mut TestAppContext,
    request: ApprovalRequest,
) -> (VisualTestContext, oneshot::Receiver<bool>, Question) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
        let mut registry = crate::command::CommandRegistry::new();
        crate::ui::confirm_dialog::register_commands(&mut registry);
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
    let (reply, answer) = oneshot::channel();
    let question = cx
        .update(|cx| open_in(window.into(), request, reply, cx))
        .expect("the question opens");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    crate::ui::confirm_dialog::deliver_first_frame(&mut vcx);
    (vcx, answer, question)
}

fn dialog_open(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.has_active_dialog(cx))
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    vcx.simulate_keystrokes(keys);
    vcx.run_until_parked();
}

#[gpui_kit::test]
async fn enter_denies_an_irreversible_action(cx: &mut TestAppContext) {
    let (mut vcx, mut answer, _) = asked(cx, request(true));
    press(&mut vcx, "enter");
    assert!(!dialog_open(&mut vcx));
    assert_eq!(answer.try_recv(), Ok(false));
}

#[gpui_kit::test]
async fn the_shortcut_allows_an_irreversible_action(cx: &mut TestAppContext) {
    let (mut vcx, mut answer, _) = asked(cx, request(true));
    press(&mut vcx, "secondary-backspace");
    assert!(!dialog_open(&mut vcx));
    assert_eq!(answer.try_recv(), Ok(true));
}

#[gpui_kit::test]
async fn tab_then_enter_allows_an_irreversible_action(cx: &mut TestAppContext) {
    let (mut vcx, mut answer, _) = asked(cx, request(true));
    press(&mut vcx, "tab");
    press(&mut vcx, "enter");
    assert_eq!(answer.try_recv(), Ok(true));
}

#[gpui_kit::test]
async fn escape_denies(cx: &mut TestAppContext) {
    for irreversible in [true, false] {
        let (mut vcx, mut answer, _) = asked(cx, request(irreversible));
        press(&mut vcx, "escape");
        assert!(!dialog_open(&mut vcx));
        assert_eq!(answer.try_recv(), Ok(false), "irreversible: {irreversible}");
    }
}

#[gpui_kit::test]
async fn enter_allows_a_recoverable_action(cx: &mut TestAppContext) {
    let (mut vcx, mut answer, _) = asked(cx, request(false));
    press(&mut vcx, "enter");
    assert!(!dialog_open(&mut vcx));
    assert_eq!(answer.try_recv(), Ok(true));
}

#[gpui_kit::test]
async fn a_withdrawn_question_closes_and_answers_nothing(cx: &mut TestAppContext) {
    let (mut vcx, mut answer, question) = asked(cx, request(true));
    // As the gate does: from a job on the main thread, not inside a window.
    cx.update(|cx| withdraw(question, cx));
    vcx.run_until_parked();
    assert!(!dialog_open(&mut vcx));
    crate::ui::confirm_dialog::deliver_first_frame(&mut vcx);
    assert_eq!(answer.try_recv(), Err(TryRecvError::Closed));
}

#[test]
fn the_dialog_shows_every_target_and_parameter() {
    with_recorded_text(|cx, recorded: &RecordingTextSystem| {
        let _asked = asked(cx, request(true));
        for text in [
            "Delete 2 Pods?",
            "delete_pods",
            "dev",
            "team-a",
            "api-7d9f",
            "web-55c2",
            "Replicas",
            "2 → 3",
            "Cancel",
            "Delete",
        ] {
            assert!(recorded.families_of(text).is_some(), "{text:?} is drawn");
        }
    });
}

/// `mcp-connect-and-focus` 2.1: connecting a context is the recoverable tier,
/// so Enter allows it and Escape denies it.
#[gpui_kit::test]
async fn enter_allows_a_connection(cx: &mut TestAppContext) {
    let request = ApprovalRequest::connect(
        "staging",
        Some(("qa-bastion", crate::config::tunnels::TunnelKind::Ssh)),
    );
    assert_eq!(
        super::details(&request)
            .iter()
            .map(|detail| (detail.label.to_string(), detail.value.to_string()))
            .collect::<Vec<_>>(),
        [
            ("Agent tool".to_string(), "connect_context".to_string()),
            ("Context".to_string(), "staging".to_string()),
            ("Tunnel".to_string(), "qa-bastion (SSH tunnel)".to_string()),
        ],
        "a connection names its context and tunnel, and no namespace or kind"
    );
    let (mut vcx, mut answer, _) = asked(cx, request);
    press(&mut vcx, "enter");
    assert!(!dialog_open(&mut vcx));
    assert_eq!(answer.try_recv(), Ok(true));
}

#[gpui_kit::test]
async fn escape_denies_a_connection(cx: &mut TestAppContext) {
    let (mut vcx, mut answer, _) = asked(cx, ApprovalRequest::connect("staging", None));
    press(&mut vcx, "escape");
    assert!(!dialog_open(&mut vcx));
    assert_eq!(answer.try_recv(), Ok(false));
}
