//! The shell panel in a window, over a stand-in session: what the container
//! writes shows in the transcript, a line typed and entered reaches its stdin
//! (echoed, as the shell has no TTY to echo it), and a session that ends keeps
//! its transcript and says so.

use super::panel::{ExecPanel, ExecTarget, SessionState};
use super::render::{ENDED, TRANSCRIPT};
use crate::k8s::resource::exec::bridge::ExecEvent;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};
use tokio::sync::mpsc;

struct Harness {
    panel: Entity<ExecPanel>,
    vcx: VisualTestContext,
    stdin: mpsc::Receiver<Vec<u8>>,
    events: mpsc::Sender<ExecEvent>,
}

fn open(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let (stdin_tx, stdin) = mpsc::channel(8);
    let (events, events_rx) = mpsc::channel(8);
    let target = ExecTarget {
        namespace: "shop".into(),
        pod: "web-1".into(),
        container: "app".into(),
    };
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Exec(target.clone()), "demo".into());
        let panel =
            cx.new(|cx| ExecPanel::with_session(target, scope, stdin_tx, events_rx, window, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        let input = panel.read(cx).input();
        input.read(cx).focus_handle(cx).focus(window, cx);
    });
    Harness {
        panel,
        vcx,
        stdin,
        events,
    }
}

impl Harness {
    fn send(&mut self, event: ExecEvent) {
        let handle = self.vcx.update(|_, cx| crate::runtime::handle(cx));
        let events = self.events.clone();
        handle.block_on(async move { events.send(event).await.unwrap() });
    }

    fn wait_for(&mut self, what: &str, done: impl Fn(&ExecPanel) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if self.vcx.update(|_, cx| done(self.panel.read(cx))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }

    fn drawn(&mut self, selector: &'static str) -> bool {
        self.vcx.update(|window, cx| window.render_frame(cx));
        self.vcx.debug_bounds(selector).is_some()
    }
}

/// 3.2: the container's output shows, and a typed line goes to the shell.
#[gpui_kit::test]
async fn output_shows_and_a_typed_line_reaches_the_shell(cx: &mut TestAppContext) {
    let mut h = open(cx);

    h.send(ExecEvent::Output("total 0\n".into()));
    h.wait_for("showed the output", |panel| {
        panel.transcript.contains("total 0")
    });
    assert!(h.drawn(TRANSCRIPT));

    h.vcx.simulate_keystrokes("l s enter");
    h.vcx.run_until_parked();
    let sent = h.stdin.try_recv().expect("the line went to stdin");
    assert_eq!(sent, b"ls\n");
    let transcript = h.vcx.update(|_, cx| h.panel.read(cx).transcript.clone());
    assert!(transcript.ends_with("$ ls\n"), "echoed: {transcript:?}");
}

/// 3.4: when the container's side ends, the panel says so and keeps its
/// transcript; typing goes nowhere.
#[gpui_kit::test]
async fn an_ended_session_keeps_its_transcript(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.send(ExecEvent::Output("about to exit\n".into()));
    h.send(ExecEvent::Ended(Some(
        "command terminated with non-zero exit code".into(),
    )));

    h.wait_for("ended the session", |panel| {
        matches!(panel.state, SessionState::Ended(Some(_)))
    });
    let transcript = h.vcx.update(|_, cx| h.panel.read(cx).transcript.clone());
    assert_eq!(transcript, "about to exit\n", "the transcript stays");
    assert!(h.drawn(ENDED), "the panel says it ended");
    assert!(h.drawn(TRANSCRIPT), "and still shows the transcript");

    h.vcx.simulate_keystrokes("l s enter");
    h.vcx.run_until_parked();
    assert!(
        h.stdin.try_recv().is_err(),
        "nothing goes to an ended shell"
    );
}
