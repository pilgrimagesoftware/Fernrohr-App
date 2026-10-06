//! `k9s-remaining-keybindings` section 5, in a window with the panel's key: `p`
//! switches the log request to the container's previous instance and back,
//! against a log server that answers either - or, for a container that never
//! restarted, refuses previous logs as the API does.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::keymap::KeymapConfig;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::util::logs::LogsPanel;
use crate::util::logs::view::NO_PREVIOUS_INSTANCE;
use gpui_kit::{Entity, Focusable as _, TestAppContext, VisualTestContext};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The fake log endpoint: the current instance's lines (growing as the test
/// says), the previous instance's, whether there is one, and every query seen.
#[derive(Clone, Default)]
struct LogServer {
    current: Arc<Mutex<Vec<String>>>,
    restarted: Arc<AtomicBool>,
    queries: Arc<Mutex<Vec<String>>>,
}

impl LogServer {
    async fn serve(self) -> std::net::SocketAddr {
        use tokio::io::AsyncWriteExt as _;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let server = self.clone();
                tokio::spawn(async move {
                    let request = crate::k8s::test_recorder::read_request(&mut stream).await;
                    let query = request
                        .target
                        .split_once('?')
                        .map(|(_, q)| q.to_string())
                        .unwrap_or_default();
                    server.queries.lock().push(query.clone());
                    let (status, body) = if query.contains("previous=true") {
                        if server.restarted.load(Ordering::SeqCst) {
                            ("200 OK", "old 1\nold 2\n".to_string())
                        } else {
                            let status = serde_json::json!({ "kind": "Status", "apiVersion": "v1",
                                "status": "Failure", "reason": "BadRequest", "code": 400,
                                "message": "previous terminated container \"app\" in pod \"web-1\" not found" });
                            ("400 Bad Request", status.to_string())
                        }
                    } else {
                        let lines = server.current.lock().clone();
                        (
                            "200 OK",
                            lines.iter().map(|line| format!("{line}\n")).collect(),
                        )
                    };
                    let content_type = if status.starts_with("200") {
                        "text/plain"
                    } else {
                        "application/json"
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        addr
    }
}

struct Harness {
    panel: Entity<LogsPanel>,
    vcx: VisualTestContext,
    server: LogServer,
}

fn open(cx: &mut TestAppContext, restarted: bool) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::util::logs::register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let server = LogServer::default();
    server.restarted.store(restarted, Ordering::SeqCst);
    *server.current.lock() = vec!["current 1".into()];
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let addr = handle.block_on(server.clone().serve());
    let client = {
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
            .unwrap()
    };
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connected(client));
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "shop".into(),
            name: "web-1".into(),
            containers: vec!["app".into()],
            context_name: "demo".into(),
        })));
    });
    let (panel, vcx) = cx.add_window_view(|_window, cx| {
        LogsPanel::new(PanelScope::new(NavTarget::Logs, "demo".to_string()), cx)
    });
    let mut vcx = vcx.clone();
    vcx.update(|window, cx| {
        window.activate_window();
        panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    let mut harness = Harness { panel, vcx, server };
    harness.wait_for_lines(&["current 1"]);
    harness
}

impl Harness {
    fn lines(&mut self) -> Vec<String> {
        self.vcx
            .update(|_, cx| self.panel.read(cx).view.read(cx).lines().to_vec())
    }

    fn wait_for_lines(&mut self, expected: &[&str]) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if self.lines() == expected {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never showed {expected:?}, showed {:?}", self.lines());
    }

    fn press_p(&mut self) {
        self.vcx.simulate_keystrokes("p");
        self.vcx.run_until_parked();
    }
}

/// 5.1 and 5.3: `p` asks for the previous instance's logs (`previous=true`,
/// not followed) and shows them; `p` again resumes the current logs - lines
/// written meanwhile included.
#[gpui_kit::test]
async fn previous_logs_toggle_on_and_back_to_current(cx: &mut TestAppContext) {
    let mut h = open(cx, true);
    assert!(!h.server.queries.lock()[0].contains("previous=true"));

    h.press_p();
    h.wait_for_lines(&["old 1", "old 2"]);
    let query = h.server.queries.lock().last().cloned().unwrap();
    assert!(query.contains("previous=true"), "{query}");
    assert!(
        !query.contains("follow=true"),
        "a finished log isn't followed: {query}"
    );
    assert!(h.vcx.update(|_, cx| h.panel.read(cx).previous));

    h.server.current.lock().push("current 2".into());
    h.press_p();
    h.wait_for_lines(&["current 1", "current 2"]);
    let query = h.server.queries.lock().last().cloned().unwrap();
    assert!(!query.contains("previous=true"), "{query}");
    assert!(
        query.contains("follow=true"),
        "current logs are followed: {query}"
    );
}

/// 5.2: a container that never restarted has no previous instance - the panel
/// says so, rather than showing nothing.
#[gpui_kit::test]
async fn no_previous_instance_says_so(cx: &mut TestAppContext) {
    let mut h = open(cx, false);

    h.press_p();
    for _ in 0..400 {
        h.vcx.run_until_parked();
        let shown = h.vcx.update(|_, cx| {
            h.panel
                .read(cx)
                .view
                .read(cx)
                .terminal_message()
                .map(str::to_string)
        });
        if shown.as_deref() == Some(NO_PREVIOUS_INSTANCE) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("never said there is no previous instance");
}
