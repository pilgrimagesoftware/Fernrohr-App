//! `list-loading-indicator` in a Pods panel, in a real window with the app's
//! own keymap: the delayed loading indicator, "No Pods in <namespace>", and a
//! relist that keeps its rows with the header's refreshing spinner.

use crate::consts::LIST_INDICATOR_DELAY;
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::PodsPanel;
use crate::k8s::resource::pods::test_support::pod_in;
use crate::ui::list_state::{REFRESHING_SELECTOR, text_selector};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use k8s_openapi::api::core::v1::Pod;
use kube_runtime::watcher;

const CONTEXT: &str = "state-pods";

struct Harness {
    vcx: VisualTestContext,
    panel: Entity<PodsPanel>,
}

/// A connected Pods panel scoped to `namespaces`, whose store hasn't listed.
fn pods(cx: &mut TestAppContext, namespaces: &[&str]) -> Harness {
    cx.executor().allow_parking();
    let path = crate::util::test_paths::temp_path("pods-states");
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, path.clone(), &path);
    });
    let client = {
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    };
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            CONTEXT,
            ConnectionState::Connected(client.clone()),
        )
    });
    let scope = PanelScope::new(NavTarget::pods(), CONTEXT.to_string())
        .scoped_to(namespaces.iter().map(|ns| ns.to_string()).collect());
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let connection =
                cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
            let namespaces = cx.new(|_| NamespaceList::empty());
            PodsPanel::with_connection(scope, connection, namespaces, cx)
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let vcx = VisualTestContext::from_window(window.into(), cx);
    // The first render subscribes the panel to the session's Pods store, in
    // place of its placeholder: the store the tests then feed.
    vcx.run_until_parked();
    Harness {
        vcx,
        panel: built.expect("the window built its panel"),
    }
}

fn feed(h: &mut Harness, events: Vec<watcher::Event<Pod>>) {
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        let table = panel.read(cx).table.clone();
        table.update(cx, |table, cx| {
            for event in events {
                table.apply(event);
            }
            cx.notify();
        });
    });
    h.vcx.run_until_parked();
}

fn drawn(h: &mut Harness, selector: String) -> bool {
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector.leak()).is_some()
}

fn wait_out_the_delay(h: &mut Harness) {
    h.vcx.executor().advance_clock(LIST_INDICATOR_DELAY);
    h.vcx.run_until_parked();
}

#[gpui_kit::test]
async fn a_slow_first_list_of_pods_shows_loading(cx: &mut TestAppContext) {
    let mut h = pods(cx, &[]);
    feed(
        &mut h,
        vec![
            watcher::Event::Init,
            watcher::Event::InitApply(pod_in("team-a", "u1", "web", 0)),
            watcher::Event::InitApply(pod_in("team-a", "u2", "api", 0)),
        ],
    );
    wait_out_the_delay(&mut h);
    assert!(drawn(&mut h, text_selector("Loading Pods…")));
    assert!(drawn(&mut h, text_selector("2 received")));
}

#[gpui_kit::test]
async fn a_namespace_with_no_pods_says_so(cx: &mut TestAppContext) {
    let mut h = pods(cx, &["team-a"]);
    feed(
        &mut h,
        vec![
            watcher::Event::Init,
            watcher::Event::InitApply(pod_in("team-b", "u1", "web", 0)),
            watcher::Event::InitDone,
        ],
    );
    assert!(drawn(&mut h, text_selector("No Pods in team-a")));
}

#[gpui_kit::test]
async fn a_relist_of_pods_keeps_them_and_shows_it_is_refreshing(cx: &mut TestAppContext) {
    let mut h = pods(cx, &[]);
    feed(
        &mut h,
        vec![
            watcher::Event::Init,
            watcher::Event::InitApply(pod_in("team-a", "u1", "web", 0)),
            watcher::Event::InitDone,
            watcher::Event::Init,
        ],
    );
    wait_out_the_delay(&mut h);
    assert!(drawn(&mut h, REFRESHING_SELECTOR.into()));
    assert!(
        !drawn(&mut h, text_selector("Loading Pods…")),
        "a refresh is not a first load: the rows stay"
    );
    feed(&mut h, vec![watcher::Event::InitDone]);
    assert!(!drawn(&mut h, REFRESHING_SELECTOR.into()));
}
