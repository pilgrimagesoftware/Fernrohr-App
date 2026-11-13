//! #359: Disconnect Others and Disconnect All, from the Context menu and the
//! palette, through their Recoverable confirmation answered with real keys:
//! Escape keeps every context, Enter disconnects the ones named.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::status_bar::{DisconnectAllContexts, DisconnectOtherContexts};
use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{Action, AppContext as _, Entity, TestAppContext, VisualTestContext};

/// A workspace window on `contexts`, the first active, with focus inside it.
fn workspace(
    cx: &mut TestAppContext,
    contexts: &[&str],
) -> (Entity<MainWindow>, VisualTestContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        for context in contexts {
            ClusterRegistry::insert_test_session(cx, context, ConnectionState::Connecting);
        }
    });
    let names: Vec<String> = contexts.iter().map(|name| name.to_string()).collect();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(names, window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its MainWindow");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| main.update(cx, |main, cx| main.test_focus(window, cx)));
    vcx.run_until_parked();
    (main, vcx)
}

fn contexts(main: &Entity<MainWindow>, vcx: &mut VisualTestContext) -> Vec<String> {
    vcx.update(|_, cx| match &main.read(cx).mode {
        WindowMode::Workspace { contexts, .. } => contexts.clone(),
        WindowMode::Picker(_) => Vec::new(),
    })
}

fn available(vcx: &mut VisualTestContext, action: &dyn Action) -> bool {
    vcx.update(|window, cx| window.is_action_available(action, cx))
}

fn run(vcx: &mut VisualTestContext, action: impl Action) {
    vcx.dispatch_action(action);
    vcx.run_until_parked();
}

fn dialog_open(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.has_active_dialog(cx))
}

#[gpui_kit::test]
async fn with_one_context_only_disconnect_all_is_offered(cx: &mut TestAppContext) {
    let (_main, mut vcx) = workspace(cx, &["dev"]);
    assert!(available(&mut vcx, &DisconnectAllContexts));
    assert!(
        !available(&mut vcx, &DisconnectOtherContexts),
        "no other context: the menu item shows disabled"
    );
}

#[gpui_kit::test]
async fn disconnect_others_keeps_the_active_context(cx: &mut TestAppContext) {
    let (main, mut vcx) = workspace(cx, &["dev", "staging", "test"]);
    assert!(available(&mut vcx, &DisconnectOtherContexts));

    run(&mut vcx, DisconnectOtherContexts);
    assert!(dialog_open(&mut vcx), "it asks first");
    press(&mut vcx, "escape");
    assert!(!dialog_open(&mut vcx));
    assert_eq!(
        contexts(&main, &mut vcx),
        ["dev", "staging", "test"],
        "Escape keeps all"
    );

    run(&mut vcx, DisconnectOtherContexts);
    press(&mut vcx, "enter");
    assert_eq!(
        contexts(&main, &mut vcx),
        ["dev"],
        "Enter keeps only the active one"
    );
    assert_eq!(
        vcx.update(|_, cx| ClusterRegistry::holder_count(cx, "staging")),
        0,
        "and releases the others"
    );
}

#[gpui_kit::test]
async fn disconnect_all_returns_the_window_to_the_picker(cx: &mut TestAppContext) {
    let (main, mut vcx) = workspace(cx, &["dev", "staging"]);
    run(&mut vcx, DisconnectAllContexts);
    assert!(dialog_open(&mut vcx), "it asks first");
    press(&mut vcx, "enter");
    assert!(contexts(&main, &mut vcx).is_empty());
    assert!(
        vcx.update(|_, cx| matches!(main.read(cx).mode, WindowMode::Picker(_))),
        "back to the cluster picker"
    );
}
