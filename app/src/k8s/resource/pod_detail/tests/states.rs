//! `resource-detail-ui-improvements` 1 and 4: init containers show their own
//! state - a waiting one as Waiting with its reason, never another
//! container's state - and the pod's phase and each container's state carry
//! their severity's colour beside their text.

use super::config_fixture::Harness;
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::resource::pod_detail::fetch::PodDetailState;
use crate::k8s::resource::pod_detail::fields::pod_fields;
use crate::k8s::resource::pod_detail::model::{DetailSection, DetailView, PodFieldValue};
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::k8s::resource::pod_detail::register_commands;
use crate::keymap::{self, KeymapConfig};
use crate::ui::detail::BadgeTone;
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};
use k8s_openapi::api::core::v1::Pod;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

/// A Pending pod whose init container waits on `PodInitializing` and whose app
/// container is in `CrashLoopBackOff`.
fn starting_pod() -> Pod {
    serde_json::from_value(json!({
        "metadata": { "name": "api-7d9f-ftg5t", "namespace": "staging", "uid": "u1" },
        "spec": {
            "initContainers": [{ "name": "wait-db", "image": "busybox:1.36" }],
            "containers": [{ "name": "app", "image": "registry.example/api:1.2.3" }],
        },
        "status": {
            "phase": "Pending",
            "initContainerStatuses": [{
                "name": "wait-db", "image": "busybox:1.36", "imageID": "", "ready": false,
                "restartCount": 0,
                "state": { "waiting": { "reason": "PodInitializing", "message": "waiting for db" } },
            }],
            "containerStatuses": [{
                "name": "app", "image": "registry.example/api:1.2.3", "imageID": "",
                "ready": false, "restartCount": 4,
                "state": { "waiting": { "reason": "CrashLoopBackOff", "message": "back-off 40s" } },
            }],
        },
    }))
    .unwrap()
}

fn harness(cx: &mut TestAppContext, pod: Pod) -> Harness {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let connection =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let pod_ref = PodRef {
            namespace: "staging".into(),
            name: "api-7d9f-ftg5t".into(),
        };
        let scope = PanelScope::new(
            NavTarget::pod("staging", "api-7d9f-ftg5t"),
            "kind-dev".into(),
        );
        let panel = cx.new(|cx| {
            let mut panel = PodDetailPanel::with_connection(
                pod_ref,
                scope,
                DetailView::Structured,
                connection,
                cx,
            );
            panel.state = PodDetailState::Loaded(Box::new(pod));
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    Harness {
        window,
        panel: built.expect("the window built its panel"),
        config_reads: Arc::new(AtomicUsize::new(0)),
    }
}

/// Whether the element with `selector` is drawn.
fn drawn(vcx: &mut VisualTestContext, h: &Harness, selector: &'static str) -> bool {
    let _ = vcx.update_window(h.window.into(), |_, window, cx| window.render_frame(cx));
    vcx.debug_bounds(selector).is_some()
}

/// The containers (or init containers) field's states, as the view shows them.
fn states(pod: &Pod, label: &str) -> Vec<(String, String, Option<String>, BadgeTone)> {
    let fields = pod_fields(pod, jiff::Timestamp::now());
    let Some(PodFieldValue::Containers(containers)) = fields
        .into_iter()
        .find(|field| field.label == label)
        .map(|field| field.value)
    else {
        panic!("no {label} field");
    };
    containers
        .into_iter()
        .map(|c| (c.name, c.state, c.state_message, c.state_tone))
        .collect()
}

/// Spec: "A waiting init container" - Waiting with its reason (and message),
/// in the info colour, read from the init containers' own statuses.
#[gpui_kit::test]
async fn a_waiting_init_container_reads_as_waiting(cx: &mut TestAppContext) {
    let h = harness(cx, starting_pod());
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    vcx.update(|_, cx| {
        h.panel.update(cx, |panel, cx| {
            panel.set_active_tab(DetailSection::Containers, cx)
        })
    });
    assert!(
        drawn(&mut vcx, &h, "container-state-wait-db"),
        "the init container's state is drawn"
    );
    assert_eq!(
        states(&starting_pod(), "Init Containers"),
        [(
            "wait-db".to_string(),
            "Waiting: PodInitializing".to_string(),
            Some("waiting for db".to_string()),
            BadgeTone::Info
        )]
    );
}

/// Spec: "A crashing container" - the danger colour, with the text
/// `CrashLoopBackOff`; and the Pending phase in the info colour.
#[gpui_kit::test]
async fn a_crashing_container_and_the_phase_carry_their_colour(cx: &mut TestAppContext) {
    let h = harness(cx, starting_pod());
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    assert!(
        drawn(&mut vcx, &h, "pod-status-value"),
        "the phase is drawn"
    );
    vcx.update(|_, cx| {
        h.panel.update(cx, |panel, cx| {
            panel.set_active_tab(DetailSection::Containers, cx)
        })
    });
    assert!(drawn(&mut vcx, &h, "container-state-app"));
    let app = &states(&starting_pod(), "Containers")[0];
    assert_eq!(app.1, "Waiting: CrashLoopBackOff");
    assert_eq!(app.3, BadgeTone::Bad, "CrashLoopBackOff is danger");
    let fields = pod_fields(&starting_pod(), jiff::Timestamp::now());
    let phase = fields
        .into_iter()
        .find(|field| field.label == "Status")
        .map(|field| field.value);
    assert_eq!(
        phase,
        Some(PodFieldValue::Status {
            text: "Pending".into(),
            tone: BadgeTone::Info
        })
    );
}

/// A completed init container is success with its exit code, not neutral.
#[test]
fn a_completed_init_container_is_success_with_its_exit_code() {
    let mut pod = starting_pod();
    let statuses = pod
        .status
        .as_mut()
        .unwrap()
        .init_container_statuses
        .as_mut()
        .unwrap();
    statuses[0].state = Some(
        serde_json::from_value(json!({ "terminated": { "exitCode": 0, "reason": "Completed" } }))
            .unwrap(),
    );
    let init = &states(&pod, "Init Containers")[0];
    assert_eq!(init.1, "Terminated: Completed (exit 0)");
    assert_eq!(init.3, BadgeTone::Good);
}
