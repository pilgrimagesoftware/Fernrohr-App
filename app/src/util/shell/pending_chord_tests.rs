//! `pending-chord-indicator` 2.1-2.2 through a real window and the app's
//! keymap: `cmd-k` shows the indicator and its five completions, a completing
//! key runs its command and clears it, a key that completes nothing clears it
//! and runs nothing, a focus change clears it, a single-step shortcut never
//! shows it - and a chord reaches its action the same with or without a bar.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::panel::arrange::{ClosePanelGroup, DOCK_KEY_CONTEXT};
use crate::ui::status_bar::StatusBarView;
use crate::ui::status_bar::chord_selectors::{
    INDICATOR_SELECTOR, MORE_SELECTOR, completion_selector,
};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::component::dock::{DockPlacement, PaneRef};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window,
    WindowHandle, div,
};
use std::cell::Cell;
use std::rc::Rc;

/// Every arrange command, all under `cmd-k`, in palette order (#138).
const ARRANGE_CHORDS: [&str; 13] = [
    "panel.split_left",
    "panel.split_right",
    "panel.split_up",
    "panel.split_down",
    "panel.move_left",
    "panel.move_right",
    "panel.move_up",
    "panel.move_down",
    "panel.merge_left",
    "panel.merge_right",
    "panel.merge_up",
    "panel.merge_down",
    "panel.close_group",
];

struct Harness {
    window: WindowHandle<Root>,
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
}

/// A window on `demo` showing Pods, focused - one panel group.
fn harness(cx: &mut TestAppContext) -> Harness {
    harness_with_keymap(cx, "")
}

/// [`harness`], with `keymap` as `keymap.toml`'s text.
fn harness_with_keymap(cx: &mut TestAppContext, keymap_text: &str) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    if !keymap_text.is_empty() {
        std::fs::write(&keymap, keymap_text).unwrap();
    }
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| main.update(cx, |main, cx| main.test_focus(window, cx)));
    vcx.simulate_keystrokes("cmd-1");
    vcx.run_until_parked();
    Harness { window, main, vcx }
}

impl Harness {
    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    fn chord(&mut self) -> Option<(Vec<String>, Vec<&'static str>)> {
        let main = self.main.clone();
        self.vcx.update(|_, cx| {
            main.read(cx)
                .test_status_bar()
                .expect("a workspace has a status bar")
                .read(cx)
                .pending_chord()
        })
    }

    fn drawn(&mut self, selector: String) -> bool {
        let _ = self
            .vcx
            .update_window(self.window.into(), |_, window, cx| window.render_frame(cx));
        self.vcx.debug_bounds(selector.leak()).is_some()
    }

    fn groups(&mut self) -> usize {
        let main = self.main.clone();
        self.vcx.update(|_, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.read(cx).mode else {
                return 0;
            };
            let area = dock_area.read(cx);
            let tree = area.layout(DockPlacement::Center).expect("a centre");
            crate::ui::panel::arrange::group_rects(tree.root())
                .into_iter()
                .filter(|(node, _)| {
                    matches!(
                        tree.find_node(*node).map(|n| n.kind()),
                        Some(PaneRef::Tabs { .. })
                    )
                })
                .count()
        })
    }
}

/// Spec: "First key of a chord" - with split, move, merge and close group all
/// under `cmd-k`, the popover lists the first eight and "… and 5 more".
#[gpui_kit::test]
async fn cmd_k_shows_the_pending_keys_and_every_arrange_completion(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    assert_eq!(h.chord(), None);
    h.press("cmd-k");
    assert_eq!(
        h.chord(),
        // `cmd` is the platform key: it reads back as `super-k` on Linux.
        Some((
            vec![gpui_kit::Keystroke::parse("cmd-k").unwrap().unparse()],
            ARRANGE_CHORDS.to_vec()
        ))
    );
    assert!(h.drawn(INDICATOR_SELECTOR.to_string()));
    for id in &ARRANGE_CHORDS[..8] {
        assert!(h.drawn(completion_selector(id)), "{id} is listed");
    }
    assert!(h.drawn(MORE_SELECTOR.to_string()), "… and 5 more");
}

/// More completions than the popover's eight rows end in "… and N more" -
/// here the eight move and merge commands rebound onto `cmd-k 1`-`8`, which
/// the live indicator lists as its new keys.
#[gpui_kit::test]
async fn more_than_eight_completions_end_in_a_count(cx: &mut TestAppContext) {
    let moves = [
        "panel.move_left",
        "panel.move_right",
        "panel.move_up",
        "panel.move_down",
        "panel.merge_left",
        "panel.merge_right",
        "panel.merge_up",
        "panel.merge_down",
    ];
    let keymap: String = std::iter::once("[bindings]\n".to_string())
        .chain(
            moves
                .iter()
                .enumerate()
                .map(|(n, id)| format!("\"{id}\" = \"cmd-k {}\"\n", n + 1)),
        )
        .collect();
    let mut h = harness_with_keymap(cx, &keymap);
    h.press("cmd-k");
    let (_, ids) = h.chord().expect("pending");
    assert_eq!(ids.len(), 13);
    assert!(h.drawn(MORE_SELECTOR.to_string()), "… and 5 more");
    assert!(h.drawn(completion_selector(ids[7])), "the eighth row shows");
    assert!(!h.drawn(completion_selector(ids[8])), "the ninth doesn't");
}

