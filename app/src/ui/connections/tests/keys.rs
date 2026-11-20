//! Selection and the three actions by real keystrokes (`connections-window`
//! "Connections window is keyboard operable", "Open a connection in a window",
//! "Disconnect a connection", "End one agent's hold", "Opening the Connections
//! window").

use super::support::{agent, init, open, session, window_holders, workspace};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{ClusterRegistry, Holder};
use crate::ui::connections::render::row_selector;
use gpui_kit::{Modifiers, TestAppContext};

fn waiting() -> ConnectionState {
    ConnectionState::WaitingForTunnel
}

fn holders(cx: &mut TestAppContext, context: &str) -> Vec<Holder> {
    cx.update(|cx| ClusterRegistry::holders(cx, context))
}

fn two_rows(cx: &mut TestAppContext) {
    for context in ["cluster-a", "cluster-b"] {
        session(cx, context, waiting());
        agent(cx, context, Some("assistant"));
    }
}

/// Spec "Open from the palette": the command's key opens it, and again brings
/// the one open forward rather than opening a second.
#[gpui_kit::test]
async fn the_open_command_opens_one_window_and_refocuses_it(cx: &mut TestAppContext) {
    init(cx);
    let mut vcx = open(cx);
    let windows = cx.update(|cx| cx.windows().len());

    // Pressed again with focus in the window itself.
    vcx.press(crate::ui::connections::commands::OPEN_DEFAULT_BINDING);

    assert_eq!(
        cx.update(|cx| cx.windows().len()),
        windows,
        "no second window"
    );
}

/// Up and Down move the selection and stop at the ends.
#[gpui_kit::test]
async fn the_arrow_keys_move_the_selection(cx: &mut TestAppContext) {
    init(cx);
    two_rows(cx);
    let mut vcx = open(cx);
    assert_eq!(vcx.selected().as_deref(), Some("cluster-a"));

    vcx.press("down");
    assert_eq!(vcx.selected().as_deref(), Some("cluster-b"));
    vcx.press("down");
    assert_eq!(vcx.selected().as_deref(), Some("cluster-b"), "the end");
    vcx.press("up");
    assert_eq!(vcx.selected().as_deref(), Some("cluster-a"));
}

/// A click moves the same selection; hovering a row never does.
#[gpui_kit::test]
async fn a_click_selects_a_row_and_hover_does_not(cx: &mut TestAppContext) {
    init(cx);
    two_rows(cx);
    let mut vcx = open(cx);

    assert!(vcx.drawn(&row_selector("cluster-b")));
    let at = vcx
        .vcx
        .debug_bounds(Box::leak(row_selector("cluster-b").into_boxed_str()) as &'static str)
        .expect("drawn")
        .center();
    vcx.vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.settle();
    assert_eq!(
        vcx.selected().as_deref(),
        Some("cluster-a"),
        "hover moves nothing"
    );

    vcx.click_selector(&row_selector("cluster-b"));
    assert_eq!(vcx.selected().as_deref(), Some("cluster-b"));
}

/// Spec "Open an agent's connection": Enter adds the context to the frontmost
/// window, which holds the connection that is already open.
#[gpui_kit::test]
async fn open_in_window_reuses_the_connection_without_approval(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "other", waiting());
    let _window = workspace(cx, &["other"]);
    session(cx, "cluster-a", waiting());
    let assistant = agent(cx, "cluster-a", Some("assistant"));
    let before = cx.update(|cx| ClusterRegistry::existing_connection(cx, "cluster-a"));
    let mut vcx = open(cx);
    assert_eq!(window_holders(cx, "cluster-a"), 0);

    vcx.press("enter");

    assert_eq!(
        window_holders(cx, "cluster-a"),
        1,
        "the window holds it now"
    );
    assert!(
        holders(cx, "cluster-a").contains(&Holder::Agent(assistant)),
        "the agent keeps it"
    );
    let after = cx.update(|cx| ClusterRegistry::existing_connection(cx, "cluster-a"));
    assert_eq!(before, after, "the connection was reused, not redialled");
    let rows = vcx.rows();
    assert_eq!(rows[0].window_titles().len(), 1, "the row lists the window");
}

/// Spec "Disconnect from the keyboard" and "Disconnect a shared connection".
#[gpui_kit::test]
async fn disconnect_asks_then_releases_every_holder(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", Some("assistant"));
    let _window = workspace(cx, &["cluster-a"]);
    let mut vcx = open(cx);
    assert_eq!(holders(cx, "cluster-a").len(), 2);

    vcx.press("d");
    assert!(vcx.dialog_open(), "it asks first");
    assert_eq!(holders(cx, "cluster-a").len(), 2, "nothing yet");
    vcx.press("enter");

    assert!(!vcx.dialog_open());
    assert!(holders(cx, "cluster-a").is_empty(), "every holder let go");
    assert!(vcx.listed().is_empty(), "the row is gone");
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "cluster-a"))
            .is_none(),
        "the connection closed"
    );
}

/// Spec "Cancel": every holder keeps the context.
#[gpui_kit::test]
async fn cancelling_disconnect_keeps_every_holder(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", Some("assistant"));
    let _window = workspace(cx, &["cluster-a"]);
    let mut vcx = open(cx);

    vcx.press("d");
    assert!(vcx.dialog_open());
    vcx.press("escape");

    assert!(!vcx.dialog_open());
    assert_eq!(holders(cx, "cluster-a").len(), 2, "Escape keeps both");
    assert_eq!(window_holders(cx, "cluster-a"), 1);
    assert_eq!(vcx.listed(), ["cluster-a"]);
}

/// Disconnect acts on the selected row, not the first.
#[gpui_kit::test]
async fn disconnect_takes_the_selected_row(cx: &mut TestAppContext) {
    init(cx);
    two_rows(cx);
    let mut vcx = open(cx);

    vcx.press("down");
    vcx.press("d");
    vcx.press("enter");

    assert_eq!(vcx.listed(), ["cluster-a"]);
}

/// Spec "Release an agent that's done": with no window, the connection closes.
#[gpui_kit::test]
async fn ending_the_sole_agents_hold_disconnects(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", Some("assistant"));
    let mut vcx = open(cx);

    vcx.press("e");

    assert!(!vcx.dialog_open(), "no confirmation");
    assert!(holders(cx, "cluster-a").is_empty());
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "cluster-a"))
            .is_none(),
        "the connection closed"
    );
    assert!(vcx.listed().is_empty());
}

/// With a window also holding it, ending the agent's hold leaves the window's.
#[gpui_kit::test]
async fn ending_an_agents_hold_leaves_a_windows(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", Some("assistant"));
    let _window = workspace(cx, &["cluster-a"]);
    let mut vcx = open(cx);

    vcx.press("e");

    assert_eq!(window_holders(cx, "cluster-a"), 1);
    assert_eq!(holders(cx, "cluster-a").len(), 1, "only the window's");
    assert_eq!(vcx.listed(), ["cluster-a"], "still listed");
}

/// A row with no agent has nothing for End Agent's Hold to end.
#[gpui_kit::test]
async fn ending_a_hold_on_a_window_only_row_does_nothing(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    let _window = workspace(cx, &["cluster-a"]);
    let mut vcx = open(cx);

    vcx.press("e");

    assert_eq!(window_holders(cx, "cluster-a"), 1);
}
