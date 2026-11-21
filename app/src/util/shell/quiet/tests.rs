//! #236, headless: the last window closing disconnects everything and leaves
//! the app running quiet, with the MCP server kept or stopped by the setting;
//! reopening restores the window that closed last and starts the server again;
//! a context from the status menu opens a window connected to it, where the
//! last window was and at its size (#350).

use crate::config::ui::BackgroundMcp;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{AgentId, ClusterRegistry, Holder};
use crate::util::shell::quiet::{
    Hooks, OpenContextWindow, QUIET_MODE, context_window_layout, install, is_quiet, quit_mode,
    register_handlers, reopen,
};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{
    ClosedWindowLayouts, MainWindow, WindowLayout, close_window, init, is_main_window, open_window,
};
use gpui_kit::component::Root;
use gpui_kit::{AnyWindowHandle, App, Global, QuitMode, TestAppContext};
use std::cell::Cell;

/// How often each hook ran, in this test's app.
#[derive(Default)]
struct Calls {
    starts: usize,
    stops: usize,
}

impl Global for Calls {}

thread_local! {
    static KILLS: Cell<usize> = const { Cell::new(0) };
    /// The pids the last kill was told to spare.
    static SPARED: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn hooks() -> Hooks {
    Hooks {
        start_mcp: |cx: &mut App| cx.global_mut::<Calls>().starts += 1,
        stop_mcp: |cx: &mut App| cx.global_mut::<Calls>().stops += 1,
        kill_forwards: |spare| {
            KILLS.with(|kills| kills.set(kills.get() + 1));
            SPARED.with(|spared| *spared.borrow_mut() = spare.to_vec());
        },
    }
}

/// An app with the shell and quiet mode installed, `background` as the
/// setting, and a main window connected to `demo`.
fn quiet_app(background: BackgroundMcp, cx: &mut TestAppContext) -> AnyWindowHandle {
    quiet_app_with(background, hooks(), "demo", cx, |cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    })
}

/// [`quiet_app`] with `hooks`, and a window on `context`, whose session
/// `seed` registers first.
fn quiet_app_with(
    background: BackgroundMcp,
    hooks: Hooks,
    context: &str,
    cx: &mut TestAppContext,
    seed: impl FnOnce(&mut App),
) -> AnyWindowHandle {
    cx.executor().allow_parking();
    KILLS.with(|kills| kills.set(0));
    SPARED.with(|spared| spared.borrow_mut().clear());
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        seed(cx);
        cx.set_global(Calls::default());
        crate::ui::background_mcp::set(background, cx);
        register_handlers(hooks, cx);
        install(cx);
    });
    let handle = cx.update(|cx| {
        open_window(
            cx,
            WindowLayout {
                contexts: vec![context.into()],
                ..WindowLayout::default()
            },
        )
    });
    cx.run_until_parked();
    handle
}

fn close(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update(|cx| {
        let _ = handle.update(cx, |_, window, cx| close_window(window, cx));
    });
    cx.run_until_parked();
}

fn main_windows(cx: &mut TestAppContext) -> Vec<AnyWindowHandle> {
    cx.update(|cx| {
        cx.windows()
            .into_iter()
            .filter(|handle| is_main_window(*handle, cx))
            .collect()
    })
}

/// The contexts `handle`'s main window holds.
fn contexts_of(handle: AnyWindowHandle, cx: &mut TestAppContext) -> Vec<String> {
    cx.update(|cx| {
        handle
            .downcast::<Root>()
            .and_then(|root| root.read(cx).ok())
            .and_then(|root| root.view().clone().downcast::<MainWindow>().ok())
            .map(|main| main.read(cx).contexts())
            .unwrap_or_default()
    })
}

#[test]
fn macos_quits_only_when_asked() {
    if QUIET_MODE {
        assert_eq!(quit_mode(), QuitMode::Explicit);
    } else {
        assert_eq!(quit_mode(), QuitMode::LastWindowClosed);
    }
}

