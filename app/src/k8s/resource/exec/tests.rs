//! The shell panel in a window with the app's whole keymap, over a stand-in
//! session: keys - the app's own letters and Ctrl-C included - reach the
//! shell as bytes, the palette's key still opens the palette, a text-size
//! change resizes the shell, and an ended session keeps its screen, says how
//! it ended and takes no more input.

use super::panel::{ExecPanel, ExecTarget, SessionState};
use super::render::{ENDED, HINTS, INPUT_NOTICE, TERMINAL, ended_notice};
use super::transport::{ExecEnd, ExecTransport};
use crate::command::CommandRegistry;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use gpui_terminal::{ExitReport, GridSize, TerminalBuilder, TerminalSink};
use std::cell::Cell;
use std::rc::Rc;
use tokio::sync::{mpsc, watch};

struct Harness {
    panel: Entity<ExecPanel>,
    vcx: VisualTestContext,
    /// What the terminal sent to the shell's stdin.
    input: mpsc::Receiver<Vec<u8>>,
    /// The last size the terminal gave the shell.
    resize: watch::Receiver<Option<GridSize>>,
    /// The shell's output, and its exit, as the exec would report them.
    sink: TerminalSink,
    ended: mpsc::Sender<ExecEnd>,
    palette_opened: Rc<Cell<usize>>,
}

/// A shell panel in a window with every app command bound, focused.
fn open(cx: &mut TestAppContext) -> Harness {
    open_with_queue(cx, 64)
}

