//! End Agent's Hold with more than one agent on a row (`connections-window`:
//! End one agent's hold): only the chosen agent's hold ends, by click or by
//! keys, and the others keep the context.

use super::support::{agent, init, open, session};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{ClusterRegistry, Holder};
use crate::ui::confirm_dialog::deliver_first_frame;
use crate::ui::connections::render::end_hold_button_id;
use gpui_kit::TestAppContext;

fn agents_holding(cx: &mut TestAppContext, context: &str) -> Vec<Holder> {
    cx.update(|cx| ClusterRegistry::holders(cx, context))
        .into_iter()
        .filter(|holder| matches!(holder, Holder::Agent(_)))
        .collect()
}

/// Two agents share `cluster-a`; the row lists them in id order, so the first
/// agent made is the first End button.
fn two_agents(cx: &mut TestAppContext) -> (Holder, Holder) {
    init(cx);
    session(cx, "cluster-a", ConnectionState::WaitingForTunnel);
    let first = agent(cx, "cluster-a", Some("assistant"));
    let second = agent(cx, "cluster-a", Some("assistant"));
    (Holder::Agent(first), Holder::Agent(second))
}

/// Each agent's own button ends only that agent's hold, even when both report
/// the same name.
#[gpui_kit::test]
async fn an_agents_button_ends_only_its_hold(cx: &mut TestAppContext) {
    let (first, second) = two_agents(cx);
    let mut win = open(cx);

    win.click(&end_hold_button_id("cluster-a", 1));

    assert!(!win.dialog_open(), "a click on one agent asks nothing");
    assert_eq!(agents_holding(cx, "cluster-a"), vec![first]);
    assert_eq!(win.listed(), ["cluster-a"], "the other agent keeps it");
    let _ = second;
}

/// From the keyboard, `e` asks which agent; Enter ends the focused one only.
#[gpui_kit::test]
async fn the_key_asks_which_agent_and_ends_only_that_one(cx: &mut TestAppContext) {
    let (first, second) = two_agents(cx);
    let mut win = open(cx);

    win.press("e");
    assert!(win.dialog_open(), "several agents: the picker asks");
    assert_eq!(
        agents_holding(cx, "cluster-a").len(),
        2,
        "nothing ended yet"
    );
    deliver_first_frame(&mut win.vcx);
    win.press("enter");

    assert!(!win.dialog_open());
    assert_eq!(
        agents_holding(cx, "cluster-a"),
        vec![second],
        "the first agent, focused when the picker opened, was ended"
    );
    let _ = first;
}

/// Tab moves to the next agent, so the keyboard can pick any of them.
#[gpui_kit::test]
async fn tab_picks_another_agent(cx: &mut TestAppContext) {
    let (first, _second) = two_agents(cx);
    let mut win = open(cx);

    win.press("e");
    deliver_first_frame(&mut win.vcx);
    win.press("tab enter");

    assert_eq!(agents_holding(cx, "cluster-a"), vec![first]);
}

/// Escape closes the picker and ends nothing.
#[gpui_kit::test]
async fn escape_ends_nothing(cx: &mut TestAppContext) {
    two_agents(cx);
    let mut win = open(cx);

    win.press("e");
    deliver_first_frame(&mut win.vcx);
    win.press("escape");

    assert!(!win.dialog_open());
    assert_eq!(agents_holding(cx, "cluster-a").len(), 2);
}
