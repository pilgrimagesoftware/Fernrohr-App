//! `pod-quick-look` section 1, in a real window with the app's keymap
//! (`util::shell::init`) over a fake cluster (`k8s::test_cluster`): Space on a
//! selected pod opens the quick look, Space and Escape close it, Enter and its
//! button open the detail panel, Down moves it, and it follows the pod live.

use super::view::{OPEN_DETAILS_ID, POPOVER_ID, warning_text};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pod_detail::glance::glance;
use crate::k8s::resource::pods::{PodsPanel, SelectedPod};
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::detail::lifecycle::{BANNER_ID, Lifecycle};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, Focusable as _, Keystroke, Modifiers, TestAppContext,
    VisualTestContext,
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

const CONTEXT: &str = "quick-look";
const PODS: (&str, &str) = ("/api/v1", "pods");

fn temp_path() -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "fernrohr-quick-look-{}-{n}.toml",
        std::process::id()
    ))
}

/// `shop/<name>` with two containers: `app`, ready unless `crashing` (then
/// crash-looping after 5 restarts), and `sidecar`, always ready.
fn pod(uid: &str, name: &str, crashing: bool) -> Value {
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
fn warning(pod: &str, uid: &str, reason: &str) -> Value {
    json!({
        "apiVersion": "v1", "kind": "Event",
        "metadata": { "name": format!("{pod}.{reason}"), "namespace": "shop", "uid": format!("e-{pod}") },
        "involvedObject": { "kind": "Pod", "apiVersion": "v1", "namespace": "shop", "name": pod, "uid": uid },
        "type": "Warning", "reason": reason, "message": "Back-off restarting failed container",
        "count": 3, "lastTimestamp": jiff::Timestamp::now().to_string(),
    })
}

struct Harness {
    cluster: FakeCluster,
    panel: Entity<PodsPanel>,
    vcx: VisualTestContext,
    /// How many times `ShowPodDetail` reached the app.
    details_opened: Rc<Cell<usize>>,
}

/// A Pods panel over a fake cluster holding crash-looping `web-1` (with a
/// `BackOff` warning) and healthy `web-2`, focused, with `web-1` selected.
fn open(cx: &mut TestAppContext) -> Harness {
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
    fn press(&mut self, key: &str) {
        self.vcx
            .simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
        self.vcx.run_until_parked();
    }

    fn wait_for(&mut self, what: &str, done: impl Fn(&PodsPanel, &gpui_kit::App) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if self.vcx.update(|_, cx| done(self.panel.read(cx), cx)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }

    fn selected(&mut self) -> Option<String> {
        self.vcx.update(|_, cx| {
            cx.try_global::<SelectedPod>()
                .and_then(|selected| selected.0.clone())
                .map(|pod| pod.name)
        })
    }

    /// The pod the open quick look is over, if one is open.
    fn looking_at(&mut self) -> Option<String> {
        self.vcx.update(|_, cx| {
            let popover = self.panel.read(cx).quick_look()?;
            Some(popover.read(cx).target.name.clone())
        })
    }

    /// Whether `selector` is drawn in the window.
    fn drawn(&mut self, selector: &'static str) -> bool {
        self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(selector).is_some()
        })
    }
}

