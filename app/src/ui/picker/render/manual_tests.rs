//! `manual-confirmation-tunnels` 4.3: the picker's status line while the context
//! being connected awaits a manual tunnel's confirmation - what it says, Proceed and
//! Cancel there with their keys, and what it says once answered.

use super::{PICKER_CANCEL_ID, PICKER_PROCEED_ID};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::tunnel::manual::{Decision, ManualConfirmations};
use crate::ui::picker::ClusterPicker;
use crate::ui::typography::recorder::{RecordingTextSystem, with_recorded_text};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ElementId, Entity, Modifiers, TestAppContext, VisualTestContext, WindowHandle,
};
use tokio::sync::oneshot::Receiver;

const MESSAGE: &str = "Connect the corporate VPN";

struct Harness {
    vcx: VisualTestContext,
    picker: Entity<ClusterPicker>,
    connection: Entity<ClusterConnection>,
    answer: Receiver<Decision>,
}

/// A picker whose attempt on `kind-dev` waits on `corp-vpn`.
fn waiting(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::tunnel::manual::ManualConfirmations::init(cx);
    });
    let mut built = None;
    let window: WindowHandle<Root> = cx.add_window(|window, cx| {
        let picker = cx.new(|cx| ClusterPicker::new(window, cx));
        built = Some(picker.clone());
        Root::new(picker, window, cx)
    });
    let picker = built.expect("the window built its picker");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    let connection =
        cx.new(|_| ClusterConnection::test_with_state(ConnectionState::WaitingForTunnel));
    let answer = cx.update(|cx| {
        let answer = ManualConfirmations::insert_test_pending(
            cx,
            "corp-vpn-id",
            "corp-vpn",
            Some(MESSAGE),
            &["kind-dev"],
        );
        picker.update(cx, |picker, cx| {
            // The picker's contexts come from the host's kubeconfig, and with none
            // it draws only "no contexts" - so give it its own, keeping the test
            // independent of the machine it runs on.
            picker.contexts = Ok(vec!["kind-dev".to_string()]);
            picker.test_attempt("kind-dev", connection.clone(), cx)
        });
        answer
    });
    vcx.run_until_parked();
    Harness {
        vcx,
        picker,
        connection,
        answer,
    }
}

fn has(h: &mut Harness, id: &'static str) -> bool {
    h.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(ElementId::Name(id.into())).is_some()
    })
}

fn says(recorded: &RecordingTextSystem, text: &str) -> bool {
    recorded.lines().iter().any(|(line, _)| line.contains(text))
}

#[test]
fn the_status_line_asks_for_confirmation_with_the_instruction() {
    with_recorded_text(|cx, recorded| {
        let mut h = waiting(cx);
        assert!(has(&mut h, PICKER_PROCEED_ID), "Proceed is offered");
        assert!(has(&mut h, PICKER_CANCEL_ID), "Cancel is offered");
        assert!(
            says(recorded, "kind-dev is waiting for you to confirm corp-vpn."),
            "names the context and tunnel"
        );
        assert!(says(recorded, MESSAGE), "shows the instruction");
        let _ = &h.picker;
    });
}

#[gpui_kit::test]
async fn proceed_in_the_status_line_answers_the_tunnel(cx: &mut TestAppContext) {
    let mut h = waiting(cx);
    let at = h.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.find(PICKER_PROCEED_ID).bounds().center()
    });
    h.vcx.simulate_click(at, Modifiers::none());
    h.vcx.run_until_parked();
    assert_eq!(h.answer.try_recv(), Ok(Decision::Proceed));
}

#[gpui_kit::test]
async fn the_proceed_key_works_from_the_picker(cx: &mut TestAppContext) {
    let mut h = waiting(cx);
    // The app's own bindings, so the key resolves as it does at launch.
    h.vcx.update(|_, cx| {
        let mut registry = crate::command::CommandRegistry::new();
        crate::ui::manual_tunnel::register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        crate::ui::manual_tunnel::register_handler(cx);
    });
    let focus = h.vcx.update(|_, cx| h.picker.read(cx).focus_handle.clone());
    h.vcx.update(|window, cx| window.focus(&focus, cx));
    let keys = gpui_kit::Keystroke::parse("secondary-alt-p")
        .expect("valid")
        .unparse();
    h.vcx.simulate_keystrokes(&keys);
    h.vcx.run_until_parked();
    assert_eq!(h.answer.try_recv(), Ok(Decision::Proceed));
}

#[test]
fn once_cancelled_the_line_reports_the_reason_and_offers_nothing() {
    with_recorded_text(|cx, recorded| {
        let mut h = waiting(cx);
        let connection = h.connection.clone();
        h.vcx.update(|_, cx| {
            ManualConfirmations::resolve(cx, "corp-vpn-id", Decision::Cancel);
            connection.update(cx, |connection, cx| {
                connection.state = ConnectionState::Failed("corp-vpn was cancelled".into());
                cx.notify();
            });
        });
        h.vcx.run_until_parked();
        assert!(!has(&mut h, PICKER_PROCEED_ID));
        assert!(!has(&mut h, PICKER_CANCEL_ID));
        assert!(says(
            recorded,
            "Could not connect to kind-dev: corp-vpn was cancelled"
        ));
    });
}