#[gpui_kit::test]
async fn the_last_window_closing_disconnects_and_keeps_the_mcp_server(cx: &mut TestAppContext) {
    let window = quiet_app(BackgroundMcp::KeepRunning, cx);
    assert_eq!(
        cx.update(|cx| ClusterRegistry::window_holder_count(cx, "demo")),
        1
    );

    close(window, cx);
    assert!(cx.update(|cx| cx.windows().is_empty()), "no window is left");
    assert_eq!(
        cx.update(|cx| ClusterRegistry::window_holder_count(cx, "demo")),
        0,
        "the context is disconnected"
    );
    assert!(cx.update(|cx| is_quiet(cx)), "the app runs on, quiet");
    assert_eq!(KILLS.with(Cell::get), 1, "leftover forwards are killed");
    assert_eq!(
        cx.update(|cx| cx.global::<Calls>().stops),
        0,
        "the server keeps serving"
    );
}

/// `mcp-connections` 6.1: a context an agent holds outlives the last window,
/// connection and tunnel alike - quiet mode's sweep spares the forwards open
/// sessions use - and port-forwards still stop.
#[gpui_kit::test]
async fn an_agent_held_context_outlives_the_last_window(cx: &mut TestAppContext) {
    let window = quiet_app(BackgroundMcp::KeepRunning, cx);
    let agent = AgentId::next();
    let connection = cx.update(|cx| {
        crate::k8s::cluster::session::LiveAgents::open(cx, agent, None);
        ClusterRegistry::hold(cx, "demo", agent);
        ClusterRegistry::existing_connection(cx, "demo").expect("demo is open")
    });

    close(window, cx);

    assert!(cx.update(|cx| is_quiet(cx)), "the app runs on, quiet");
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holders(cx, "demo")),
        [Holder::Agent(agent)],
        "only the window's hold went"
    );
    assert_eq!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "demo")),
        Some(connection),
        "the same connection, not a redial"
    );
    assert_eq!(
        KILLS.with(Cell::get),
        1,
        "leftover forwards are still swept"
    );
    assert_eq!(
        SPARED.with(|spared| spared.borrow().clone()),
        cx.update(|cx| ClusterRegistry::forward_pids_in_use(cx)),
        "sparing the ones open sessions use"
    );
}

#[gpui_kit::test]
async fn with_the_setting_off_the_mcp_server_stops_and_starts_again(cx: &mut TestAppContext) {
    let window = quiet_app(BackgroundMcp::StopWithLastWindow, cx);
    close(window, cx);
    assert_eq!(
        cx.update(|cx| cx.global::<Calls>().stops),
        1,
        "stopped with the last window"
    );

    cx.update(|cx| {
        reopen(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| cx.global::<Calls>().starts),
        1,
        "started with the next one"
    );
    assert!(!cx.update(|cx| is_quiet(cx)));
}

#[gpui_kit::test]
async fn reopening_restores_the_window_that_closed_last(cx: &mut TestAppContext) {
    let window = quiet_app(BackgroundMcp::KeepRunning, cx);
    close(window, cx);

    let reopened = cx.update(reopen);
    cx.run_until_parked();
    assert_eq!(main_windows(cx).len(), 1, "one main window");
    assert_eq!(contexts_of(reopened, cx), ["demo"], "connected as it was");
    assert_eq!(
        cx.update(|cx| ClusterRegistry::window_holder_count(cx, "demo")),
        1
    );
    let remembered = cx.update(|cx| {
        cx.try_global::<ClosedWindowLayouts>()
            .map_or(0, |closed| closed.0.len())
    });
    assert_eq!(remembered, 0, "the closed layout is spent");

    // With nothing remembered, reopening opens a default window.
    close(reopened, cx);
    cx.update(|cx| cx.global_mut::<ClosedWindowLayouts>().0.clear());
    let fresh = cx.update(reopen);
    cx.run_until_parked();
    assert!(contexts_of(fresh, cx).is_empty(), "the cluster picker");
}

#[gpui_kit::test]
async fn a_context_from_the_status_menu_opens_a_window_connected_to_it(cx: &mut TestAppContext) {
    let window = quiet_app(BackgroundMcp::KeepRunning, cx);
    close(window, cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connecting);
        cx.dispatch_action(&OpenContextWindow {
            context: "staging".into(),
        });
    });
    cx.run_until_parked();
    let windows = main_windows(cx);
    assert_eq!(windows.len(), 1);
    assert_eq!(contexts_of(windows[0], cx), ["staging"]);
    assert!(!cx.update(|cx| is_quiet(cx)));
}

