//! `pod-events-time-window` 3.1: the Overview tab surfaces the pod's recent
//! Warning events within the window, with a link to the Events tab - and shows
//! nothing for a quiet pod.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::resource::pod_detail::fetch::PodDetailState;
use crate::k8s::resource::pod_detail::model::DetailSection;
use crate::k8s::resource::pod_detail::{DetailView, PodDetailPanel};
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext};
use jiff::{SignedDuration, Timestamp};
use k8s_openapi::api::core::v1::Event as K8sEvent;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};

fn event(uid: &str, type_: &str, reason: &str, minutes_ago: i64) -> K8sEvent {
    K8sEvent {
        metadata: ObjectMeta {
            uid: Some(uid.into()),
            ..Default::default()
        },
        type_: Some(type_.into()),
        reason: Some(reason.into()),
        message: Some(format!("{reason} happened")),
        last_timestamp: Some(Time(
            Timestamp::now() - SignedDuration::from_mins(minutes_ago),
        )),
        ..Default::default()
    }
}

/// A loaded pod detail panel on its Overview tab, with `events` delivered.
fn panel_with(
    cx: &mut TestAppContext,
    events: Vec<K8sEvent>,
) -> (VisualTestContext, Entity<PodDetailPanel>) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let connection =
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting));
        let pod = PodRef {
            namespace: "shop".into(),
            name: "web-1".into(),
        };
        let scope = PanelScope::new(NavTarget::pod("shop", "web-1"), "demo".into());
        let panel = cx.new(|cx| {
            let mut panel =
                PodDetailPanel::with_connection(pod, scope, DetailView::Structured, connection, cx);
            panel.state = PodDetailState::Loaded(Box::new(super::fixtures::rich_pod()));
            panel.test_set_events(events, cx);
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    (vcx, built.expect("the window built its panel"))
}

fn drawn(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.debug_bounds(selector).is_some()
}

/// A crashing pod: its recent warnings show on the Overview, newest first, the
/// Normal event and the warning older than the window left out - and the link
/// opens the Events tab.
#[gpui_kit::test]
async fn a_crashing_pods_recent_warnings_show_on_the_overview(cx: &mut TestAppContext) {
    let (mut vcx, panel) = panel_with(
        cx,
        vec![
            event("a", "Warning", "BackOff", 2),
            event("b", "Warning", "Unhealthy", 10),
            event("c", "Normal", "Pulled", 1),
            event("d", "Warning", "FailedMount", 300),
        ],
    );
    let reasons: Vec<String> = vcx.update(|_, cx| {
        panel
            .read(cx)
            .overview_warnings(Timestamp::now(), cx)
            .into_iter()
            .map(|event| event.reason)
            .collect()
    });
    assert_eq!(reasons, ["BackOff", "Unhealthy"]);
    assert!(
        drawn(&mut vcx, "pod-overview-warnings"),
        "drawn on the Overview"
    );

    let link = vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find("pod-overview-events-link")
            .expect("the link is drawn")
            .bounds()
            .center()
    });
    vcx.simulate_click(link, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| panel.read(cx).active_tab),
        DetailSection::Events,
        "the link opens the Events tab"
    );
}

/// A quiet pod - only Normal events - shows no warnings block at all.
#[gpui_kit::test]
async fn a_quiet_pod_shows_no_warnings(cx: &mut TestAppContext) {
    let (mut vcx, _panel) = panel_with(cx, vec![event("c", "Normal", "Pulled", 1)]);
    assert!(!drawn(&mut vcx, "pod-overview-warnings"));
}