/// Spec "Opening a quick look": Space on a selected crashing pod shows its
/// phase, `1/2` ready, restarts, each container's image and state, and its
/// latest `BackOff` warning, beside the row.
#[gpui_kit::test]
async fn space_on_a_crashing_pod_shows_it_at_a_glance(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    assert!(!harness.drawn(POPOVER_ID), "nothing open yet");

    harness.press("space");
    assert_eq!(harness.looking_at().as_deref(), Some("web-1"));
    assert!(harness.drawn(POPOVER_ID), "the popover is drawn");
    harness.wait_for("listed the latest warning", |panel, cx| {
        let popover = panel.quick_look().unwrap().read(cx);
        popover.latest_warning(cx).is_some()
    });

    harness.vcx.update(|_, cx| {
        let popover = harness.panel.read(cx).quick_look().unwrap().read(cx);
        let glance = glance(popover.pod().expect("listed"), jiff::Timestamp::now());
        assert_eq!(glance.status, "Running");
        assert_eq!(glance.ready, "1/2");
        assert_eq!(glance.restarts, 5);
        assert_eq!(glance.node, "node-a");
        assert_eq!(glance.pod_ip, "10.0.0.7");
        assert_eq!(glance.owners[0].name, "web-7d9f");
        let app = &glance.containers[0];
        assert_eq!(app.image, "shop/web:1.4");
        assert!(app.state.contains("CrashLoopBackOff"), "{}", app.state);
        let warning = popover.latest_warning(cx).unwrap();
        assert!(
            warning_text(&warning, jiff::Timestamp::now()).starts_with("BackOff: Back-off"),
            "{warning:?}"
        );
    });
    harness.vcx.update(|window, cx| window.render_frame(cx));
    assert!(harness.vcx.debug_bounds("quick-look-status").is_some());
    assert!(harness.vcx.debug_bounds("quick-look-warning").is_some());
    let popover = harness.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(POPOVER_ID).expect("drawn").bounds()
    });
    let status = harness.vcx.debug_bounds("quick-look-status").unwrap();
    assert!(
        popover.contains(&status.center()),
        "the fields are laid out inside the popover, not collapsed out of it"
    );
}

/// Space and Escape close the quick look and keep the selection; Enter and the
/// Open Details button each open the pod's detail panel and close it.
#[gpui_kit::test]
async fn the_quick_look_closes_and_opens_details_from_the_keyboard_and_mouse(
    cx: &mut TestAppContext,
) {
    let mut harness = open(cx);

    harness.press("space");
    harness.press("escape");
    assert_eq!(harness.looking_at(), None, "Escape closes it");
    assert_eq!(
        harness.selected().as_deref(),
        Some("web-1"),
        "and keeps the row"
    );

    harness.press("space");
    harness.press("space");
    assert_eq!(harness.looking_at(), None, "Space closes it again");

    harness.press("space");
    harness.press("enter");
    assert_eq!(harness.looking_at(), None, "Enter closes it");
    assert_eq!(
        harness.details_opened.get(),
        1,
        "and opens the detail panel"
    );

    harness.press("space");
    assert!(harness.drawn(OPEN_DETAILS_ID));
    let button = harness.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(OPEN_DETAILS_ID).expect("drawn")
    });
    harness
        .vcx
        .simulate_click(button.bounds().center(), Modifiers::none());
    harness.vcx.run_until_parked();
    assert_eq!(harness.looking_at(), None, "the button closes it");
    assert_eq!(
        harness.details_opened.get(),
        2,
        "and opens the detail panel"
    );
}

/// Specs "Scanning several pods", "Live while open" and "The pod goes away":
/// Down moves the quick look with the selection, a ready-count change shows in
/// place, and a pod being deleted reads as Terminating, then as deleted with its
/// last state kept - the detail panels' words - with the popover still open.
#[gpui_kit::test]
async fn the_quick_look_follows_the_selection_and_the_pod_live(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.press("space");

    harness.press("down");
    assert_eq!(harness.selected().as_deref(), Some("web-2"));
    assert_eq!(
        harness.looking_at().as_deref(),
        Some("web-2"),
        "it followed"
    );

    harness
        .cluster
        .apply(PODS.0, PODS.1, pod("u2", "web-2", true));
    harness.wait_for("showed web-2's ready count drop", |panel, cx| {
        let popover = panel.quick_look().unwrap().read(cx);
        popover
            .pod()
            .is_some_and(|pod| glance(pod, jiff::Timestamp::now()).ready == "1/2")
    });

    let mut terminating = pod("u2", "web-2", true);
    let deadline = jiff::Timestamp::now() + jiff::SignedDuration::from_secs(30);
    terminating["metadata"]["deletionTimestamp"] = json!(deadline.to_string());
    harness.cluster.apply(PODS.0, PODS.1, terminating);
    harness.wait_for("showed web-2 Terminating", |panel, cx| {
        let popover = panel.quick_look().unwrap().read(cx);
        matches!(popover.lifecycle(), Some(Lifecycle::Terminating { .. }))
    });
    assert!(harness.drawn(BANNER_ID), "the banner says so");

    harness.cluster.delete(PODS.0, PODS.1, "shop", "web-2");
    harness.wait_for("showed web-2 deleted", |panel, cx| {
        let popover = panel.quick_look().unwrap().read(cx);
        matches!(popover.lifecycle(), Some(Lifecycle::Deleted { .. }))
    });
    assert_eq!(harness.looking_at().as_deref(), Some("web-2"), "still open");
    harness.vcx.update(|_, cx| {
        let popover = harness.panel.read(cx).quick_look().unwrap().read(cx);
        let last = glance(
            popover.pod().expect("its last state"),
            jiff::Timestamp::now(),
        );
        assert_eq!(last.ready, "1/2", "its last state stays");
        let message = popover
            .lifecycle()
            .unwrap()
            .message("pod", jiff::Timestamp::now());
        assert!(message.starts_with("This pod was deleted at "), "{message}");
    });
    assert!(harness.drawn(POPOVER_ID), "and still drawn");
    assert!(harness.drawn(BANNER_ID), "under the deleted banner");
}