/// Decision (4): an agent's action needing approval with no window open
/// reopens the window that closed last, and asks there.
#[gpui_kit::test]
async fn an_approval_with_no_window_reopens_one_to_ask_in(cx: &mut TestAppContext) {
    use crate::mcp::approval::{ApprovalRequest, Asking};
    use gpui_kit::component::WindowExt as _;
    let window = quiet_app(BackgroundMcp::KeepRunning, cx);
    close(window, cx);
    let (reply, _answer) = tokio::sync::oneshot::channel();
    let question = cx.update(|cx| {
        crate::ui::agent_approval::open(
            ApprovalRequest {
                title: "Scale Deployment?".into(),
                confirm: "Scale".into(),
                tool: "scale_workload".into(),
                context: "demo".into(),
                namespace: "team-a".into(),
                kind: "Deployment (apps)".into(),
                targets: vec!["web".into()],
                parameters: vec![("Replicas".into(), "2 → 3".into())],
                asking: Asking::Action {
                    irreversible: false,
                },
            },
            reply,
            cx,
        )
    });
    assert!(question.is_some(), "the question is asked");
    cx.run_until_parked();
    let windows = main_windows(cx);
    assert_eq!(windows.len(), 1, "in a reopened main window");
    assert_eq!(contexts_of(windows[0], cx), ["demo"]);
    let asking = cx.update(|cx| {
        windows[0]
            .update(cx, |_, window, cx| window.has_active_dialog(cx))
            .unwrap_or(false)
    });
    assert!(asking, "with its dialog open");
}

/// #350: a context from the status menu opens at the size and position of the
/// window that closed last, not the default - and only on that context.
#[gpui_kit::test]
async fn a_status_menu_window_takes_the_last_closed_geometry(cx: &mut TestAppContext) {
    let window = quiet_app(BackgroundMcp::KeepRunning, cx);
    close(window, cx);
    cx.update(|cx| {
        for layout in cx.global_mut::<ClosedWindowLayouts>().0.values_mut() {
            layout.width = 1400.0;
            layout.height = 900.0;
            layout.x = Some(40.0);
            layout.y = Some(60.0);
        }
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connecting);
        cx.dispatch_action(&OpenContextWindow {
            context: "staging".into(),
        });
    });
    cx.run_until_parked();
    let windows = main_windows(cx);
    assert_eq!(windows.len(), 1);
    assert_eq!(
        contexts_of(windows[0], cx),
        ["staging"],
        "only the chosen context"
    );
    let size = cx.update(|cx| {
        windows[0]
            .update(cx, |_, window, _| window.viewport_size())
            .expect("the window is open")
    });
    assert_eq!(
        (f32::from(size.width), f32::from(size.height)),
        (1400.0, 900.0),
        "the last window's size"
    );
}

#[test]
fn a_context_window_layout_keeps_geometry_not_contexts() {
    let last = WindowLayout {
        width: 1400.0,
        height: 900.0,
        x: Some(40.0),
        y: Some(60.0),
        contexts: vec!["demo".into()],
        resource_panel_width: Some(300.0),
        ..WindowLayout::default()
    };
    let layout = context_window_layout("staging", Some(last));
    assert_eq!(layout.contexts, ["staging"]);
    assert!(layout.panels.is_empty());
    assert_eq!((layout.width, layout.height), (1400.0, 900.0));
    assert_eq!((layout.x, layout.y), (Some(40.0), Some(60.0)));
    assert_eq!(layout.resource_panel_width, Some(300.0));

    let fresh = context_window_layout("staging", None);
    assert_eq!(
        (fresh.width, fresh.height, fresh.x),
        (
            WindowLayout::default().width,
            WindowLayout::default().height,
            None
        ),
        "nothing closed yet: the default size, centred"
    );
}

