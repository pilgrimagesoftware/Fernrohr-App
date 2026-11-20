//! The command palette through real keystrokes: which commands it offers, how
//! the selection moves, and that a disabled row never runs.

use super::open;
use crate::command::{Command, CommandRegistry};
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext as _, Context, FocusHandle, IntoElement, Render, TestAppContext, VisualTestContext,
    Window, actions, div,
};
use std::cell::Cell;
use std::rc::Rc;

actions!(palette_test, [First, Second, Third]);

struct Host {
    focus: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::InteractiveElement as _;
        // A panel's own context, and a conditional one beside it.
        let mut context = gpui_kit::KeyContext::default();
        context.add("TestPanel");
        context.add("TestCondition");
        div().key_context(context).track_focus(&self.focus)
    }
}

/// A command gated to a context nothing on the focus path adds is not
/// offered: the palette is empty, and Enter runs nothing.
#[gpui_kit::test]
async fn an_absent_contexts_command_is_not_offered(cx: &mut TestAppContext) {
    let ran = Rc::new(Cell::new(false));
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        let mut registry = CommandRegistry::new();
        registry.register(Command {
            id: "test.absent",
            title: "Absent Test Command",
            default_binding: "",
            context: Some("NotPushed && !Input"),
            action: Box::new(First),
            menu: None,
        });
        cx.set_global(registry);
        let flag = ran.clone();
        cx.on_action(move |_: &First, _cx| flag.set(true));
    });
    let window = cx.add_window(|window, cx| {
        let host = cx.new(|cx| Host {
            focus: cx.focus_handle(),
        });
        let focus = host.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root::new(host, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(open);
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert!(
        !ran.get(),
        "a command whose context isn't pushed isn't offered"
    );
}

/// A command gated to a context an element adds beside its primary one -
/// `PodDetailShellable` beside `PodDetailPanel` - is offered, and runs.
#[gpui_kit::test]
async fn a_secondary_contexts_command_is_offered(cx: &mut TestAppContext) {
    let ran = Rc::new(Cell::new(false));
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        let mut registry = CommandRegistry::new();
        registry.register(Command {
            id: "test.conditional",
            title: "Conditional Test Command",
            default_binding: "",
            context: Some("TestCondition && !Input"),
            action: Box::new(First),
            menu: None,
        });
        cx.set_global(registry);
        let flag = ran.clone();
        cx.on_action(move |_: &First, _cx| flag.set(true));
    });
    let window = cx.add_window(|window, cx| {
        let host = cx.new(|cx| Host {
            focus: cx.focus_handle(),
        });
        let focus = host.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root::new(host, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(open);
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert!(
        ran.get(),
        "the secondary context's command was offered and ran"
    );
}

/// Down moves through the filtered list and Enter runs the selected command.
#[gpui_kit::test]
async fn arrows_select_and_enter_runs(cx: &mut TestAppContext) {
    let ran_second = Rc::new(Cell::new(false));
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        let mut registry = CommandRegistry::new();
        for (id, title, action) in [
            (
                "test.first",
                "First Test Command",
                Box::new(First) as Box<dyn gpui_kit::Action>,
            ),
            ("test.second", "Second Test Command", Box::new(Second)),
        ] {
            registry.register(Command {
                id,
                title,
                default_binding: "",
                context: None,
                action,
                menu: None,
            });
        }
        cx.set_global(registry);
        let flag = ran_second.clone();
        cx.on_action(move |_: &Second, _cx| flag.set(true));
    });
    let window = cx.add_window(|window, cx| {
        let host = cx.new(|cx| Host {
            focus: cx.focus_handle(),
        });
        let focus = host.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root::new(host, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(open);
    vcx.run_until_parked();

    vcx.simulate_keystrokes("down enter");
    vcx.run_until_parked();
    assert!(
        ran_second.get(),
        "Down selects the second command and Enter runs it"
    );
}

/// Typing filters and moves the selection to the first match; Enter runs it.
#[gpui_kit::test]
async fn typing_selects_the_first_match_and_enter_runs_it(cx: &mut TestAppContext) {
    let ran_second = Rc::new(Cell::new(false));
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        let mut registry = CommandRegistry::new();
        for (id, title, action) in [
            (
                "test.first",
                "First Test Command",
                Box::new(First) as Box<dyn gpui_kit::Action>,
            ),
            ("test.second", "Second Test Command", Box::new(Second)),
        ] {
            registry.register(Command {
                id,
                title,
                default_binding: "",
                context: None,
                action,
                menu: None,
            });
        }
        cx.set_global(registry);
        let flag = ran_second.clone();
        cx.on_action(move |_: &Second, _cx| flag.set(true));
    });
    let window = cx.add_window(|window, cx| {
        let host = cx.new(|cx| Host {
            focus: cx.focus_handle(),
        });
        let focus = host.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root::new(host, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(open);
    vcx.run_until_parked();

    vcx.simulate_input("second");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert!(ran_second.get(), "the filter's first match runs on Enter");
}

/// #406: a command the focused panel says can't run is listed with its
/// reason, the selection starts past it so Enter runs the next one, and
/// clicking it runs nothing.
#[gpui_kit::test]
async fn an_unavailable_command_is_listed_with_its_reason_and_never_runs(cx: &mut TestAppContext) {
    let (ran_first, ran_second) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        let mut registry = CommandRegistry::new();
        for (id, title, action) in [
            (
                "test.first",
                "First Test Command",
                Box::new(First) as Box<dyn gpui_kit::Action>,
            ),
            ("test.second", "Second Test Command", Box::new(Second)),
        ] {
            registry.register(Command {
                id,
                title,
                default_binding: "",
                context: None,
                action,
                menu: None,
            });
        }
        cx.set_global(registry);
        let first = ran_first.clone();
        cx.on_action(move |_: &First, _cx| first.set(true));
        let second = ran_second.clone();
        cx.on_action(move |_: &Second, _cx| second.set(true));
    });
    let window = cx.add_window(|window, cx| {
        let host = cx.new(|cx| Host {
            focus: cx.focus_handle(),
        });
        let focus = host.read(cx).focus.clone();
        crate::command::unavailable::register(
            &focus,
            |action, _| {
                action
                    .as_any()
                    .is::<First>()
                    .then(|| "Not right now.".into())
            },
            cx,
        );
        window.focus(&focus, cx);
        Root::new(host, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(open);
    vcx.run_until_parked();

    let reason = vcx
        .debug_bounds("palette-reason-0")
        .expect("the first row shows its reason");
    assert!(
        vcx.debug_bounds("palette-reason-1").is_none(),
        "a runnable row shows none"
    );
    vcx.simulate_click(reason.center(), gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert!(!ran_first.get(), "clicking it runs nothing");

    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert!(!ran_first.get(), "Enter doesn't run it either");
    assert!(ran_second.get(), "Enter runs the first runnable command");
}

/// Down from a runnable row past a disabled one lands on the next runnable
/// row, so Enter runs that, never the disabled one.
#[gpui_kit::test]
async fn down_skips_past_a_disabled_row(cx: &mut TestAppContext) {
    let ran: Rc<std::cell::RefCell<Vec<&'static str>>> = Rc::default();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        let mut registry = CommandRegistry::new();
        for (id, title, action) in [
            (
                "test.first",
                "First Test Command",
                Box::new(First) as Box<dyn gpui_kit::Action>,
            ),
            ("test.second", "Second Test Command", Box::new(Second)),
            ("test.third", "Third Test Command", Box::new(Third)),
        ] {
            registry.register(Command {
                id,
                title,
                default_binding: "",
                context: None,
                action,
                menu: None,
            });
        }
        cx.set_global(registry);
        let log = ran.clone();
        cx.on_action(move |_: &First, _cx| log.borrow_mut().push("first"));
        let log = ran.clone();
        cx.on_action(move |_: &Second, _cx| log.borrow_mut().push("second"));
        let log = ran.clone();
        cx.on_action(move |_: &Third, _cx| log.borrow_mut().push("third"));
    });
    let window = cx.add_window(|window, cx| {
        let host = cx.new(|cx| Host {
            focus: cx.focus_handle(),
        });
        let focus = host.read(cx).focus.clone();
        crate::command::unavailable::register(
            &focus,
            |action, _| {
                action
                    .as_any()
                    .is::<Second>()
                    .then(|| "Not right now.".into())
            },
            cx,
        );
        window.focus(&focus, cx);
        Root::new(host, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(open);
    vcx.run_until_parked();

    vcx.simulate_keystrokes("down enter");
    vcx.run_until_parked();
    assert_eq!(*ran.borrow(), ["third"], "Down passed the disabled row");
}