/// Design D2: the warning watch starts with the popover, moves - after the
/// debounce - with the selection, and ends when it closes.
#[gpui_kit::test]
async fn closing_the_quick_look_releases_its_event_watch(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.press("space");
    let first = harness.vcx.update(|_, cx| {
        let popover = harness.panel.read(cx).quick_look().unwrap().read(cx);
        popover.events_table().expect("watching")
    });

    harness.press("down");
    assert!(
        first.upgrade().is_none(),
        "moving on drops the old pod's watch"
    );
    assert!(
        harness.vcx.update(|_, cx| {
            harness
                .panel
                .read(cx)
                .quick_look()
                .unwrap()
                .read(cx)
                .events_table()
                .is_none()
        }),
        "not before the selection has rested"
    );
    harness
        .vcx
        .executor()
        .advance_clock(crate::consts::QUICK_LOOK_EVENTS_DEBOUNCE);
    harness.wait_for("restarted the watch on web-2", |panel, cx| {
        panel
            .quick_look()
            .unwrap()
            .read(cx)
            .events_table()
            .is_some()
    });
    let second = harness.vcx.update(|_, cx| {
        let popover = harness.panel.read(cx).quick_look().unwrap().read(cx);
        popover.events_table().expect("watching")
    });

    harness.press("escape");
    assert!(second.upgrade().is_none(), "closing drops the watch");
}

/// Spec "Logs from the context menu": right-clicking a row selects it, and the
/// menu's Logs runs the same command as `l` on that row.
#[gpui_kit::test]
async fn logs_from_a_rows_context_menu_opens_that_pods_logs(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent};
    let mut harness = open(cx);
    let logs_opened = Rc::new(Cell::new(0));
    harness.vcx.update(|_, cx| {
        let opened = logs_opened.clone();
        cx.on_action(move |_: &crate::ui::nav::ShowLogs, _| opened.set(opened.get() + 1));
    });
    harness.vcx.update(|window, cx| window.render_frame(cx));
    let row = harness
        .vcx
        .debug_bounds("pod-cell-1-0")
        .expect("web-2's row is drawn")
        .center();

    harness.vcx.simulate_event(MouseDownEvent {
        position: row,
        button: MouseButton::Right,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    harness.vcx.simulate_event(MouseUpEvent {
        position: row,
        button: MouseButton::Right,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    harness.vcx.run_until_parked();
    assert_eq!(
        harness.selected().as_deref(),
        Some("web-2"),
        "right-click selects"
    );

    // Quick Look, Open Details, Logs, YAML: Down to the third, Enter.
    harness.press("down");
    harness.press("down");
    harness.press("down");
    harness.press("enter");
    assert_eq!(logs_opened.get(), 1, "Logs ran");
    assert_eq!(harness.selected().as_deref(), Some("web-2"), "for that pod");
}