/// Spec: "Chord completes".
#[gpui_kit::test]
async fn a_completing_key_runs_its_command_and_clears(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    assert_eq!(h.groups(), 1);
    h.press("cmd-k left");
    assert_eq!(h.groups(), 2, "the split ran");
    assert_eq!(h.chord(), None);
    assert!(!h.drawn(INDICATOR_SELECTOR.to_string()));
}

/// Spec: "Chord abandoned".
#[gpui_kit::test]
async fn a_key_that_completes_nothing_clears_and_runs_nothing(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.press("cmd-k");
    h.press("x");
    assert_eq!(h.chord(), None);
    assert_eq!(h.groups(), 1, "no split, move, merge or close");
}

/// A focus change drops the pending keys, and the indicator with them.
#[gpui_kit::test]
async fn a_focus_change_clears(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.press("cmd-k");
    assert!(h.chord().is_some());
    let main = h.main.clone();
    h.vcx.update(|window, cx| {
        let resource = main
            .read(cx)
            .test_resource_panel()
            .expect("a workspace window")
            .read(cx)
            .focus_handle();
        resource.focus(window, cx);
    });
    h.vcx.run_until_parked();
    assert_eq!(h.chord(), None);
}

/// Spec: "Single-step shortcuts show nothing".
#[gpui_kit::test]
async fn a_single_step_shortcut_never_shows_it(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let shown = Rc::new(Cell::new(false));
    let bar = {
        let main = h.main.clone();
        h.vcx
            .update(|_, cx| main.read(cx).test_status_bar().expect("a status bar"))
    };
    let _watch = h.vcx.update(|_, cx| {
        let shown = shown.clone();
        cx.observe(&bar, move |bar, cx| {
            if bar.read(cx).pending_chord().is_some() {
                shown.set(true);
            }
        })
    });
    // Show Pods: a complete binding that starts no other.
    h.press("cmd-1");
    assert!(!shown.get(), "the indicator never appeared");
    assert_eq!(h.chord(), None);
}

/// A view in the dock's key context, counting Close Group.
struct DockStandIn {
    focus: FocusHandle,
    closed: Rc<Cell<usize>>,
    bar: Option<Entity<StatusBarView>>,
}

impl Render for DockStandIn {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let closed = self.closed.clone();
        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context(DOCK_KEY_CONTEXT)
            .on_action(move |_: &ClosePanelGroup, _, _| closed.set(closed.get() + 1))
            .children(self.bar.clone())
            .child(div().child(format!("{}", cx.entity_id())))
    }
}

/// `cmd-k w` in a dock stand-in, with or without a watching status bar: how
/// many times Close Group ran, and whether the bar saw the chord pending.
fn close_group_by_chord(cx: &mut TestAppContext, with_bar: bool) -> (usize, bool) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let mut registry = CommandRegistry::new();
        crate::ui::panel::arrange::register_commands(&mut registry);
        cx.bind_keys(crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            &gpui_kit::DummyKeyboardMapper,
        ));
        cx.set_global(registry);
        cx.set_global(crate::keymap::LiveKeymap::new(
            temp_workspace_path(),
            Default::default(),
        ));
    });
    let closed = Rc::new(Cell::new(0));
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let bar = with_bar.then(|| {
            cx.new(|cx| {
                let mut bar = StatusBarView::new(Vec::new(), cx);
                bar.watch_pending_input(window, cx);
                bar
            })
        });
        let view = cx.new(|cx| DockStandIn {
            focus: cx.focus_handle(),
            closed: closed.clone(),
            bar,
        });
        built = Some(view.clone());
        Root::new(view, window, cx)
    });
    let view = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    vcx.simulate_keystrokes("cmd-k");
    vcx.run_until_parked();
    let seen = vcx.update(|_, cx| {
        view.read(cx)
            .bar
            .as_ref()
            .is_some_and(|bar| bar.read(cx).pending_chord().is_some())
    });
    vcx.simulate_keystrokes("w");
    vcx.run_until_parked();
    (closed.get(), seen)
}

/// 2.2: the indicator takes no focus and no key - the completing key reaches
/// the same action with the bar watching as without it.
#[gpui_kit::test]
async fn a_chord_reaches_its_action_with_or_without_the_bar(cx: &mut TestAppContext) {
    assert_eq!(close_group_by_chord(cx, false), (1, false));
}

#[gpui_kit::test]
async fn a_chord_reaches_its_action_with_the_bar_watching(cx: &mut TestAppContext) {
    assert_eq!(
        close_group_by_chord(cx, true),
        (1, true),
        "the bar saw the chord, and Close Group still ran once"
    );
}
