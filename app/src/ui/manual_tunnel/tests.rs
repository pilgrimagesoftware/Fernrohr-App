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

/// A notification backend that records what it was asked to show.
#[derive(Default)]
struct Recording(parking_lot::Mutex<Vec<crate::notify::Note>>);

impl crate::notify::Backend for Recording {
    fn post(&self, note: &crate::notify::Note) -> Result<crate::notify::Posted, String> {
        self.0.lock().push(note.clone());
        Ok(crate::notify::Posted::Shown)
    }
}

fn posted(
    vcx: &mut VisualTestContext,
    recording: &Recording,
    count: usize,
) -> Vec<crate::notify::Note> {
    // Posting runs on a blocking task; give it a moment to land.
    for _ in 0..200 {
        vcx.run_until_parked();
        if recording.0.lock().len() >= count {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    recording.0.lock().clone()
}

/// 4.5: a new prompt posts one notification naming the tunnel, its instruction and
/// who waits; a context joining it posts nothing; another tunnel's prompt posts its
/// own.
#[gpui_kit::test]
async fn each_new_prompt_posts_one_notification(cx: &mut TestAppContext) {
    let mut vcx = window(cx);
    let recording = std::sync::Arc::new(Recording::default());
    vcx.update(|_, cx| crate::notify::set_backend(recording.clone(), cx));

    vcx.update(|_, cx| {
        ManualConfirmations::note_waiting(cx, "corp-vpn-id", "dev");
    });
    let _corp = vcx.update(|_, cx| {
        ManualConfirmations::publish_test_pending(
            cx,
            "corp-vpn-id",
            "corp-vpn",
            Some("Connect the corporate VPN"),
        )
    });
    let notes = posted(&mut vcx, &recording, 1);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "corp-vpn is waiting for you");
    assert_eq!(notes[0].body, "Connect the corporate VPN\nWaiting: dev");

    // Another context joins the same prompt: nothing more is posted.
    vcx.update(|_, cx| ManualConfirmations::note_waiting(cx, "corp-vpn-id", "qa"));
    vcx.run_until_parked();
    assert_eq!(posted(&mut vcx, &recording, 1).len(), 1);
    assert_eq!(pending(&mut vcx), ["corp-vpn"]);

    // A second tunnel gets its own.
    let _lab = vcx.update(|_, cx| {
        ManualConfirmations::publish_test_pending(cx, "lab-vpn-id", "lab-vpn", None)
    });
    let notes = posted(&mut vcx, &recording, 2);
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[1].title, "lab-vpn is waiting for you");
    assert_eq!(
        notes[1].body,
        "Bring its network path up, then choose Proceed in Fernrohr."
    );
}
