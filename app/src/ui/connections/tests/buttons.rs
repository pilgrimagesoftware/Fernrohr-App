//! The row icon buttons by clicks and hovers (`icon-buttons.md`): each does what
//! its key does, selects its own row first, and names itself in a tooltip.

use super::support::{Win, agent, init, open, session, window_holders, workspace};
use crate::k8s::cluster::connection::ConnectionState;
use crate::ui::confirm_dialog::{cancel_id, confirm_id};
use crate::ui::connections::actions::{DISCONNECT_ID_PREFIX, disconnect_body};
use crate::ui::connections::render::{
    DISCONNECT_TOOLTIP, END_HOLD_TOOLTIP, OPEN_TOOLTIP, disconnect_button_id, end_hold_button_id,
    open_button_id,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, TestAppContext};

fn waiting() -> ConnectionState {
    ConnectionState::WaitingForTunnel
}

/// Hovers the element `id` and returns whether its tooltip reading `label`
/// showed.
fn tooltip_shows(win: &mut Win, id: &str, label: &'static str) -> bool {
    let vcx = &mut win.vcx;
    let id: &'static str = Box::leak(id.to_string().into_boxed_str());
    let at = vcx.update(|window, cx| {
        window.render_frame(cx);
        window.find(id).bounds().center()
    });
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    vcx.run_until_parked();
    let selector: &'static str = crate::ui::icon_tooltip::selector(label).leak();
    vcx.debug_bounds(selector).is_some()
}

/// Every button names its action in a tooltip.
#[gpui_kit::test]
async fn each_button_has_its_tooltip(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", Some("assistant"));
    let mut vcx = open(cx);

    for (id, label) in [
        (open_button_id("cluster-a"), OPEN_TOOLTIP),
        (disconnect_button_id("cluster-a"), DISCONNECT_TOOLTIP),
        (end_hold_button_id("cluster-a", 0), END_HOLD_TOOLTIP),
    ] {
        assert!(tooltip_shows(&mut vcx, &id, label), "{label}'s tooltip");
    }
}

#[gpui_kit::test]
async fn the_open_button_adds_the_context_to_the_frontmost_window(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "other", waiting());
    let _window = workspace(cx, &["other"]);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", None);
    let mut vcx = open(cx);

    vcx.click(&open_button_id("cluster-a"));

    assert_eq!(window_holders(cx, "cluster-a"), 1);
}

#[gpui_kit::test]
async fn the_disconnect_button_asks_then_confirms_or_cancels(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", Some("assistant"));
    let mut vcx = open(cx);

    vcx.click(&disconnect_button_id("cluster-a"));
    assert!(vcx.dialog_open(), "it asks");
    vcx.click(&cancel_id(DISCONNECT_ID_PREFIX));
    assert!(!vcx.dialog_open());
    assert_eq!(vcx.listed(), ["cluster-a"], "Cancel keeps it");

    vcx.click(&disconnect_button_id("cluster-a"));
    vcx.click(&confirm_id(DISCONNECT_ID_PREFIX));
    assert!(vcx.listed().is_empty(), "Disconnect closes it");
}

#[gpui_kit::test]
async fn the_end_hold_button_releases_the_agent_without_asking(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", Some("assistant"));
    let mut vcx = open(cx);

    vcx.click(&end_hold_button_id("cluster-a", 0));

    assert!(!vcx.dialog_open());
    assert!(vcx.listed().is_empty());
}

/// A row's button acts on its own row, whichever is selected.
#[gpui_kit::test]
async fn a_button_selects_its_own_row_first(cx: &mut TestAppContext) {
    init(cx);
    for context in ["cluster-a", "cluster-b"] {
        session(cx, context, waiting());
        agent(cx, context, None);
    }
    let mut vcx = open(cx);
    assert_eq!(vcx.selected().as_deref(), Some("cluster-a"));

    vcx.click(&end_hold_button_id("cluster-b", 0));

    assert_eq!(vcx.listed(), ["cluster-a"], "b's agent was ended");
    vcx.settle();
}

/// A row with no agent has no End Agent's Hold button.
#[gpui_kit::test]
async fn a_window_only_row_has_no_end_hold_button(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    let _window = workspace(cx, &["cluster-a"]);
    let mut vcx = open(cx);

    let id: &'static str = end_hold_button_id("cluster-a", 0).to_string().leak();
    vcx.vcx.update(|window, cx| window.render_frame(cx));
    assert!(vcx.vcx.update(|window, _| window.try_find(id)).is_none());
}

/// The confirmation names each window and agent, an agent as it reported
/// itself.
#[gpui_kit::test]
async fn the_confirmation_names_windows_and_agents(cx: &mut TestAppContext) {
    init(cx);
    session(cx, "cluster-a", waiting());
    agent(cx, "cluster-a", Some("assistant"));
    agent(cx, "cluster-a", None);
    let _window = workspace(cx, &["cluster-a"]);
    let mut vcx = open(cx);

    let row = vcx.rows().remove(0);
    let title = row.window_titles().remove(0);
    let body = disconnect_body(&row);

    assert_eq!(
        body.plain(),
        format!(
            "\u{201c}cluster-a\u{201d} will disconnect. It closes the panels for it in \u{201c}{title}\u{201d}. \
             It ends the hold of \u{201c}assistant\u{201d} (self-reported), an unnamed agent."
        )
    );
    assert_eq!(body.name_list(), ["cluster-a", title.as_str(), "assistant"]);
}
