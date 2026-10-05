//! `pending-chord-indicator` 3.3-3.4 under a controlled clock: `ctrl-b` is a
//! binding on its own and the start of `ctrl-b h`, so pressing it alone waits
//! for the Shortcut timeout - 3 s by default, 6 s once changed - and not GPUI's
//! own second. A chord that changes first cancels the wait, a bar that goes
//! away releases its pause, and `cmd-k`, a prefix that's no binding on its
//! own, waits as long as it takes.
//!
//! The total wait pins `consts::GPUI_PENDING_INPUT_TIMEOUT`: if a GPUI release
//! changes its own timeout, the 3 s and 6 s cases here fail.

use super::StatusBarView;
use crate::command::CommandRegistry;
use crate::config::ui::ShortcutTimeout;
use crate::ui::panel::arrange::{ClosePanelGroup, DOCK_KEY_CONTEXT};
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window,
    actions, div,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

actions!(chord_timeout_test, [Short, Long]);

#[derive(Default)]
struct Counts {
    short: Cell<usize>,
    long: Cell<usize>,
    closed: Cell<usize>,
}

/// A view in the dock's key context, counting what its keys run, with the
/// status bar watching its window.
struct Host {
    focus: FocusHandle,
    counts: Rc<Counts>,
    bar: Option<Entity<StatusBarView>>,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (short, long, closed) = (
            self.counts.clone(),
            self.counts.clone(),
            self.counts.clone(),
        );
        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context(DOCK_KEY_CONTEXT)
            .on_action(move |_: &Short, _, _| short.short.set(short.short.get() + 1))
            .on_action(move |_: &Long, _, _| long.long.set(long.long.get() + 1))
            .on_action(move |_: &ClosePanelGroup, _, _| closed.closed.set(closed.closed.get() + 1))
            .children(self.bar.clone())
    }
}

struct Harness {
    host: Entity<Host>,
    counts: Rc<Counts>,
    vcx: VisualTestContext,
}

fn harness(cx: &mut TestAppContext, timeout: Option<u8>) -> Harness {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let mut registry = CommandRegistry::new();
        crate::ui::panel::arrange::register_commands(&mut registry);
        cx.bind_keys(crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            &gpui_kit::DummyKeyboardMapper,
        ));
        cx.bind_keys([
            KeyBinding::new("ctrl-b", Short, Some(DOCK_KEY_CONTEXT)),
            KeyBinding::new("ctrl-b h", Long, Some(DOCK_KEY_CONTEXT)),
        ]);
        cx.set_global(registry);
        if let Some(secs) = timeout {
            crate::ui::shortcut_timeout::set(ShortcutTimeout::from(i64::from(secs)), cx);
        }
    });
    let counts = Rc::new(Counts::default());
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let bar = cx.new(|cx| {
            let mut bar = StatusBarView::new(Vec::new(), cx);
            bar.watch_pending_input(window, cx);
            bar
        });
        let host = cx.new(|cx| Host {
            focus: cx.focus_handle(),
            counts: counts.clone(),
            bar: Some(bar),
        });
        built = Some(host.clone());
        Root::new(host, window, cx)
    });
    let host = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        let focus = host.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    vcx.run_until_parked();
    Harness { host, counts, vcx }
}

impl Harness {
    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    fn wait(&mut self, millis: u64) {
        self.vcx
            .executor()
            .advance_clock(Duration::from_millis(millis));
        self.vcx.run_until_parked();
    }

    fn pending(&mut self) -> bool {
        self.vcx
            .update(|window, _| window.pending_input().is_some())
    }
}

/// Spec: "Default timeout" - about 3 s, not GPUI's 1 s, and not before.
#[gpui_kit::test]
async fn an_ambiguous_chord_waits_the_default_three_seconds(cx: &mut TestAppContext) {
    let mut h = harness(cx, None);
    h.press("ctrl-b");
    h.wait(2_900);
    assert!(h.pending(), "still waiting at 2.9 s");
    assert_eq!(h.counts.short.get(), 0);
    h.wait(200);
    assert_eq!(h.counts.short.get(), 1, "ran by 3.1 s");
    assert!(!h.pending());
}

/// Spec: "Changed timeout applies at once".
#[gpui_kit::test]
async fn a_changed_timeout_applies_to_the_next_chord(cx: &mut TestAppContext) {
    let mut h = harness(cx, Some(6));
    h.press("ctrl-b");
    h.wait(5_900);
    assert_eq!(h.counts.short.get(), 0, "still waiting at 5.9 s");
    h.wait(200);
    assert_eq!(h.counts.short.get(), 1, "ran by 6.1 s");
}

/// A chord that moves on mid-wait cancels it: the longer binding runs, and
/// the shorter never does.
#[gpui_kit::test]
async fn a_chord_that_changes_mid_wait_cancels_it(cx: &mut TestAppContext) {
    let mut h = harness(cx, None);
    h.press("ctrl-b");
    h.wait(1_500);
    h.press("h");
    assert_eq!(h.counts.long.get(), 1);
    assert!(!h.pending());
    h.wait(10_000);
    assert_eq!(h.counts.short.get(), 0, "the cancelled wait runs nothing");
}

/// A bar that goes away mid-wait - its window closing - releases the pause,
/// so GPUI's own timer finishes the wait instead of it hanging.
#[gpui_kit::test]
async fn a_released_bar_releases_its_pause(cx: &mut TestAppContext) {
    let mut h = harness(cx, Some(10));
    h.press("ctrl-b");
    h.wait(500);
    let paused = h.vcx.update(|window, _| {
        window
            .pending_input()
            .and_then(|pending| pending.timeout())
            .is_some_and(|timeout| timeout.is_paused())
    });
    assert!(paused, "the bar is holding GPUI's timer");
    let host = h.host.clone();
    h.vcx
        .update(|_, cx| host.update(cx, |host, _| host.bar = None));
    h.vcx.run_until_parked();
    h.wait(1_100);
    assert_eq!(
        h.counts.short.get(),
        1,
        "GPUI's own second ran out, long before the 10 s preference"
    );
}

/// Spec: "Unambiguous prefix never times out".
#[gpui_kit::test]
async fn an_unambiguous_prefix_never_times_out(cx: &mut TestAppContext) {
    let mut h = harness(cx, None);
    h.press("cmd-k");
    h.wait(60_000);
    assert!(h.pending(), "still pending after a minute");
    h.press("w");
    assert_eq!(h.counts.closed.get(), 1, "Close Group ran");
}
