//! `manual-confirmation-tunnels` section 4 through a real workspace window: a
//! context waiting on a manual tunnel's confirmation - its health, its capsule's
//! fill, icon, tooltip and place in the row, and (4.2) its menu.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::context_health::{ContextHealth, Severity};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::tunnel::manual::{Decision, ManualConfirmations};
use crate::ui::status_bar::{StatusItem, attention_selector, icon_selector};
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
    let tooltip = item.tooltip();
    assert!(
        tooltip.starts_with("Awaiting confirmation for "),
        "{tooltip}"
    );
    assert!(tooltip.ends_with(&format!(": {MESSAGE}")), "{tooltip}");

    h.vcx.update(|window, cx| window.render_frame(cx));
    let at = h
        .vcx
        .debug_bounds(icon_selector("staging").leak())
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

fn click(h: &mut Harness, id: &'static str) {
    let center = h.vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .unwrap_or_else(|| panic!("{id} is drawn"))
            .bounds()
            .center()
    });
    h.vcx.simulate_click(center, Modifiers::none());
    h.vcx.run_until_parked();
}

fn keys(h: &mut Harness, keys: &str) {
    h.vcx.simulate_keystrokes(keys);
    h.vcx.run_until_parked();
}

fn dialog_open(h: &mut Harness) -> bool {
    use gpui_kit::component::WindowExt as _;
    h.vcx.update(|window, cx| window.has_active_dialog(cx))
}

fn two_waiting(cx: &mut TestAppContext) -> (Harness, oneshot::Receiver<Decision>) {
    let mut h = harness(
        cx,
        &[
            ("dev", ConnectionState::WaitingForTunnel),
            ("qa", ConnectionState::WaitingForTunnel),
        ],
    );
    let answer = awaiting(&mut h, &["dev", "qa"]);
    (h, answer)
}

fn still_waiting(h: &mut Harness) -> bool {
    h.vcx.update(|_, cx| {
        ManualConfirmations::entity(cx)
            .is_some_and(|entity| entity.read(cx).pending_for("corp-vpn-id").is_some())
    })
}

/// 4.2: Proceed leads a waiting capsule's menu - Down reaches it first - and answers the
/// tunnel for every context sharing it, from whichever capsule it was chosen.
#[gpui_kit::test]
async fn proceed_from_either_capsule_answers_the_shared_tunnel(cx: &mut TestAppContext) {
    let (mut h, mut answer) = two_waiting(cx);
    click(&mut h, "context-chip-menu-qa");
    keys(&mut h, "down");
    keys(&mut h, "enter");
    assert_eq!(answer.try_recv(), Ok(Decision::Proceed));
    assert!(!still_waiting(&mut h), "both capsules' tunnel is answered");
}

/// 4.2: Cancel is the next row down.
#[gpui_kit::test]
async fn cancel_is_the_row_after_proceed(cx: &mut TestAppContext) {
    let (mut h, mut answer) = two_waiting(cx);
    click(&mut h, "context-chip-menu-dev");
    keys(&mut h, "down down");
    keys(&mut h, "enter");
    assert_eq!(answer.try_recv(), Ok(Decision::Cancel));
    assert!(!still_waiting(&mut h));
}

/// 4.2: Disconnect, below them, closes only its own context; the tunnel keeps
/// waiting for the other.
#[gpui_kit::test]
async fn disconnect_on_a_waiting_capsule_closes_only_its_context(cx: &mut TestAppContext) {
    let (mut h, mut answer) = two_waiting(cx);
    click(&mut h, "context-chip-menu-dev");
    // Proceed, Cancel, then - past the separator - Disconnect.
    keys(&mut h, "down down down");
    keys(&mut h, "enter");
    assert!(dialog_open(&mut h), "Disconnect asks first");
    // Tab past Cancel to Disconnect, then Space.
    keys(&mut h, "tab tab");
    let space = gpui_kit::Keystroke::parse("space").expect("valid");
    h.vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    h.vcx
        .simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    h.vcx.run_until_parked();

    let names: Vec<_> = items(&mut h)
        .into_iter()
        .map(|item| item.context_name)
        .collect();
    assert_eq!(names, ["qa"], "only dev went");
    assert_eq!(answer.try_recv(), Err(oneshot::error::TryRecvError::Empty));
    assert!(still_waiting(&mut h), "qa still waits on the tunnel");
}

/// A capsule not waiting on anything offers only Disconnect: Enter picks it.
#[gpui_kit::test]
async fn a_capsule_not_waiting_offers_only_disconnect(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[("dev", ConnectionState::Connecting)]);
    click(&mut h, "context-chip-menu-dev");
    keys(&mut h, "enter");
    assert!(dialog_open(&mut h), "the first row is Disconnect");
}

/// 4.2: the Proceed and Cancel rows show the commands' keys, read from the live
/// keymap.
#[test]
fn the_menu_rows_show_the_commands_keys() {
    crate::ui::typography::recorder::with_recorded_text(|cx, recorded| {
        let (mut h, _answer) = two_waiting(cx);
        click(&mut h, "context-chip-menu-dev");
        h.vcx.update(|window, cx| window.render_frame(cx));
        for key in ["secondary-alt-p", "secondary-alt-c"] {
            let label = gpui_kit::component::kbd::Kbd::format(
                &gpui_kit::Keystroke::parse(key).expect("valid"),
            );
            assert!(
                recorded.families_of(&label).is_some(),
                "{key} ({label}) is drawn"
            );
        }
        keys(&mut h, "escape");
    });
}
