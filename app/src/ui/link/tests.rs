//! The "Go to…" picker through real keystrokes: `resource-links` 4.1-4.2.
//!
//! Driven through a stand-in detail view with a fixed set of references,
//! because the only followable kinds before the object viewer lands are Pod and
//! Namespace, and a real pod names just one of those. The pod detail panel's own
//! `g` path is covered in `pod_detail`'s tests.

use super::{
    FollowReference, GO_TO_COMMAND_ID, GO_TO_DEFAULT_BINDING, GoToEntry, GoToReference,
    LINKS_KEY_CONTEXT, followable, open_go_to, register_commands,
};
use crate::command::CommandRegistry;
use crate::k8s::object_ref::ObjectRef;
use crate::keymap::{self, KeymapConfig};
use gpui_kit::component::{IndexPath, Root};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    KeyContext, ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window,
    WindowHandle, div,
};
use std::cell::RefCell;
use std::rc::Rc;

fn namespace(name: &str) -> ObjectRef {
    ObjectRef::cluster_scoped("", "Namespace", name)
}

/// A detail view reduced to what the picker needs from one: a focus handle, a
/// key context carrying `LINKS_KEY_CONTEXT`, and its references.
struct Source {
    focus_handle: FocusHandle,
    entries: Vec<GoToEntry>,
}

