//! `logs-panel-instancing` through a real window: a pod's logs open in a
//! Logs panel of its own by default - one per pod, the same pod's again
//! focusing it - and the flipped action, or the reuse preference, opens the
//! one shared panel instead. Another container of an open pod switches its
//! panel; a pinned panel ignores later selections.

use crate::config::ui::LogsPanels;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::nav::{NavTarget, OpenedPanel, PodRef, ShowLogs, ShowLogsFlipped};
use crate::util::logs::LogsPanel;
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::{Action, AppContext as _, Entity, TestAppContext, VisualTestContext};

/// A Logs panel as a test sees it: its target, and - a pod's own panel - the
/// pod and container it's pinned to.
type SeenLogsPanel = (NavTarget, Option<(String, Option<String>)>);

struct Harness {
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
}

fn harness(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    Harness { main, vcx }
}

impl Harness {
    /// Selects `pod` (its containers in this order) and opens its logs with
    /// `action`, as every entry point does.
    fn open(&mut self, pod: &str, containers: &[&str], action: impl Action) {
        let selection = PodSelection {
            namespace: "shop".into(),
            name: pod.into(),
            containers: containers.iter().map(|c| c.to_string()).collect(),
            context_name: "demo".into(),
        };
        let main = self.main.clone();
        self.vcx.update(|window, cx| {
            cx.set_global(SelectedPod(Some(selection)));
            let focus = main.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
            window.dispatch_action(action.boxed_clone(), cx);
        });
        self.vcx.run_until_parked();
    }

    /// The window's Logs panels: each one's target and, for a pod's own, the
    /// pod and container it's pinned to.
    fn logs_panels(&mut self) -> Vec<SeenLogsPanel> {
        let main = self.main.clone();
        self.vcx.update(|_, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
                return Vec::new();
            };
            open_panels
                .iter()
                .filter_map(|open| match &open.panel {
                    Some(OpenedPanel::Logs(panel)) => {
                        Some((open.key.target.clone(), panel.read(cx).test_pinned()))
                    }
                    _ => None,
                })
                .collect()
        })
    }

    fn set_preference(&mut self, panels: LogsPanels) {
        self.vcx
            .update(|_, cx| crate::ui::logs_panels::set(panels, cx));
    }
}

fn pod_logs(name: &str) -> NavTarget {
    NavTarget::PodLogs(PodRef {
        namespace: "shop".into(),
        name: name.into(),
    })
}

/// By default each pod's logs get a panel of their own, and opening the same
/// pod's logs again focuses its panel rather than adding one.
#[gpui_kit::test]
async fn each_pods_logs_open_in_their_own_panel_by_default(cx: &mut TestAppContext) {
    let mut h = harness(cx);

    h.open("web-1", &["web"], ShowLogs);
    h.open("web-2", &["web"], ShowLogs);
    h.open("web-1", &["web"], ShowLogs);

    let targets: Vec<NavTarget> = h.logs_panels().into_iter().map(|(t, _)| t).collect();
    assert_eq!(targets, [pod_logs("web-1"), pod_logs("web-2")]);
}

/// The flipped action opens the one shared panel instead - once each time,
/// and the same shared panel for another pod.
#[gpui_kit::test]
async fn the_flipped_action_reuses_the_shared_panel(cx: &mut TestAppContext) {
    let mut h = harness(cx);

    h.open("web-1", &["web"], ShowLogsFlipped);
    h.open("web-2", &["web"], ShowLogsFlipped);

    assert_eq!(h.logs_panels(), [(NavTarget::Logs, None)]);
}

/// With the reuse preference, plain logs share the one panel, and the flipped
/// action gives a pod its own.
#[gpui_kit::test]
async fn the_reuse_preference_turns_both_around(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.set_preference(LogsPanels::Reuse);

    h.open("web-1", &["web"], ShowLogs);
    h.open("web-2", &["web"], ShowLogs);
    h.open("web-3", &["web"], ShowLogsFlipped);

    let targets: Vec<NavTarget> = h.logs_panels().into_iter().map(|(t, _)| t).collect();
    assert_eq!(targets, [NavTarget::Logs, pod_logs("web-3")]);
}

/// Another container of a pod whose panel is open switches that panel to it,
/// rather than opening a second panel for the pod.
#[gpui_kit::test]
async fn another_container_switches_the_pods_panel(cx: &mut TestAppContext) {
    let mut h = harness(cx);

    h.open("web-1", &["web", "proxy"], ShowLogs);
    h.open("web-1", &["proxy", "web"], ShowLogs);

    assert_eq!(
        h.logs_panels(),
        [(
            pod_logs("web-1"),
            Some(("web-1".to_string(), Some("proxy".to_string())))
        )]
    );
}

/// A pod's own panel stays on its pod: selecting another pod, without
/// opening its logs, doesn't move it.
#[gpui_kit::test]
async fn a_pods_panel_ignores_later_selections(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.open("web-1", &["web"], ShowLogs);

    h.vcx.update(|_, cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "shop".into(),
            name: "web-2".into(),
            containers: vec!["web".into()],
            context_name: "demo".into(),
        })))
    });
    h.vcx.run_until_parked();

    assert_eq!(
        h.logs_panels(),
        [(
            pod_logs("web-1"),
            Some(("web-1".to_string(), Some("web".to_string())))
        )]
    );
}

/// A pod's own panel saves its pod and container, and the saved state reads
/// back as that pod's target; the shared panel's reads back as the shared one.
#[gpui_kit::test]
async fn a_pods_panel_saves_and_restores_as_that_pod(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::BasePanel as _;
    let mut h = harness(cx);
    h.open("web-1", &["proxy", "web"], ShowLogs);

    let main = h.main.clone();
    let saved = h.vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            panic!("a workspace");
        };
        let panel: Entity<LogsPanel> = open_panels
            .iter()
            .find_map(|open| match &open.panel {
                Some(OpenedPanel::Logs(panel)) => Some(panel.clone()),
                _ => None,
            })
            .expect("a Logs panel");
        panel.read(cx).dump(cx)
    });
    let gpui_kit::component::dock::PanelInfo::Panel(data) = &saved.info else {
        panic!("a panel's state");
    };
    assert_eq!(data["pod_name"], "web-1");
    assert_eq!(data["pod_namespace"], "shop");

    let pinned = crate::util::logs::pinned_from_state(data, "demo").expect("pinned");
    assert_eq!(
        (pinned.name.as_str(), pinned.namespace.as_str()),
        ("web-1", "shop")
    );
    let shared = serde_json::json!({ "context_name": "demo", "namespaces": [] });
    assert!(crate::util::logs::pinned_from_state(&shared, "demo").is_none());
}
