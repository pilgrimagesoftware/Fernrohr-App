//! Proceed and Cancel from the keyboard, through a real workspace window and the
//! app's keymap: one waiting tunnel is answered at once, several ask which.

use super::picker::row_selector;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::tunnel::manual::{Decision, ManualConfirmations};
use crate::util::shell::{MainWindow, TOGGLE_PALETTE_DEFAULT_BINDING, init};
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Keystroke, TestAppContext, VisualTestContext};
use tokio::sync::oneshot::{Receiver, error::TryRecvError};

/// A workspace window over `dev` and `qa`, both waiting on a tunnel.
fn window(cx: &mut TestAppContext) -> VisualTestContext {
    cx.executor().allow_parking();
    let workspace = crate::util::test_paths::temp_path("manual-tunnel-workspace");
    let keymap = crate::util::test_paths::temp_path("manual-tunnel-keymap");
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        for context in ["dev", "qa"] {
            ClusterRegistry::insert_test_session(cx, context, ConnectionState::WaitingForTunnel);
        }
    });
    let window = cx.add_window(|window, cx| {
        let main =
            cx.new(|cx| MainWindow::test_workspace(vec!["dev".into(), "qa".into()], window, cx));
        main.update(cx, |main, cx| main.test_focus(window, cx));
        Root::new(main, window, cx)
    });
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx
}

fn waiting(vcx: &mut VisualTestContext, tunnel: &str, context: &str) -> Receiver<Decision> {
    let answer = vcx.update(|_, cx| {
        ManualConfirmations::insert_test_pending(
            cx,
            &format!("{tunnel}-id"),
            tunnel,
            None,
            &[context],
        )
    });
    vcx.run_until_parked();
    answer
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    let keys = Keystroke::parse(keys).expect("valid").unparse();
    vcx.simulate_keystrokes(&keys);
    vcx.run_until_parked();
}

fn dialog_open(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.has_active_dialog(cx))
}

fn pending(vcx: &mut VisualTestContext) -> Vec<String> {
    vcx.update(|_, cx| {
        ManualConfirmations::entity(cx)
            .map(|entity| {
                entity
                    .read(cx)
                    .pending()
                    .iter()
                    .map(|entry| entry.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
async fn the_proceed_key_answers_the_one_waiting_tunnel(cx: &mut TestAppContext) {
    let mut vcx = window(cx);
    let mut answer = waiting(&mut vcx, "corp-vpn", "dev");
    press(&mut vcx, super::PROCEED_KEY);
    assert_eq!(answer.try_recv(), Ok(Decision::Proceed));
    assert!(pending(&mut vcx).is_empty());
    assert!(!dialog_open(&mut vcx), "nothing to ask with one waiting");
}

#[gpui_kit::test]
async fn proceed_runs_from_the_palette_without_the_mouse(cx: &mut TestAppContext) {
    let mut vcx = window(cx);
    let mut answer = waiting(&mut vcx, "corp-vpn", "dev");
    press(&mut vcx, TOGGLE_PALETTE_DEFAULT_BINDING);
    vcx.simulate_input("Proceed with Manual Tunnel");
    vcx.run_until_parked();
    press(&mut vcx, "enter");
    assert_eq!(answer.try_recv(), Ok(Decision::Proceed));
}

#[gpui_kit::test]
async fn with_several_waiting_the_command_asks_which(cx: &mut TestAppContext) {
    let mut vcx = window(cx);
    let mut corp = waiting(&mut vcx, "corp-vpn", "dev");
    let mut lab = waiting(&mut vcx, "lab-vpn", "qa");

    press(&mut vcx, super::CANCEL_KEY);
    assert!(dialog_open(&mut vcx), "a picker asks which tunnel");
    vcx.update(|window, cx| window.render_frame(cx));
    for tunnel in ["corp-vpn-id", "lab-vpn-id"] {
        assert!(
            vcx.debug_bounds(row_selector(tunnel).leak()).is_some(),
            "{tunnel} is listed"
        );
    }
    press(&mut vcx, "down");
    press(&mut vcx, "enter");

    assert!(!dialog_open(&mut vcx));
    assert_eq!(lab.try_recv(), Ok(Decision::Cancel), "only the chosen one");
    assert_eq!(corp.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(pending(&mut vcx), ["corp-vpn"]);
}

#[gpui_kit::test]
async fn escape_answers_none(cx: &mut TestAppContext) {
    let mut vcx = window(cx);
    let mut corp = waiting(&mut vcx, "corp-vpn", "dev");
    let mut lab = waiting(&mut vcx, "lab-vpn", "qa");
    press(&mut vcx, super::PROCEED_KEY);
    assert!(dialog_open(&mut vcx));
    press(&mut vcx, "escape");
    assert!(!dialog_open(&mut vcx));
    assert_eq!(corp.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(lab.try_recv(), Err(TryRecvError::Empty));
}

#[gpui_kit::test]
async fn with_nothing_waiting_the_commands_do_nothing(cx: &mut TestAppContext) {
    let mut vcx = window(cx);
    press(&mut vcx, super::PROCEED_KEY);
    press(&mut vcx, super::CANCEL_KEY);
    assert!(!dialog_open(&mut vcx));
}
