//! Running an entry through a real window over a fake cluster
//! (`exec-command-library` 2.3): one matching pod runs at once; several open the
//! chooser, where Enter runs the first listed and Escape opens nothing; no
//! Running pod, or a missing container, is reported and opens nothing. The
//! connected-context chooser picks by the same keys.

use super::{Place, Reported, run};
use crate::config::exec_library::{Entry, Target, WorkloadKind};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::exec::ExecTarget;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::util::shell::MainWindow;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

fn pod(name: &str, phase: &str, app: &str) -> Value {
    json!({ "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": name, "namespace": "shop", "uid": name, "labels": { "app": app } },
        "spec": { "containers": [{ "name": "app" }] },
        "status": { "phase": phase } })
}

struct Harness {
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
}

/// A workspace on `demo` over a fake cluster with three Running `web` pods (in
/// name order web-1..web-3, applied out of order), a Pending `db` pod, and a
/// Deployment selecting `app=web`.
fn open(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(
            cx,
            crate::util::test_paths::temp_path("exec-run-workspace"),
            &crate::util::test_paths::temp_path("exec-run-keymap"),
        );
    });
    let (cluster, client) = FakeCluster::start(cx);
    for (name, phase, app) in [
        ("web-3", "Running", "web"),
        ("web-1", "Running", "web"),
        ("web-2", "Running", "web"),
        ("db-0", "Pending", "db"),
        ("api-1", "Running", "api"),
    ] {
        cluster.apply("/api/v1", "pods", pod(name, phase, app));
    }
    cluster.apply(
        "/apis/apps/v1",
        "deployments",
        json!({ "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": { "name": "web", "namespace": "shop", "uid": "deploy-web" },
            "spec": { "selector": { "matchLabels": { "app": "web" } } } }),
    );
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connected(client));
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("built");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    Harness { main, vcx }
}

impl Harness {
    fn run(&mut self, entry: Entry, place: Place) {
        self.vcx
            .update(|window, cx| run(entry, "demo".into(), place, window, cx));
        self.settle();
    }

    fn settle(&mut self) {
        for _ in 0..20 {
            self.vcx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn press(&mut self, key: &str) {
        self.vcx.simulate_keystrokes(key);
        self.settle();
    }

    fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }

    /// The command sessions the window has open.
    fn sessions(&mut self) -> Vec<ExecTarget> {
        self.vcx.update(|_, cx| {
            self.main
                .read(cx)
                .test_open_targets()
                .into_iter()
                .filter_map(|target| match target {
                    NavTarget::Exec(exec) => Some(exec),
                    _ => None,
                })
                .collect()
        })
    }

    fn reported(&mut self) -> Vec<(String, String)> {
        self.vcx.update(|_, cx| {
            cx.try_global::<Reported>()
                .map(|r| r.0.clone())
                .unwrap_or_default()
        })
    }
}

fn entry(target: Target, container: Option<&str>) -> Entry {
    Entry {
        name: "Stream info".into(),
        command: "nats stream info X".into(),
        container: container.map(str::to_string),
        target,
    }
}

fn web_deployment() -> Target {
    Target::Workload {
        namespace: "shop".into(),
        workload_kind: WorkloadKind::Deployment,
        name: "web".into(),
    }
}

fn selector(selector: &str) -> Target {
    Target::Selector {
        namespace: Some("shop".into()),
        selector: selector.into(),
    }
}

/// Spec "Several matching pods": the chooser lists the three, first
/// preselected; Enter runs the entry's command in web-1.
#[gpui_kit::test]
async fn enter_in_the_chooser_runs_the_first_listed_pod(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.run(entry(web_deployment(), None), Place::Library);
    assert!(harness.dialog_open(), "three pods: it asks which");
    assert!(harness.sessions().is_empty(), "nothing opened yet");

    harness.press("enter");
    let sessions = harness.sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pod, "web-1", "the first in name order");
    assert_eq!(sessions[0].container, "app");
    let command = sessions[0].command.clone().expect("a library command");
    assert_eq!(command.entry_name, "Stream info");
    assert_eq!(command.argv, ["nats", "stream", "info", "X"]);
}

#[gpui_kit::test]
async fn escape_in_the_chooser_opens_nothing(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.run(entry(web_deployment(), None), Place::Library);
    harness.press("escape");
    assert!(!harness.dialog_open());
    assert!(harness.sessions().is_empty());
}

/// Spec "One matching pod": no chooser.
#[gpui_kit::test]
async fn one_matching_pod_runs_without_asking(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.run(entry(selector("app=api"), None), Place::Library);
    assert!(!harness.dialog_open(), "no chooser for one pod");
    let sessions = harness.sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pod, "api-1");
}

/// Spec "No matching pod": only a Pending pod matches - reported naming the
/// entry and its scope, and no panel opens.
#[gpui_kit::test]
async fn no_running_pod_is_reported_with_the_scope(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.run(entry(selector("app=db"), None), Place::Library);
    assert!(harness.sessions().is_empty());
    let reported = harness.reported();
    assert_eq!(reported.len(), 1);
    assert!(reported[0].0.contains("Stream info"), "{reported:?}");
    assert!(
        reported[0].1.contains("namespace shop, selector app=db"),
        "{reported:?}"
    );
}

/// Spec "Missing container": reported naming the container and pod.
#[gpui_kit::test]
async fn a_missing_container_is_reported_and_opens_nothing(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.run(entry(selector("app=api"), Some("tools")), Place::Library);
    assert!(harness.sessions().is_empty());
    let reported = harness.reported();
    assert!(
        reported[0].1.contains("tools") && reported[0].1.contains("shop/api-1"),
        "{reported:?}"
    );
}

/// An any-pod entry run from a pod runs in that pod.
#[gpui_kit::test]
async fn an_any_pod_entry_runs_in_the_pod_it_is_run_from(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    let from: k8s_openapi::api::core::v1::Pod =
        serde_json::from_value(pod("web-2", "Running", "web")).unwrap();
    harness.run(entry(Target::Any, None), Place::Pod(Box::new(from)));
    let sessions = harness.sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pod, "web-2");
}

/// The connected-context chooser picks by the same keys: Enter, the first.
#[gpui_kit::test]
async fn the_context_chooser_picks_the_first_by_enter(cx: &mut TestAppContext) {
    use super::super::chooser::{Choice, open as choose};
    use std::cell::RefCell;
    use std::rc::Rc;
    let mut harness = open(cx);
    let picked = Rc::new(RefCell::new(None));
    let choices = ["demo", "staging"]
        .map(|name| Choice {
            label: name.into(),
            detail: None,
        })
        .to_vec();
    {
        let picked = picked.clone();
        harness.vcx.update(|window, cx| {
            choose(
                "Run in Which Context?",
                choices,
                move |index, _, _| *picked.borrow_mut() = Some(index),
                window,
                cx,
            )
        });
    }
    harness.settle();
    harness.press("enter");
    assert_eq!(*picked.borrow(), Some(0));
}

/// Run on a pod that isn't Running: the error names that pod, not the entry's
/// scope, and nothing opens.
#[gpui_kit::test]
async fn a_pod_that_isnt_running_is_named_in_the_error(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    let from: k8s_openapi::api::core::v1::Pod =
        serde_json::from_value(pod("db-0", "Pending", "db")).unwrap();
    harness.run(entry(Target::Any, None), Place::Pod(Box::new(from)));
    assert!(harness.sessions().is_empty());
    let reported = harness.reported();
    assert_eq!(reported.len(), 1);
    assert_eq!(reported[0].1, "Pod shop/db-0 is not Running.");
}
