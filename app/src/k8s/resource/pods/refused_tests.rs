//! #446: a refused Pods list says so - in the standalone panel and in a node's embedded
//! table - instead of loading forever or claiming the node has no pods.

use super::{PodsPanel, PodsTable};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use kube_runtime::watcher;

/// A connected panel whose Pods table the server refused, drawn once. `node` makes it the
/// embedded table of a Node's detail panel.
fn refused_panel(
    cx: &mut TestAppContext,
    node: Option<&str>,
) -> (Entity<PodsPanel>, VisualTestContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (_cluster, client) = FakeCluster::start(cx);
    let node = node.map(str::to_string);
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = PodsPanel::with_stubs(
                PanelScope::new(NavTarget::pods(), "refused-ctx".to_string()),
                cx,
            );
            // Connected, with no watch started: `subscribed` stands in for one.
            panel.connection =
                cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
            panel.subscribed = true;
            panel.node = node;
            panel.table.update(cx, |table, _| {
                table.set_refused("pods is forbidden".to_string())
            });
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    (built.expect("the window built its panel"), vcx)
}

#[gpui_kit::test]
async fn a_refused_list_draws_the_refusal_not_the_table(cx: &mut TestAppContext) {
    let (panel, mut vcx) = refused_panel(cx, None);
    let table_built = vcx.update(|_, cx| panel.read(cx).pod_table.is_some());
    assert!(!table_built, "no table, so no endless Loading state");
}

#[gpui_kit::test]
async fn a_refused_node_table_does_not_claim_the_node_is_empty(cx: &mut TestAppContext) {
    let (panel, mut vcx) = refused_panel(cx, Some("node-a"));
    assert!(
        vcx.debug_bounds("node-pods-empty").is_none(),
        "not the 'no pods are scheduled' text"
    );
    let table_built = vcx.update(|_, cx| panel.read(cx).pod_table.is_some());
    assert!(!table_built);
}

#[test]
fn a_restarted_watch_clears_the_refusal() {
    let mut table = PodsTable::new();
    table.set_refused("forbidden".to_string());
    assert!(table.refused().is_some());

    table.apply(watcher::Event::Init);

    assert_eq!(table.refused(), None);
}
