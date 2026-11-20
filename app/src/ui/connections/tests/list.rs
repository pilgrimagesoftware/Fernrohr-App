//! What the window lists (`connections-window`: lists every connection, holders
//! identify who uses a connection): agent-only, a connection that closes,
//! nothing connected, a window and an agent sharing one, an unnamed agent, and
//! an ended agent's lingering hold.

use super::support::{agent, init, open, session, workspace};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{ClusterRegistry, Holder};
use crate::ui::connections::render::{
    EMPTY_SELECTOR, row_selector, state_selector, tunnel_selector,
};
use crate::ui::connections::rows::HolderLabel;
use gpui_kit::TestAppContext;

fn waiting() -> ConnectionState {
    ConnectionState::WaitingForTunnel
}

/// Spec "Agent-only connection is listed".
#[gpui_kit::test]
async fn an_agent_only_connection_is_listed_with_its_state_and_holder(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    let assistant = agent(cx, "cluster-a", Some("assistant"));
    let mut vcx = open(cx);

    let rows = vcx.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].context, "cluster-a");
    assert_eq!(
        rows[0].item.text, "Waiting for tunnel",
        "the status bar's state text"
    );
    assert_eq!(
        rows[0].holders,
        [HolderLabel::Agent {
            id: assistant,
            name: Some("assistant".into()),
            connected_it: true,
        }],
        "the agent is the only holder"
    );
    assert_eq!(
        rows[0].holders[0].text(),
        "reports itself as \u{201c}assistant\u{201d}",
        "its name is labelled as self-reported"
    );
    assert!(vcx.drawn(&row_selector("cluster-a")));
    assert!(vcx.drawn(&state_selector("cluster-a")));
    assert!(!vcx.drawn(EMPTY_SELECTOR));
    assert_eq!(vcx.selected().as_deref(), Some("cluster-a"));
}

/// Spec "Connection closes".
#[gpui_kit::test]
async fn a_row_disappears_when_its_last_holder_lets_go(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    session(cx, "cluster-b", waiting());
    let _a = agent(cx, "cluster-a", None);
    let b = agent(cx, "cluster-b", None);
    let mut vcx = open(cx);
    assert_eq!(vcx.listed(), ["cluster-a", "cluster-b"]);

    cx.update(|cx| ClusterRegistry::release(cx, "cluster-b", b));
    vcx.settle();

    assert_eq!(vcx.listed(), ["cluster-a"]);
    assert!(!vcx.drawn(&row_selector("cluster-b")));
}

/// A selected row that closes hands the selection to the row now in its place.
#[gpui_kit::test]
async fn the_selection_moves_to_a_neighbour_when_its_row_closes(cx: &mut TestAppContext) {
    init(cx);
    for context in ["cluster-a", "cluster-b", "cluster-c"] {
        session(cx, context, waiting());
    }
    let _a = agent(cx, "cluster-a", None);
    let b = agent(cx, "cluster-b", None);
    let _c = agent(cx, "cluster-c", None);
    let mut vcx = open(cx);
    vcx.press("down");
    assert_eq!(vcx.selected().as_deref(), Some("cluster-b"));

    cx.update(|cx| ClusterRegistry::release(cx, "cluster-b", b));
    vcx.settle();

    assert_eq!(vcx.selected().as_deref(), Some("cluster-c"));
}

/// Spec "Nothing connected".
#[gpui_kit::test]
async fn it_says_so_when_no_context_is_connected(cx: &mut TestAppContext) {
    init(cx);
    let mut vcx = open(cx);

    assert!(vcx.rows().is_empty());
    assert!(vcx.drawn(EMPTY_SELECTOR), "No context is connected");
    assert_eq!(vcx.selected(), None);
}

/// A session nobody holds - one a read opened and nothing took - isn't listed.
#[gpui_kit::test]
async fn a_session_with_no_holder_is_not_listed(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    let mut vcx = open(cx);

    assert!(vcx.listed().is_empty());
}

/// Spec "Window and agent share a connection".
#[gpui_kit::test]
async fn a_shared_connection_lists_the_window_and_marks_who_connected_it(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    // The agent connects it first; a window then takes it too.
    let assistant = agent(cx, "cluster-a", Some("assistant"));
    let _window = workspace(cx, &["cluster-a"]);
    let titles = cx.update(|cx| crate::util::shell::windows_holding("cluster-a", cx));
    assert_eq!(titles.len(), 1, "the window holds it");
    let mut vcx = open(cx);

    let rows = vcx.rows();
    assert_eq!(
        rows[0].holders,
        [
            HolderLabel::Window {
                title: titles[0].1.clone()
            },
            HolderLabel::Agent {
                id: assistant,
                name: Some("assistant".into()),
                connected_it: true,
            },
        ],
        "windows first, then the agent, still marked as the one that connected it"
    );
    assert_eq!(rows[0].holders[0].text(), titles[0].1);
}

/// Spec "Client without a name".
#[gpui_kit::test]
async fn a_client_without_a_name_is_an_unnamed_agent(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-c", waiting());
    agent(cx, "cluster-c", None);
    let mut vcx = open(cx);

    assert_eq!(vcx.rows()[0].holders[0].text(), "Unnamed agent");
}

/// The window never names an agent by anything but what it reported, labelled.
#[gpui_kit::test]
async fn a_second_agent_is_not_marked_as_the_one_that_connected_it(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", Some("first"));
    agent(cx, "cluster-a", Some("second"));
    let mut vcx = open(cx);

    let marks: Vec<bool> = vcx.rows()[0]
        .holders
        .iter()
        .filter_map(|holder| match holder {
            HolderLabel::Agent { connected_it, .. } => Some(*connected_it),
            HolderLabel::Window { .. } => None,
        })
        .collect();
    assert_eq!(marks.iter().filter(|marked| **marked).count(), 1);
    assert_eq!(marks.len(), 2);
}

/// A context an ended agent's hold still keeps open is listed, with no agent
/// named: the connection is open and can be disconnected.
#[gpui_kit::test]
async fn a_lingering_hold_keeps_a_row_with_no_named_holder(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    let ended = agent(cx, "cluster-a", Some("assistant"));
    cx.update(|cx| ClusterRegistry::linger_agent(cx, ended, std::time::Duration::from_secs(3600)));
    let mut vcx = open(cx);

    let rows = vcx.rows();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].holders.is_empty(), "no agent is listed for it");
    assert!(rows[0].lingering);
    assert!(
        cx.update(|cx| ClusterRegistry::holders(cx, "cluster-a"))
            .iter()
            .all(|holder| matches!(holder, Holder::Lingering(_)))
    );
}

/// A state change on a listed connection redraws its row.
#[gpui_kit::test]
async fn a_state_change_updates_the_row(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", None);
    let mut vcx = open(cx);
    assert_eq!(vcx.rows()[0].item.text, "Waiting for tunnel");

    cx.update(|cx| {
        let connection =
            ClusterRegistry::existing_connection(cx, "cluster-a").expect("its session");
        connection.update(cx, |connection, cx| {
            connection.state = ConnectionState::Failed("refused".into());
            cx.notify();
        });
    });
    vcx.settle();

    assert_eq!(vcx.rows()[0].item.text, "Connection failed");
}

/// The bound tunnel is not drawn for a direct connection.
#[gpui_kit::test]
async fn a_direct_connection_shows_no_tunnel(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", None);
    let mut vcx = open(cx);

    assert_eq!(vcx.rows()[0].tunnel, None);
    assert!(!vcx.drawn(&tunnel_selector("cluster-a")));
}
