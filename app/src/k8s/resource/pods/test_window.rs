//! A Pods panel in a real window with the app's keymap (`util::shell::init`)
//! over a fake cluster (`k8s::test_cluster`), shared by the panel's window
//! tests - the quick look's and the row actions'.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::{PodsPanel, SelectedPod};
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, Focusable as _, Keystroke, TestAppContext, VisualTestContext,
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(in crate::k8s::resource::pods) const CONTEXT: &str = "quick-look";
pub(in crate::k8s::resource::pods) const PODS: (&str, &str) = ("/api/v1", "pods");

pub(in crate::k8s::resource::pods) fn temp_path() -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "fernrohr-quick-look-{}-{n}.toml",
        std::process::id()
    ))
}

/// `shop/<name>` with two containers: `app`, ready unless `crashing` (then
/// crash-looping after 5 restarts), and `sidecar`, always ready.
pub(in crate::k8s::resource::pods) fn pod(uid: &str, name: &str, crashing: bool) -> Value {
    let app_state = if crashing {
        json!({ "waiting": { "reason": "CrashLoopBackOff" } })
    } else {
        json!({ "running": {} })
    };
    json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": {
            "name": name, "namespace": "shop", "uid": uid,
            "ownerReferences": [{
                "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "web-7d9f",
                "uid": "rs-1", "controller": true,
            }],
        },
        "spec": {
            "nodeName": "node-a",
            "containers": [
                { "name": "app", "image": "shop/web:1.4" },
                { "name": "sidecar", "image": "envoy:1.30" },
            ],
        },
        "status": {
            "phase": "Running", "podIP": "10.0.0.7",
            "containerStatuses": [
                {
                    "name": "app", "image": "shop/web:1.4", "imageID": "",
                    "ready": !crashing, "restartCount": if crashing { 5 } else { 0 },
                    "state": app_state,
                },
                {
                    "name": "sidecar", "image": "envoy:1.30", "imageID": "",
                    "ready": true, "restartCount": 0, "state": { "running": {} },
                },
            ],
        },
    })
}

/// A Warning about `shop/<pod>` seen just now.
pub(in crate::k8s::resource::pods) fn warning(pod: &str, uid: &str, reason: &str) -> Value {
    json!({
        "apiVersion": "v1", "kind": "Event",
        "metadata": { "name": format!("{pod}.{reason}"), "namespace": "shop", "uid": format!("e-{pod}") },
        "involvedObject": { "kind": "Pod", "apiVersion": "v1", "namespace": "shop", "name": pod, "uid": uid },
        "type": "Warning", "reason": reason, "message": "Back-off restarting failed container",
        "count": 3, "lastTimestamp": jiff::Timestamp::now().to_string(),
    })
}

pub(in crate::k8s::resource::pods) struct Harness {
    pub(in crate::k8s::resource::pods) cluster: FakeCluster,
    pub(in crate::k8s::resource::pods) panel: Entity<PodsPanel>,
    pub(in crate::k8s::resource::pods) vcx: VisualTestContext,
    /// How many times `ShowPodDetail` reached the app.
    pub(in crate::k8s::resource::pods) details_opened: Rc<Cell<usize>>,
}

/// A Pods panel over a fake cluster holding crash-looping `web-1` (with a
/// `BackOff` warning) and healthy `web-2`, focused, with `web-1` selected.
pub(in crate::k8s::resource::pods) fn open(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    let details_opened = Rc::new(Cell::new(0));
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, temp_path(), &temp_path());
        let opened = details_opened.clone();
        cx.on_action(move |_: &crate::ui::nav::ShowPodDetail, _| opened.set(opened.get() + 1));
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(PODS.0, PODS.1, pod("u1", "web-1", true));
    cluster.apply(PODS.0, PODS.1, pod("u2", "web-2", false));
    cluster.apply("/api/v1", "events", warning("web-1", "u1", "BackOff"));
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            CONTEXT,
            ConnectionState::Connected(client.clone()),
        )
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let connection =
                cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
            let namespaces = cx.new(|_| NamespaceList::empty());
            PodsPanel::with_connection(
                PanelScope::new(NavTarget::pods(), CONTEXT.to_string()),
                connection,
                namespaces,
                cx,
            )
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    let mut harness = Harness {
        cluster,
        panel,
        vcx,
        details_opened,
    };
    harness.wait_for("listed both pods", |panel, cx| {
        panel.table.read(cx).pods().len() == 2
    });
    harness.press("down");
    assert_eq!(harness.selected().as_deref(), Some("web-1"));
    harness
}

impl Harness {
    pub(in crate::k8s::resource::pods) fn press(&mut self, key: &str) {
        self.vcx
            .simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
        self.vcx.run_until_parked();
    }

    pub(in crate::k8s::resource::pods) fn wait_for(
        &mut self,
        what: &str,
        done: impl Fn(&PodsPanel, &gpui_kit::App) -> bool,
    ) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if self.vcx.update(|_, cx| done(self.panel.read(cx), cx)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }

    pub(in crate::k8s::resource::pods) fn selected(&mut self) -> Option<String> {
        self.vcx.update(|_, cx| {
            cx.try_global::<SelectedPod>()
                .and_then(|selected| selected.0.clone())
                .map(|pod| pod.name)
        })
    }

    /// The pod the open quick look is over, if one is open.
    pub(in crate::k8s::resource::pods) fn looking_at(&mut self) -> Option<String> {
        self.vcx.update(|_, cx| {
            let popover = self.panel.read(cx).quick_look()?;
            Some(popover.read(cx).target.name.clone())
        })
    }

    /// Whether `selector` is drawn in the window.
    pub(in crate::k8s::resource::pods) fn drawn(&mut self, selector: &'static str) -> bool {
        self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(selector).is_some()
        })
    }
}
