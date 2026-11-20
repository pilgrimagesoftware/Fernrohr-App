//! `app-shell`: Status bar capsule marks agent-held contexts, and the
//! capsule's Disconnect names the agents that keep the connection.
//!
//! Named imports, not a glob: see `status_bar.rs` on the compiler crash.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{AgentId, AgentLabels, ClusterRegistry};
use crate::ui::status_bar::{StatusBarView, agent_icon_selector};
use crate::util::context_lifecycle::disconnect_confirmation_body;
use crate::util::shell::MainWindow;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext};

fn bar_on(cx: &mut TestAppContext, context: &str) -> (Entity<StatusBarView>, VisualTestContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, context, ConnectionState::Connecting);
    });
    let picker = cx.add_window(MainWindow::test_picker_window);
    let main_window = cx.update(|cx| picker.root(cx).expect("open").downgrade());
    let mut built = None;
    let names = vec![context.to_string()];
    let window = cx.add_window(|window, cx| {
        let bar = cx.new(|cx| StatusBarView::for_window(names, main_window, cx));
        built = Some(bar.clone());
        Root::new(bar, window, cx)
    });
    let bar = built.expect("the window built its bar");
    (bar, VisualTestContext::from_window(window.into(), cx))
}

fn agent(cx: &mut VisualTestContext, context: &str, name: Option<&str>) -> AgentId {
    let agent = AgentId::next();
    cx.update(|_, cx| {
        AgentLabels::set(cx, agent, name.map(str::to_string));
        ClusterRegistry::hold(cx, context, agent);
    });
    cx.run_until_parked();
    agent
}

/// "Agent shares a window's context" and "Agent lets go": the capsule's item
/// carries the agent's self-reported name, then loses it when the agent goes.
#[gpui_kit::test]
async fn the_capsule_knows_which_agents_share_its_context(cx: &mut TestAppContext) {
    let (bar, mut vcx) = bar_on(cx, "cluster-a");
    let assistant = agent(&mut vcx, "cluster-a", Some("assistant"));
    let items = vcx.update(|_, cx| bar.read(cx).items(cx));
    assert_eq!(items[0].agents, vec![Some("assistant".to_string())]);

    vcx.update(|_, cx| ClusterRegistry::release_agent(cx, assistant));
    vcx.run_until_parked();
    let items = vcx.update(|_, cx| bar.read(cx).items(cx));
    assert!(items[0].agents.is_empty(), "the agent let go");
}

/// The capsule draws the agent icon, whose tooltip says the agent uses it too
/// - by its self-reported name - and draws none without an agent.
#[gpui_kit::test]
async fn the_agent_icon_shows_with_its_tooltip(cx: &mut TestAppContext) {
    let (_bar, mut vcx) = bar_on(cx, "cluster-a");
    let selector: &'static str = agent_icon_selector("cluster-a").leak();
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(vcx.debug_bounds(selector).is_none(), "no agent, no icon");

    agent(&mut vcx, "cluster-a", Some("assistant"));
    vcx.update(|window, cx| window.render_frame(cx));
    let icon = vcx.debug_bounds(selector).expect("the agent icon is drawn");
    vcx.simulate_mouse_move(icon.center(), None, Modifiers::none());
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    vcx.run_until_parked();
    let tooltip = crate::ui::icon_tooltip::selector(
        "Also used by an agent reporting itself as \u{201c}assistant\u{201d}",
    );
    assert!(
        vcx.debug_bounds(tooltip.leak()).is_some(),
        "the tooltip names the agent as self-reported"
    );
}

/// "Context an agent still uses": Disconnect's confirmation says the
/// connection stays for the agents holding it, named as they report themselves.
#[test]
fn the_disconnect_confirmation_names_the_agents_that_keep_it() {
    let body =
        disconnect_confirmation_body("cluster-a", 2, 0, &[Some("assistant".to_string()), None]);
    assert_eq!(
        body.plain(),
        "2 open panels for \u{201c}cluster-a\u{201d} will close. \u{201c}cluster-a\u{201d} stays \
         connected for an agent reporting itself as \u{201c}assistant\u{201d}, an unnamed agent."
    );
}
