//! The Add Context window through real windows: a workspace window with one
//! context, and the Add Context window its add control opens.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::add_context_window::AddContextWindow;
use crate::ui::picker::PickerEvent;
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, init};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext};

struct Harness {
    workspace: VisualTestContext,
    main: Entity<MainWindow>,
    add: AnyWindowHandle,
}

/// A workspace window using `kind-dev`, with its Add Context window open.
fn harness(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        for context in ["kind-dev", "staging"] {
            ClusterRegistry::insert_test_session(cx, context, ConnectionState::Connecting);
        }
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
        main.update(cx, |main, cx| main.focus_initial(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut workspace = VisualTestContext::from_window(window.into(), cx);
    workspace.run_until_parked();
    workspace.dispatch_action(crate::ui::status_bar::AddContext);
    workspace.run_until_parked();
    let add = workspace
        .update(|_, cx| {
            main.read(cx)
                .test_status_bar()
                .and_then(|bar| bar.read(cx).add_window())
        })
        .expect("the add control opened a window");
    Harness {
        workspace,
        main,
        add,
    }
}

fn add_view(h: &mut Harness) -> Option<Entity<AddContextWindow>> {
    let add = h.add.downcast::<Root>()?;
    h.workspace.update(|_, cx| {
        add.read(cx)
            .ok()
            .and_then(|root| root.view().clone().downcast::<AddContextWindow>().ok())
    })
}

fn is_open(h: &mut Harness, window: AnyWindowHandle) -> bool {
    h.workspace.update(|_, cx| cx.windows().contains(&window))
}

fn contexts(h: &mut Harness) -> Vec<String> {
    h.workspace
        .update(|_, cx| match h.main.read(cx).test_status_bar() {
            Some(bar) => bar
                .read(cx)
                .items(cx)
                .into_iter()
                .map(|item| item.context_name)
                .collect(),
            None => Vec::new(),
        })
}

/// A context that connects lands in the window it was opened from - its capsule,
/// active - and the Add Context window closes.
#[gpui_kit::test]
async fn a_connected_context_lands_in_the_opener_and_the_window_closes(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let view = add_view(&mut h).expect("it hosts the Add Context view");
    let add = h.add;
    let picker = h.workspace.update(|_, cx| view.read(cx).picker().clone());
    add.update(&mut h.workspace, |_, _, cx| {
        picker.update(cx, |_, cx| {
            cx.emit(PickerEvent::Connected {
                context_name: "staging".into(),
            })
        });
    })
    .unwrap();
    h.workspace.run_until_parked();

    assert_eq!(contexts(&mut h), ["kind-dev", "staging"]);
    let active = h.workspace.update(|_, cx| {
        h.main
            .read(cx)
            .test_status_bar()
            .and_then(|bar| bar.read(cx).active_context())
    });
    assert_eq!(
        active.as_deref(),
        Some("staging"),
        "the added one is active"
    );
    assert!(!is_open(&mut h, add), "the Add Context window closed");
}

/// Escape closes it, by keystroke, and adds nothing.
#[gpui_kit::test]
async fn escape_closes_it_and_adds_nothing(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let add = h.add;
    let mut add_cx = VisualTestContext::from_window(add, &h.workspace);
    add_cx.run_until_parked();
    add_cx.update(|window, cx| window.render_frame(cx));
    add_cx.simulate_keystrokes("escape");
    add_cx.run_until_parked();

    assert!(!is_open(&mut h, add), "Escape closed it");
    assert_eq!(contexts(&mut h), ["kind-dev"], "and added nothing");
}

/// Once it has closed, the add control opens a fresh one.
#[gpui_kit::test]
async fn after_it_closes_the_add_control_opens_a_new_one(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let first = h.add;
    first
        .update(&mut h.workspace, |_, window, _| window.remove_window())
        .unwrap();
    h.workspace.run_until_parked();

    h.workspace
        .dispatch_action(crate::ui::status_bar::AddContext);
    h.workspace.run_until_parked();
    let second = h.workspace.update(|_, cx| {
        h.main
            .read(cx)
            .test_status_bar()
            .and_then(|bar| bar.read(cx).add_window())
    });
    let second = second.expect("a new one opened");
    assert_ne!(second, first);
    assert!(is_open(&mut h, second));
}