impl Focusable for Source {
    fn focus_handle(&self, _cx: &gpui_kit::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Source {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut context = KeyContext::default();
        context.add("SourcePanel");
        context.add(LINKS_KEY_CONTEXT);
        div()
            .size_full()
            .key_context(context)
            .track_focus(&self.focus_handle)
            .on_action(
                cx.listener(|this: &mut Self, _: &GoToReference, window, cx| {
                    open_go_to(
                        this.entries.clone(),
                        "kind-dev".into(),
                        this.focus_handle.clone(),
                        window,
                        cx,
                    );
                }),
            )
            .child("source")
    }
}

struct Harness {
    window: WindowHandle<Root>,
    source: Entity<Source>,
    followed: Rc<RefCell<Vec<FollowReference>>>,
}

fn harness(cx: &mut TestAppContext) -> Harness {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let followed = Rc::new(RefCell::new(Vec::new()));
    let sink = followed.clone();
    cx.update(|cx| {
        cx.on_action(move |action: &FollowReference, _cx| sink.borrow_mut().push(action.clone()));
    });
    let mut source = None;
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|cx| Source {
            focus_handle: cx.focus_handle(),
            entries: vec![
                GoToEntry::new(namespace("staging"), "Namespace"),
                GoToEntry::new(namespace("prod"), "Owner namespace"),
                GoToEntry::new(ObjectRef::core("Pod", "prod", "web-1"), "Sibling"),
            ],
        });
        source = Some(view.clone());
        Root::new(view, window, cx)
    });
    let source = source.expect("the window built its view");
    window
        .update(cx, |_, window, cx| {
            source.read(cx).focus_handle.clone().focus(window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    Harness {
        window,
        source,
        followed,
    }
}

fn source_is_focused(cx: &mut VisualTestContext, harness: &Harness) -> bool {
    let source = harness.source.clone();
    harness
        .window
        .update(cx, |_, window, cx| {
            source.read(cx).focus_handle.is_focused(window)
        })
        .unwrap()
}

/// 4.1: the command is registered under its id, gated to the shared links
/// context, bound to `g`, and kept out of the menu bar.
#[test]
fn the_go_to_command_is_registered_and_gated() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let command = registry.get(GO_TO_COMMAND_ID).expect("registered");
    assert_eq!(command.context, Some(LINKS_KEY_CONTEXT));
    assert_eq!(command.default_binding, GO_TO_DEFAULT_BINDING);
    assert_eq!(command.default_binding, "g");
    assert!(command.menu.is_none());
    assert!(
        registry.available(&[]).is_empty(),
        "inert outside a detail view"
    );
    assert_eq!(registry.available(&[LINKS_KEY_CONTEXT]).len(), 1);
}

/// Only followable references reach the picker, each object once.
#[test]
fn the_picker_lists_followable_references_once_each() {
    let entries = followable(
        [
            GoToEntry::new(namespace("staging"), "Namespace"),
            GoToEntry::new(
                ObjectRef::namespaced("apps", "ReplicaSet", "staging", "web"),
                "Controlled By",
            ),
            GoToEntry::new(namespace("staging"), "Again"),
        ],
        None,
    );
    assert_eq!(
        entries,
        vec![GoToEntry::new(namespace("staging"), "Namespace")]
    );
}

/// 4.2: `g`, type a filter, Enter follows the one match.
#[gpui_kit::test]
async fn typing_a_filter_then_enter_follows_the_match(cx: &mut TestAppContext) {
    let harness = harness(cx);
    let mut vcx = VisualTestContext::from_window(harness.window.into(), cx);

    vcx.simulate_keystrokes("g");
    vcx.run_until_parked();
    vcx.simulate_input("web");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    assert_eq!(
        *harness.followed.borrow(),
        vec![FollowReference {
            context_name: "kind-dev".into(),
            target: ObjectRef::core("Pod", "prod", "web-1"),
            mode: crate::ui::nav::OpenMode::Foreground,
        }]
    );
    assert!(
        source_is_focused(&mut vcx, &harness),
        "focus is back on the view"
    );
}

/// 4.2: arrows move the selection, and Enter follows it.
#[gpui_kit::test]
async fn arrows_move_the_selection_and_enter_follows_it(cx: &mut TestAppContext) {
    let harness = harness(cx);
    let mut vcx = VisualTestContext::from_window(harness.window.into(), cx);

    vcx.simulate_keystrokes("g");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    assert_eq!(
        harness.followed.borrow().last().map(|f| f.target.clone()),
        Some(namespace("prod"))
    );
}

/// 4.2: Escape closes the picker, follows nothing, and puts focus back on the
/// view it was opened from.
#[gpui_kit::test]
async fn escape_cancels_and_returns_focus(cx: &mut TestAppContext) {
    let harness = harness(cx);
    let mut vcx = VisualTestContext::from_window(harness.window.into(), cx);

    vcx.simulate_keystrokes("g");
    vcx.run_until_parked();
    assert!(
        !source_is_focused(&mut vcx, &harness),
        "the picker has focus"
    );
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();

    assert!(harness.followed.borrow().is_empty());
    assert!(source_is_focused(&mut vcx, &harness));
}

/// 4.2: the pointer passing over an entry moves `Command`'s highlight, not the
/// selection - Enter still follows what the keyboard chose.
#[gpui_kit::test]
async fn hovering_does_not_move_the_selection(cx: &mut TestAppContext) {
    let harness = harness(cx);
    let mut vcx = VisualTestContext::from_window(harness.window.into(), cx);

    vcx.simulate_keystrokes("g");
    vcx.run_until_parked();
    vcx.update_window(harness.window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.hover(IndexPath::new(2), cx);
    })
    .unwrap();
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    assert_eq!(
        harness.followed.borrow().last().map(|f| f.target.clone()),
        Some(namespace("staging")),
        "the first entry, which the keyboard left selected - not the hovered third"
    );
}

/// A click follows the clicked entry: clicking is choosing.
#[gpui_kit::test]
async fn clicking_an_entry_follows_it(cx: &mut TestAppContext) {
    let harness = harness(cx);
    let mut vcx = VisualTestContext::from_window(harness.window.into(), cx);

    vcx.simulate_keystrokes("g");
    vcx.run_until_parked();
    vcx.update_window(harness.window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click(IndexPath::new(1), cx);
    })
    .unwrap();
    vcx.run_until_parked();

    assert_eq!(
        harness.followed.borrow().last().map(|f| f.target.clone()),
        Some(namespace("prod"))
    );
}