/// [`open`], with room for `queue` chunks of input before the shell reads.
fn open_with_queue(cx: &mut TestAppContext, queue: usize) -> Harness {
    cx.executor().allow_parking();
    let palette_opened = Rc::new(Cell::new(0));
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::ui::theme::init(crate::config::ui::Theme::Dark, cx);
        let mut registry = CommandRegistry::new();
        crate::util::shell::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        cx.set_global(registry);
        let opened = palette_opened.clone();
        cx.on_action(move |_: &crate::util::shell::ToggleCommandPalette, _| {
            opened.set(opened.get() + 1)
        });
    });
    let (input_tx, input) = mpsc::channel(queue);
    let (resize_tx, resize) = watch::channel(None);
    let (ended, ended_rx) = mpsc::channel(1);
    let runtime = cx.update(|cx| crate::runtime::handle(cx));
    let mut sink = None;
    let terminal = TerminalBuilder::new()
        .connect(|handed| {
            sink = Some(handed);
            Ok::<_, std::convert::Infallible>(ExecTransport::with_task(
                input_tx,
                resize_tx,
                runtime.spawn(std::future::pending()),
            ))
        })
        .unwrap_or_else(|never| match never {});
    let target = ExecTarget {
        namespace: "shop".into(),
        pod: "web-1".into(),
        container: "app".into(),
    };
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Exec(target.clone()), "demo".into());
        let panel = cx.new(|cx| ExecPanel::with_session(target, scope, terminal, ended_rx, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        let focus = panel.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    Harness {
        panel,
        vcx,
        input,
        resize,
        sink: sink.expect("connect hands over a sink"),
        ended,
        palette_opened,
    }
}

impl Harness {
    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    /// Everything sent to stdin so far, in order.
    fn sent(&mut self) -> Vec<u8> {
        let mut sent = Vec::new();
        while let Ok(bytes) = self.input.try_recv() {
            sent.extend(bytes);
        }
        sent
    }

    fn output(&mut self, bytes: &[u8]) {
        self.sink.output(bytes);
        self.vcx.run_until_parked();
    }

    /// The session ends: the terminal hears the exit, the panel how.
    fn end(&mut self, end: ExecEnd) {
        self.sink.exited(ExitReport::new(end.code));
        self.ended.try_send(end).expect("the panel is listening");
        self.vcx.run_until_parked();
    }

    fn top_row(&mut self) -> String {
        self.vcx.update(|_, cx| {
            let terminal = self.panel.read(cx).terminal.clone().expect("a terminal");
            terminal
                .read(cx)
                .terminal()
                .with_grid(|grid| grid.row_text(0))
        })
    }

    fn drawn(&mut self, selector: &'static str) -> bool {
        self.vcx.update(|window, cx| window.render_frame(cx));
        self.vcx.debug_bounds(selector).is_some()
    }
}

/// 3.1: Ctrl-C, Tab and the app's own letter keys (`d` describe, `y` YAML,
/// `s` shell, `l` logs) go to the shell as bytes - none runs an app command.
#[gpui_kit::test]
async fn keys_reach_the_shell_rather_than_the_app(cx: &mut TestAppContext) {
    let mut h = open(cx);
    assert!(h.drawn(TERMINAL));
    assert!(h.drawn(HINTS), "the reserved keys are listed");

    h.press("ctrl-c tab d y s l enter");

    assert_eq!(h.sent(), b"\x03\tdysl\r");
    assert_eq!(h.palette_opened.get(), 0);
}

/// 3.1: the app keeps its `cmd-` keys - the palette opens over a shell, and
/// the key isn't sent to it.
#[gpui_kit::test]
async fn the_palette_key_still_opens_the_palette(cx: &mut TestAppContext) {
    let mut h = open(cx);

    h.press("cmd-shift-p");

    assert_eq!(h.palette_opened.get(), 1, "the palette's command ran");
    assert!(h.sent().is_empty(), "nothing went to the shell");
}

/// 3.2: a larger text size re-measures the terminal and sends the shell its
/// new, smaller grid.
#[gpui_kit::test]
async fn a_text_size_change_resizes_the_shell(cx: &mut TestAppContext) {
    let mut h = open(cx);
    let before = (*h.resize.borrow_and_update()).expect("the first layout sized the shell");

    h.vcx.update(|window, cx| {
        let theme = cx.global_mut::<gpui_kit::component::Theme>();
        theme.mono_font_size *= 2.;
        window.refresh();
    });
    h.vcx.run_until_parked();
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.run_until_parked();

    let after = (*h.resize.borrow_and_update()).expect("still sized");
    assert!(
        after.columns < before.columns && after.rows < before.rows,
        "a bigger font fits fewer cells: {before:?} -> {after:?}"
    );
}

/// 3.3: the shell exits - its screen stays, the panel says how it ended,
/// keys go nowhere, and closing it costs nothing.
#[gpui_kit::test]
async fn an_ended_session_keeps_its_screen_and_takes_no_input(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.output(b"$ exit");
    assert!(
        h.vcx
            .update(|_, cx| h.panel.read(cx).close_warning().is_some()),
        "while it runs, closing it ends the shell"
    );

    h.end(ExecEnd {
        code: Some(0),
        reason: None,
    });

    assert!(matches!(
        h.vcx.update(|_, cx| h.panel.read(cx).state.clone()),
        SessionState::Ended(ExecEnd { code: Some(0), .. })
    ));
    assert_eq!(h.top_row(), "$ exit", "the screen stays");
    assert!(h.drawn(ENDED), "the panel says it ended");
    assert!(!h.drawn(HINTS), "no keys to list for a finished shell");
    h.press("l s enter");
    assert!(h.sent().is_empty(), "nothing goes to an ended shell");
    assert!(
        h.vcx
            .update(|_, cx| h.panel.read(cx).close_warning().is_none()),
        "nothing left to lose"
    );
}

/// 3.3: a dropped connection ends the session with its reason, no status.
#[gpui_kit::test]
async fn a_dropped_connection_says_so(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.end(ExecEnd {
        code: None,
        reason: Some("The connection to the shell was lost.".into()),
    });
    assert!(h.drawn(ENDED));
    assert!(matches!(
        h.vcx.update(|_, cx| h.panel.read(cx).state.clone()),
        SessionState::Ended(ExecEnd {
            code: None,
            reason: Some(_)
        })
    ));
}

#[test]
fn the_ended_notice_gives_the_status_or_the_reason() {
    assert_eq!(
        ended_notice(Some(0), None),
        "Session ended (exit status 0)."
    );
    assert_eq!(
        ended_notice(Some(130), None),
        "Session ended (exit status 130)."
    );
    assert_eq!(
        ended_notice(None, Some("container not found")),
        "Session ended: container not found"
    );
    assert_eq!(ended_notice(None, None), "Session ended.");
}

/// Escape alone goes to the shell - vi's way back to normal mode - and no
/// app handler of it fires: the panel carries `Input`, where gpui-kit binds
/// Escape to its own input action, which the Resource panel's filter and
/// text fields handle.
#[gpui_kit::test]
async fn escape_reaches_the_shell_and_no_app_handler(cx: &mut TestAppContext) {
    let mut h = open(cx);
    let escaped = Rc::new(Cell::new(0));
    h.vcx.update(|_, cx| {
        let escaped = escaped.clone();
        cx.on_action(move |_: &gpui_kit::component::input::Escape, _| {
            escaped.set(escaped.get() + 1)
        });
    });

    h.press("i escape");

    assert_eq!(h.sent(), b"i\x1b");
    assert_eq!(escaped.get(), 0, "no app Escape handler ran");
}

/// Input the shell isn't reading - its queue full - isn't lost silently:
/// the panel says so under the terminal.
#[gpui_kit::test]
async fn input_refused_by_a_full_queue_is_shown(cx: &mut TestAppContext) {
    let mut h = open_with_queue(cx, 1);

    h.press("a b");

    assert_eq!(h.sent(), b"a", "the queue held one");
    let notice = h.vcx.update(|_, cx| h.panel.read(cx).input_notice.clone());
    assert!(
        notice.is_some_and(|notice| notice.contains("isn't reading")),
        "the refusal is shown"
    );
    assert!(h.drawn(INPUT_NOTICE));
}
