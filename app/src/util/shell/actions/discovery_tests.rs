//! `discovery-resilience` through a real window (a `Root`, the app's keymap), its
//! context connected to a stand-in API server whose aggregated `metrics.k8s.io`
//! group answers 503: the other groups' kinds still list, a warning names the
//! failed group, and Resources: Refresh picks the group up once it recovers -
//! keeping the panel's collapse state.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::mock_api::{
    MockApi, cluster_whose_group_list_fails, cluster_with_a_failing_aggregated_group,
    recover_metrics,
};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{MainWindow, init};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext};

struct Harness {
    vcx: VisualTestContext,
    main: Entity<MainWindow>,
}

fn harness(cx: &mut TestAppContext, api: &MockApi) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    let client = {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        api.client()
    };
    cx.update(|cx| {
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connected(client));
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        main.update(cx, |main, cx| main.focus_initial(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    Harness { vcx, main }
}

fn kinds(h: &mut Harness) -> Option<Vec<String>> {
    h.vcx.update(|_, cx| {
        let panel = h.main.read(cx).test_resource_panel()?;
        let panel = panel.read(cx);
        panel
            .kinds()
            .map(|kinds| kinds.iter().map(|kind| kind.gvk.kind.clone()).collect())
    })
}

fn unavailable(h: &mut Harness) -> Vec<String> {
    h.vcx.update(|_, cx| {
        h.main
            .read(cx)
            .test_resource_panel()
            .unwrap()
            .read(cx)
            .test_unavailable_groups()
    })
}

/// Waits for `done`, letting the runtime's real network I/O land between parks.
fn wait_for(h: &mut Harness, mut done: impl FnMut(&mut Harness) -> bool) {
    for _ in 0..200 {
        h.vcx.run_until_parked();
        if done(h) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("timed out waiting for discovery");
}

fn drawn(h: &mut Harness, selector: &'static str) -> bool {
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector).is_some()
}

#[gpui_kit::test]
async fn a_failing_group_lists_the_rest_warns_and_refresh_recovers_it(cx: &mut TestAppContext) {
    let api = cluster_with_a_failing_aggregated_group();
    let mut h = harness(cx, &api);
    wait_for(&mut h, |h| kinds(h).is_some());

    let listed = kinds(&mut h).unwrap();
    assert!(
        listed.contains(&"Pod".into()) && listed.contains(&"Deployment".into()),
        "the other groups' kinds list: {listed:?}"
    );
    assert_eq!(
        unavailable(&mut h),
        ["metrics.k8s.io"],
        "the warning names it"
    );
    assert!(
        drawn(&mut h, "resource-unavailable"),
        "the warning row is drawn"
    );
    assert!(
        !drawn(&mut h, "resource-unavailable-list"),
        "collapsed at first"
    );
    let toggle = h.vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find("resource-unavailable-toggle")
            .expect("the toggle is drawn")
            .bounds()
            .center()
    });
    h.vcx.simulate_click(toggle, Modifiers::none());
    h.vcx.run_until_parked();
    assert!(
        drawn(&mut h, "resource-unavailable-list"),
        "expands to the list"
    );

    // Collapse Workloads, so refresh can be shown to keep it.
    let pods = DiscoveredKind::pods();
    h.vcx.update(|_, cx| {
        h.main
            .read(cx)
            .test_resource_panel()
            .unwrap()
            .update(cx, |panel, cx| panel.test_toggle_section_of(&pods, cx))
    });
    let collapsed = |h: &mut Harness| {
        h.vcx.update(|_, cx| {
            h.main
                .read(cx)
                .test_resource_panel()
                .unwrap()
                .read(cx)
                .is_section_collapsed(&DiscoveredKind::pods())
        })
    };
    assert!(collapsed(&mut h));

    recover_metrics(&api);
    press(
        &mut h.vcx,
        crate::ui::resource_panel::REFRESH_DEFAULT_BINDING,
    );
    wait_for(&mut h, |h| unavailable(h).is_empty());
    assert!(
        kinds(&mut h).unwrap().contains(&"PodMetrics".into()),
        "picked up"
    );
    assert!(!drawn(&mut h, "resource-unavailable"), "the warning went");
    assert!(collapsed(&mut h), "refresh kept the collapse state");
}

/// Listing the groups failing is the panel's error state, read as the HTTP status
/// and message - drawn in full, not a clipped dump.
#[gpui_kit::test]
async fn failing_to_list_groups_shows_a_readable_error(cx: &mut TestAppContext) {
    let api = cluster_whose_group_list_fails();
    let mut h = harness(cx, &api);
    wait_for(&mut h, |h| drawn(h, "resource-discovery-failure"));
    let message = h.vcx.update(|_, cx| {
        h.main
            .read(cx)
            .test_resource_panel()
            .unwrap()
            .read(cx)
            .test_failure()
    });
    let message = message.expect("a failure");
    assert!(message.contains("HTTP 503"), "{message}");
    assert!(!message.contains("Status {"), "{message}");
}