/// [`hooks`], with a `stop_mcp` that ends the live agents as the real one
/// (`mcp::server::stop`) does: their holds linger for
/// [`MCP_SESSION_GRACE`](crate::consts::MCP_SESSION_GRACE), then go.
fn hooks_ending_the_agents_session() -> Hooks {
    Hooks {
        stop_mcp: |cx: &mut App| {
            cx.global_mut::<Calls>().stops += 1;
            crate::k8s::cluster::session::LiveAgents::end_all(cx, crate::consts::MCP_SESSION_GRACE);
        },
        ..hooks()
    }
}

/// A kubeconfig with only `context`, on `server`.
fn kubeconfig(context: &str, server: &str) -> std::path::PathBuf {
    let path = crate::util::test_paths::temp_path("quiet-kubeconfig");
    let yaml = format!(
        "apiVersion: v1\nkind: Config\nclusters:\n  - name: {context}\n    cluster:\n      server: {server}\ncontexts:\n  - name: {context}\n    context:\n      cluster: {context}\n      user: {context}\nusers:\n  - name: {context}\n    user: {{}}\n"
    );
    std::fs::write(&path, yaml).unwrap();
    path
}

/// `mcp-connections` 6.1 with the setting off: closing the last window stops
/// the MCP server, which ends the agent's session; its hold lingers for the
/// grace period, then goes, and the context's tunnel forward goes with it.
#[gpui_kit::test]
async fn stopping_with_the_last_window_ends_the_agents_hold_and_tunnel(cx: &mut TestAppContext) {
    use crate::config::tunnels::{TunnelAuth, TunnelConfig};
    use crate::k8s::cluster::connection::ClusterConnection;
    use crate::k8s::cluster::tunnel::{self, ForwardKey};
    use gpui_kit::AppContext as _;

    let tunnels = crate::util::test_paths::temp_path("quiet-tunnels");
    let kubeconfig = kubeconfig("kind-dev", "https://10.0.0.1:6443");
    let store = crate::tunnel::store::TunnelStore::new(tunnels.clone());
    let bastion = TunnelConfig {
        name: "test".to_string(),
        bastion_user: "deploy".to_string(),
        bastion_host: "bastion.example.invalid".to_string(),
        bastion_port: 22,
        jump_hosts: Vec::new(),
        auth: TunnelAuth::default(),
        ..Default::default()
    };
    store.create("test-bastion", bastion, None).unwrap();
    store.bind("kind-dev", "test-bastion").unwrap();
    let key = ForwardKey::Ssh {
        tunnel_id: "test-bastion".to_string(),
        host: "10.0.0.1".to_string(),
        port: 6443,
    };
    let agent = AgentId::next();

    let window = quiet_app_with(
        BackgroundMcp::StopWithLastWindow,
        hooks_ending_the_agents_session(),
        "kind-dev",
        cx,
        |cx| {
            let forward = tunnel::acquire_for_context(cx, &tunnels, Some(&kubeconfig), "kind-dev")
                .unwrap()
                .expect("kind-dev is bound to test-bastion");
            let handle = crate::runtime::handle(cx);
            let client = {
                let _guard = handle.enter();
                kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap()))
                    .unwrap()
            };
            let connection = cx.new(|_| {
                ClusterConnection::test_with_state_and_forward(
                    ConnectionState::Connected(client),
                    forward,
                )
            });
            ClusterRegistry::insert_test_session_with_connection(cx, "kind-dev", connection);
            crate::k8s::cluster::session::LiveAgents::open(cx, agent, None);
            ClusterRegistry::hold(cx, "kind-dev", agent);
        },
    );
    let live = cx.update(tunnel::live_forward_keys);
    assert!(live.borrow().contains(&key));

    close(window, cx);
    assert_eq!(cx.update(|cx| cx.global::<Calls>().stops), 1);
    assert!(
        live.borrow().contains(&key),
        "the agent's hold lingers through the grace period"
    );

    cx.executor()
        .advance_clock(crate::consts::MCP_SESSION_GRACE + std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "kind-dev"))
            .is_none(),
        "the hold went, and the session with it"
    );
    assert!(!live.borrow().contains(&key), "its tunnel went too");

    let _ = std::fs::remove_file(&tunnels);
    let _ = std::fs::remove_file(&kubeconfig);
}
