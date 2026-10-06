//! The Configuration tab's fixture: an API server serving one pod, the
//! ConfigMap and Secret it references, and its events, and a connected pod
//! detail panel over it - shared by the reveal and collapse tests.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::pod_detail::configuration::{CardContents, Reveal, reveal_button_id};
use crate::k8s::resource::pod_detail::model::{DetailSection, DetailView};
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::k8s::resource::pod_detail::register_commands;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ElementId, Entity, TestAppContext, VisualTestContext, WindowHandle,
};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A ConfigMap value over 100 characters on one line.
pub(super) const BANNER: &str = "Welcome to the staging cluster - deploys freeze at 17:00, page the on-call channel before any rollback.";
/// A ConfigMap value of exactly 100 characters on one line - not large.
pub(super) const MOTD: &str = "Maintenance window: Sundays 02:00-04:00 UTC. Expect brief API restarts and clients reconnecting.....";
/// A ConfigMap value spanning several lines.
pub(super) const CONFIG_YAML: &str = "server:\n  port: 8080\n";

/// Over 100 characters: a ConfigMap value this long would collapse.
pub(super) const PASSWORD: &str = "hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-hunter2-";
pub(super) const USERNAME: &str = "other-value";

/// Serves the pod (mounting Secret `db`, reading ConfigMap `app-env` through
/// `envFrom`), its events, and both objects - counting reads of the latter.
pub(super) async fn serve(config_reads: Arc<AtomicUsize>) -> std::net::SocketAddr {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let config_reads = config_reads.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let line = request.lines().next().unwrap_or_default().to_string();
                let body = if line.contains("/events?") {
                    json!({ "apiVersion": "v1", "kind": "EventList", "metadata": {}, "items": [] })
                } else if line.contains("/pods/api-7d9f-ftg5t ") {
                    json!({
                        "apiVersion": "v1", "kind": "Pod",
                        "metadata": { "name": "api-7d9f-ftg5t", "namespace": "staging" },
                        "spec": {
                            "containers": [{
                                "name": "api",
                                "volumeMounts": [{ "name": "creds", "mountPath": "/etc/creds" }],
                                "envFrom": [{ "configMapRef": { "name": "app-env" } }],
                            }],
                            "volumes": [{ "name": "creds", "secret": { "secretName": "db" } }],
                        },
                    })
                } else if line.contains("/configmaps/app-env ") {
                    config_reads.fetch_add(1, Ordering::SeqCst);
                    json!({ "apiVersion": "v1", "kind": "ConfigMap",
                            "metadata": { "name": "app-env", "namespace": "staging" },
                            "data": {
                                "BANNER": BANNER,
                                "LOG_LEVEL": "debug",
                                "MOTD": MOTD,
                                "config.yaml": CONFIG_YAML,
                            } })
                } else if line.contains("/secrets/db ") {
                    config_reads.fetch_add(1, Ordering::SeqCst);
                    json!({ "apiVersion": "v1", "kind": "Secret",
                            "metadata": { "name": "db", "namespace": "staging" },
                            "type": "Opaque",
                            "data": { "password": "aHVudGVyMi1odW50ZXIyLWh1bnRlcjItaHVudGVyMi1odW50ZXIyLWh1bnRlcjItaHVudGVyMi1odW50ZXIyLWh1bnRlcjItaHVudGVyMi1odW50ZXIyLWh1bnRlcjItaHVudGVyMi1odW50ZXIyLWh1bnRlcjIt", "username": "b3RoZXItdmFsdWU=" } })
                } else {
                    json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
                            "reason": "NotFound", "code": 404, "message": "not found" })
                };
                let status = if body["kind"] == "Status" {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                let body = body.to_string();
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    addr
}

pub(super) struct Harness {
    pub(super) window: WindowHandle<Root>,
    pub(super) panel: Entity<PodDetailPanel>,
    pub(super) config_reads: Arc<AtomicUsize>,
}

