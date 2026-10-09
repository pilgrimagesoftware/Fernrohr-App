//! `manual-confirmation-tunnels` section 4 through a real workspace window: a
//! context waiting on a manual tunnel's confirmation - its health, its capsule's
//! fill, icon, tooltip and place in the row, and (4.2) its menu.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::context_health::{ContextHealth, Severity};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::tunnel::manual::{Decision, ManualConfirmations};
use crate::ui::status_bar::{
    StatusItem, attention_selector, state_icon_selector, state_tooltip_text,
};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, init};
use gpui_kit::assets::IconName;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext};
use tokio::sync::oneshot;

pub(super) const MESSAGE: &str = "Connect the corporate VPN";

pub(super) struct Harness {
    pub(super) vcx: VisualTestContext,
    pub(super) main: Entity<MainWindow>,
}

/// A workspace window with `contexts`, each in its given connection state.
pub(super) fn harness(cx: &mut TestAppContext, contexts: &[(&str, ConnectionState)]) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        for (context, state) in contexts {
            ClusterRegistry::insert_test_session(cx, context, state.clone());
        }
    });
    let names: Vec<String> = contexts.iter().map(|(name, _)| name.to_string()).collect();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(names, window, cx));
        main.update(cx, |main, cx| main.focus_initial(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    Harness { vcx, main }
}

/// `corp-vpn` waiting on the user, with `contexts` waiting on it.
pub(super) fn awaiting(h: &mut Harness, contexts: &[&str]) -> oneshot::Receiver<Decision> {
    let answer = h.vcx.update(|_, cx| {
        ManualConfirmations::insert_test_pending(
            cx,
            "corp-vpn-id",
            "corp-vpn",
            Some(MESSAGE),
            contexts,
        )
    });
    h.vcx.run_until_parked();
    answer
}

pub(super) fn items(h: &mut Harness) -> Vec<StatusItem> {
    h.vcx.update(|_, cx| {
        h.main
            .read(cx)
            .test_status_bar()
            .map(|bar| bar.read(cx).items(cx))
            .unwrap_or_default()
    })
}

pub(super) fn drawn(h: &mut Harness, selector: String) -> bool {
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector.leak()).is_some()
}

#[gpui_kit::test]
async fn a_waiting_context_reads_as_awaiting_confirmation(cx: &mut TestAppContext) {
    let mut h = harness(
        cx,
        &[
            ("dev", ConnectionState::WaitingForTunnel),
            ("staging", ConnectionState::WaitingForTunnel),
        ],
    );
    awaiting(&mut h, &["staging"]);
    let health = |h: &mut Harness, name: &'static str| {
        h.vcx.update(|_, cx| ClusterRegistry::health(cx, name))
    };
    match health(&mut h, "staging") {
        ContextHealth::AwaitingConfirmation {
            tunnel, message, ..
        } => {
            assert_eq!(tunnel, "corp-vpn");
            assert_eq!(message.as_deref(), Some(MESSAGE));
        }
        other => panic!("expected awaiting confirmation, got {other:?}"),
    }
    // Waiting on some other tunnel: still just waiting.
    assert!(matches!(
        health(&mut h, "dev"),
        ContextHealth::WaitingForTunnel { .. }
    ));
}

#[gpui_kit::test]
async fn a_context_no_longer_waiting_never_reads_as_awaiting(cx: &mut TestAppContext) {
    // Listed on the entry, but its own connection already failed.
    let mut h = harness(
        cx,
        &[("staging", ConnectionState::Failed("refused".into()))],
    );
    awaiting(&mut h, &["staging"]);
    assert!(matches!(
        h.vcx.update(|_, cx| ClusterRegistry::health(cx, "staging")),
        ContextHealth::Failed { .. }
    ));
}

#[gpui_kit::test]
async fn the_awaiting_capsule_comes_first_filled_with_its_own_icon(cx: &mut TestAppContext) {
    let mut h = harness(
        cx,
        &[
            ("dev", ConnectionState::Failed("refused".into())),
            ("staging", ConnectionState::WaitingForTunnel),
        ],
    );
    awaiting(&mut h, &["staging"]);

    let items = items(&mut h);
    let names: Vec<_> = items
        .iter()
        .map(|item| item.context_name.as_str())
        .collect();
    assert_eq!(names, ["staging", "dev"], "attention ahead of a failure");
    let awaiting = &items[0];
    assert_eq!(awaiting.severity, Severity::Attention);
    assert_eq!(awaiting.text, "Awaiting confirmation");
    assert_eq!(awaiting.icon, IconName::BellRing);
    assert!(
        items[1..]
            .iter()
            .all(|item| item.icon != IconName::BellRing),
        "no other state shares the icon"
    );

    assert!(drawn(&mut h, attention_selector("staging")), "filled");
    assert!(
        !drawn(&mut h, attention_selector("dev")),
        "only the waiting one"
    );
}

#[gpui_kit::test]
async fn the_state_icons_tooltip_gives_the_state_wait_and_instruction(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[("staging", ConnectionState::WaitingForTunnel)]);
    awaiting(&mut h, &["staging"]);
    let item = items(&mut h).remove(0);
    let tooltip = state_tooltip_text(&item).expect("the awaiting state has a tooltip");
    assert!(
        tooltip.starts_with("Awaiting confirmation, waiting "),
        "{tooltip}"
    );
    assert!(tooltip.ends_with(MESSAGE), "{tooltip}");

    h.vcx.update(|window, cx| window.render_frame(cx));
    let at = h
        .vcx
        .debug_bounds(state_icon_selector("staging").leak())
        .expect("the state icon is drawn")
        .center();
    h.vcx.simulate_mouse_move(at, None, Modifiers::none());
    h.vcx
        .executor()
        .advance_clock(std::time::Duration::from_secs(1));
    h.vcx.run_until_parked();
    assert!(
        drawn(&mut h, crate::ui::icon_tooltip::selector(&tooltip)),
        "hovering the icon shows {tooltip:?}"
    );
}

#[gpui_kit::test]
async fn proceeding_returns_the_capsule_to_its_usual_look(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[("staging", ConnectionState::WaitingForTunnel)]);
    let _answer = awaiting(&mut h, &["staging"]);
    h.vcx
        .update(|_, cx| ManualConfirmations::resolve(cx, "corp-vpn-id", Decision::Proceed));
    h.vcx.run_until_parked();
    assert!(!drawn(&mut h, attention_selector("staging")));
    assert_ne!(items(&mut h)[0].severity, Severity::Attention);
}