/// A connected pod detail panel over the fixture server, inside a `Root` (Tab
/// navigation is the root's), with the registry's real bindings.
pub(super) fn harness(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        crate::ui::link::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let config_reads = Arc::new(AtomicUsize::new(0));
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let addr = handle.block_on(serve(config_reads.clone()));
    let client = {
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
            .unwrap()
    };
    let connection = cx.update(|cx| {
        cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)))
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let pod = PodRef {
            namespace: "staging".into(),
            name: "api-7d9f-ftg5t".into(),
        };
        let scope = PanelScope::new(
            NavTarget::pod("staging", "api-7d9f-ftg5t"),
            "kind-dev".into(),
        );
        let panel = cx.new(|cx| {
            PodDetailPanel::with_connection(pod, scope, DetailView::Structured, connection, cx)
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    Harness {
        window,
        panel: built.expect("the window built its panel"),
        config_reads,
    }
}

/// Runs the executor until `done` holds - the fixture server answers on a
/// tokio thread, so results arrive in real time.
pub(super) fn wait_for(
    vcx: &mut VisualTestContext,
    panel: &Entity<PodDetailPanel>,
    done: impl Fn(&PodDetailPanel) -> bool,
) {
    for _ in 0..400 {
        vcx.run_until_parked();
        if vcx.update(|_, cx| done(panel.read(cx))) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let state = vcx.update(|_, cx| {
        let panel = panel.read(cx);
        format!("tab {:?}, {:?}", panel.active_tab(), panel.configuration)
    });
    panic!("timed out waiting for the panel: {state}");
}

pub(super) fn db() -> ObjectRef {
    ObjectRef::core("Secret", "staging", "db")
}

pub(super) fn shown(panel: &PodDetailPanel) -> bool {
    matches!(
        panel.configuration.reveal_of(&db(), "password"),
        Some(Reveal::Shown(_))
    )
}

/// Tabs to the password's reveal button, then presses Space on it.
pub(super) fn reveal_password_by_keyboard(vcx: &mut VisualTestContext, h: &Harness) {
    press_by_keyboard(vcx, h, reveal_button_id(0, 0));
}

/// Tabs until `button` has focus, then presses Space on it.
pub(super) fn press_by_keyboard(vcx: &mut VisualTestContext, h: &Harness, button: ElementId) {
    for _ in 0..20 {
        let focused = vcx
            .update_window(h.window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.try_find(button.clone()).and_then(|b| b.focused())
            })
            .unwrap();
        if focused == Some(true) {
            // A key down *and* up, as a real press is: `Button` fires its
            // keyboard click on the release.
            let space = gpui_kit::Keystroke::parse("space").unwrap();
            vcx.simulate_event(gpui_kit::KeyDownEvent {
                keystroke: space.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
            vcx.run_until_parked();
            return;
        }
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
    }
    panic!("Tab never reached {button:?}");
}

/// Focuses the panel itself, so its own keys (`1`-`6`, `h`) reach it.
pub(super) fn focus_panel(vcx: &mut VisualTestContext, h: &Harness) {
    let panel = h.panel.clone();
    h.window
        .update(vcx, |_, window, cx| {
            panel.read(cx).focus_handle.clone().focus(window, cx)
        })
        .unwrap();
}

/// Opens the Configuration tab with `3` and waits for every card to load.
pub(super) fn open_configuration(vcx: &mut VisualTestContext, h: &Harness) {
    wait_for(vcx, &h.panel, |panel| panel.pod().is_some());
    focus_panel(vcx, h);
    vcx.simulate_keystrokes("3");
    wait_for(vcx, &h.panel, |panel| {
        panel.active_tab() == DetailSection::Configuration
            && panel.configuration.cards.len() == 2
            && panel
                .configuration
                .cards
                .values()
                .all(|card| !matches!(card, CardContents::Loading))
    });
}
