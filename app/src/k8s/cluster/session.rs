use super::connection::{ClusterConnection, ConnectionState};
use super::context_health::ContextHealth;
use super::health::{self, HealthTransition};
use super::watch_registry::{PauseReason, WatchRegistry};
use crate::forward::managed::ForwardState;
use crate::k8s::resource::pods::{PodsTable, watch_all_namespaces};
use gpui_kit::{App, AppContext as _, Entity, Global, WindowId};
use kube::Client;
use std::collections::{HashMap, HashSet};

/// Per-context cluster state: the connection plus the shared, refcounted watch per
/// resource kind, so two panels showing the same resource kind for the same cluster
/// see the same data off one underlying `kube_runtime` stream.
struct ClusterSession {
    connection: Entity<ClusterConnection>,
    pods: Entity<PodsTable>,
    pods_watch: Option<gpui_kit::Task<()>>,
    /// The client last used to start the Pods watch - kept so section 7.2's
    /// `ConnectionHealth` can restart it on resume without a panel re-subscribing.
    pods_client: Option<Client>,
    watchers: WatchRegistry<&'static str>,
    // Kept alive for as long as the session exists; aborts on drop like every other
    // owned background task. `None` for an unbound context, which has no forward to watch.
    _health: Option<gpui_kit::Task<()>>,
    /// `window-context-bar` design.md decision 2: which windows currently use this
    /// context. The session (and so its connection and tunnel forward) lives exactly
    /// as long as this set is non-empty - see [`ClusterRegistry::hold`]/[`release`](
    /// ClusterRegistry::release).
    holders: HashSet<WindowId>,
}

/// App-scoped (not per-window) cluster state, keyed by context name so two windows
/// (or two panels in the same window) connected to different contexts each get their
/// own connection and watches, while both connected to the same context share one.
#[derive(Default)]
pub struct ClusterRegistry {
    sessions: HashMap<String, ClusterSession>,
}

impl Global for ClusterRegistry {}

impl ClusterRegistry {
    fn ensure_init(cx: &mut App, context_name: &str) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        if cx.global::<Self>().sessions.contains_key(context_name) {
            return;
        }
        let connection = ClusterConnection::connect(cx, Some(context_name.to_string()));
        Self::insert_session(cx, context_name, connection);
    }

    /// Registers `context_name`'s session around an already-built `connection`,
    /// driving pause/resume off its forward's state when it has one.
    fn insert_session(cx: &mut App, context_name: &str, connection: Entity<ClusterConnection>) {
        let pods = cx.new(|_| PodsTable::default());
        let health = connection.read(cx).forward_state().map(|state_rx| {
            let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
                health::drive(state_rx, tx).await;
            });
            let context_name = context_name.to_string();
            cx.spawn(async move |cx| {
                crate::runtime::drain(rx, move |edge| {
                    let context_name = context_name.clone();
                    cx.update(move |cx| Self::apply_health_transition(cx, &context_name, edge));
                })
                .await;
            })
        });
        cx.global_mut::<Self>().sessions.insert(
            context_name.to_string(),
            ClusterSession {
                connection,
                pods,
                pods_watch: None,
                pods_client: None,
                watchers: WatchRegistry::new(),
                _health: health,
                holders: HashSet::new(),
            },
        );
    }

    /// Adds `window_id` to `context_name`'s holder set, connecting (or reusing) its
    /// session first if none exists yet. Design decision 1: a window's hold on a
    /// context is independent of whether any panel has subscribed to a watch - the
    /// Resource panel can list kinds for a context with no open panels.
    pub fn hold(cx: &mut App, context_name: &str, window_id: WindowId) {
        Self::ensure_init(cx, context_name);
        cx.global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .expect("ensure_init just inserted this session")
            .holders
            .insert(window_id);
    }

    /// Removes `window_id` from `context_name`'s holder set. On the last release,
    /// drops the session entirely - its connection (and so, via `ClusterConnection`'s
    /// own `Drop`, its tunnel forward) and its watches. A no-op if the context has no
    /// session, or `window_id` wasn't holding it.
    ///
    /// `window-context-bar` section 3.3's Disconnect calls this directly for the one
    /// context being disconnected; closing a window releases every context it holds
    /// at once, through [`Self::release_window`] instead.
    pub fn release(cx: &mut App, context_name: &str, window_id: WindowId) {
        if !cx.has_global::<Self>() {
            return;
        }
        let should_remove = {
            let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
                return;
            };
            session.holders.remove(&window_id);
            session.holders.is_empty()
        };
        if should_remove {
            cx.global_mut::<Self>().sessions.remove(context_name);
        }
    }

    /// Releases every hold `window_id` has, across every context - what closing a
    /// window does (design.md decision 2's "closing a window releases all its
    /// holds"), without the caller needing to know which contexts that window used.
    pub fn release_window(cx: &mut App, window_id: WindowId) {
        if !cx.has_global::<Self>() {
            return;
        }
        let emptied: Vec<String> = cx
            .global_mut::<Self>()
            .sessions
            .iter_mut()
            .filter_map(|(context_name, session)| {
                session.holders.remove(&window_id);
                session.holders.is_empty().then(|| context_name.clone())
            })
            .collect();
        for context_name in emptied {
            cx.global_mut::<Self>().sessions.remove(&context_name);
        }
    }

    /// How many windows currently hold `context_name` - `0` for a context with no
    /// session (never held, or its last holder already released it). The disconnect
    /// confirmation (`window-context-bar` section 3.3) reads this minus one (itself)
    /// for "stays connected in N other windows".
    pub fn holder_count(cx: &App, context_name: &str) -> usize {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .map(|session| session.holders.len())
            .unwrap_or(0)
    }

    /// Applies one health edge to `context_name`'s Pods watch - the only watched kind
    /// today. Pausing drops the watch task (stops consuming without unsubscribing);
    /// resuming restarts it from the last client used, and only if a panel is still
    /// subscribed - a health edge arriving after every panel unsubscribed has nothing
    /// to pause or resume.
    ///
    /// `pub(crate)` rather than private: this is the same real pause/resume entry point
    /// production code drives from a tunnel flap or a 401 (see [`Self::ensure_init`] and
    /// [`Self::handle_pods_unauthorized`]), and `connection-status-bar`'s integration
    /// tests (`ui/status_bar.rs`, `k8s/resource/pods.rs`) need to drive it directly to
    /// prove the status bar and the Pods panel each react correctly to a real pause -
    /// the same reasoning this module's own tests already use it for.
    pub(crate) fn apply_health_transition(
        cx: &mut App,
        context_name: &str,
        edge: HealthTransition,
    ) {
        if !cx.global::<Self>().sessions.contains_key(context_name) {
            return;
        }
        match edge {
            HealthTransition::Pause(reason) => {
                let paused = cx
                    .global_mut::<Self>()
                    .sessions
                    .get_mut(context_name)
                    .unwrap()
                    .watchers
                    .pause(&"pods", reason);
                if paused {
                    cx.global_mut::<Self>()
                        .sessions
                        .get_mut(context_name)
                        .unwrap()
                        .pods_watch = None;
                }
            }
            HealthTransition::Resume => {
                let session = cx
                    .global_mut::<Self>()
                    .sessions
                    .get_mut(context_name)
                    .unwrap();
                let should_restart =
                    session.watchers.resume(&"pods") && session.watchers.refcount(&"pods") > 0;
                let client = session.pods_client.clone();
                if should_restart && let Some(client) = client {
                    let watch = Self::start_pods_watch(cx, context_name, client);
                    cx.global_mut::<Self>()
                        .sessions
                        .get_mut(context_name)
                        .unwrap()
                        .pods_watch = Some(watch);
                }
            }
        }
    }

    /// Registers `context_name` with a connection fixed at `state`, so tests never race
    /// a real `ClusterConnection::connect` finishing (and overwriting the state with
    /// `Failed`) at an arbitrary point. Call before anything else looks the context up.
    #[cfg(test)]
    pub(crate) fn insert_test_session(
        cx: &mut App,
        context_name: &str,
        state: super::connection::ConnectionState,
    ) -> Entity<ClusterConnection> {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        let connection = cx.new(|_| ClusterConnection::test_with_state(state));
        Self::insert_session(cx, context_name, connection.clone());
        connection
    }

    /// [`Self::insert_test_session`] for a caller that already built the connection
    /// entity itself - the hold/release tests use this to install a
    /// [`ClusterConnection`] carrying a real tunnel forward
    /// (`ClusterConnection::test_with_state_and_forward`), which `insert_test_session`
    /// itself has no way to express.
    #[cfg(test)]
    pub(crate) fn insert_test_session_with_connection(
        cx: &mut App,
        context_name: &str,
        connection: Entity<ClusterConnection>,
    ) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        Self::insert_session(cx, context_name, connection);
    }

    /// Returns `context_name`'s cluster connection, connecting lazily on first use.
    pub fn connection(cx: &mut App, context_name: &str) -> Entity<ClusterConnection> {
        Self::ensure_init(cx, context_name);
        cx.global::<Self>().sessions[context_name]
            .connection
            .clone()
    }

    /// Subscribes a panel to `context_name`'s shared Pods watch, starting it on the
    /// 0-to-1 transition. Returns the shared table to render from.
    pub fn subscribe_pods(cx: &mut App, context_name: &str, client: Client) -> Entity<PodsTable> {
        Self::ensure_init(cx, context_name);
        let table = cx.global::<Self>().sessions[context_name].pods.clone();
        let should_start = cx
            .global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap()
            .watchers
            .subscribe("pods");
        if should_start {
            cx.global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .pods_client = Some(client.clone());
            let watch = Self::start_pods_watch(cx, context_name, client);
            cx.global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .pods_watch = Some(watch);
        }
        table
    }

    /// Starts the Pods watch against `client` for `context_name`, routing a 401 through
    /// [`Self::handle_pods_unauthorized`] - the one place that knows how to pause and
    /// attempt a credential refresh. Shared by the initial subscribe and every restart
    /// (health resume, successful credential refresh) so both go through one path.
    fn start_pods_watch(cx: &mut App, context_name: &str, client: Client) -> gpui_kit::Task<()> {
        let table = cx.global::<Self>().sessions[context_name].pods.clone();
        let context_name = context_name.to_string();
        watch_all_namespaces(
            client,
            table,
            move |cx| Self::handle_pods_unauthorized(cx, &context_name),
            cx,
        )
    }

    /// Section 7.3: a watch stream reported a 401 for `context_name`. Pauses through the
    /// same path a forward flap uses (section 7.2), then attempts one credential refresh
    /// by re-resolving that context's config and probing again - the same sequence the
    /// original connect used, reused via [`super::connection::connect_and_probe`] rather
    /// than duplicated. A successful probe resumes through that same path with the
    /// refreshed client; a failed one leaves the watch paused - closing every subscribed
    /// panel is still what releases it (`unsubscribe_pods`, already unconditional on
    /// health/auth state).
    fn handle_pods_unauthorized(cx: &mut App, context_name: &str) {
        if !cx.global::<Self>().sessions.contains_key(context_name) {
            return;
        }
        Self::apply_health_transition(
            cx,
            context_name,
            HealthTransition::Pause(PauseReason::CredentialRefresh),
        );

        let connection = cx.global::<Self>().sessions[context_name]
            .connection
            .clone();
        let forward_wait = connection.read(cx).forward_wait();
        let context_for_resolve = context_name.to_string();
        let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
            let config_result =
                super::connection::resolve_config(Some(context_for_resolve.as_str())).await;
            super::connection::connect_and_probe(config_result, forward_wait, tx).await;
        });
        let context_name = context_name.to_string();
        cx.spawn(async move |cx| {
            crate::runtime::drain(rx, move |state| {
                if let super::connection::ConnectionState::Connected(client) = state {
                    let context_name = context_name.clone();
                    cx.update(move |cx| Self::refresh_pods_client(cx, &context_name, client));
                }
            })
            .await;
        })
        .detach();
    }

    /// A credential refresh succeeded for `context_name`: record the new client and
    /// resume through the same pause/resume path section 7.2 established, restarting
    /// the watch from it.
    fn refresh_pods_client(cx: &mut App, context_name: &str, client: Client) {
        if !cx.global::<Self>().sessions.contains_key(context_name) {
            return;
        }
        cx.global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap()
            .pods_client = Some(client);
        Self::apply_health_transition(cx, context_name, HealthTransition::Resume);
    }

    /// `connection-status-bar` design.md decision 1: `context_name`'s health for the
    /// window status bar, combining its connection state with its watch registry's first
    /// paused key at precedence failed > paused > waiting for tunnel > connected. A
    /// context with no session yet (no panel has ever subscribed to it) reads as
    /// `Connected` - the bar only ever asks about contexts a window is actually using,
    /// which always has a session by the time it asks.
    pub fn health(cx: &App, context_name: &str) -> ContextHealth {
        let Some(session) = cx
            .try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
        else {
            return ContextHealth::Connected;
        };
        let connection = session.connection.read(cx);
        if let ConnectionState::Failed(reason) = &connection.state {
            return ContextHealth::Failed {
                reason: reason.clone(),
                since: connection.since(),
            };
        }
        if let Some((reason, since)) = session.watchers.first_paused() {
            return ContextHealth::Paused { reason, since };
        }
        if let ConnectionState::WaitingForTunnel = &connection.state {
            return ContextHealth::WaitingForTunnel {
                since: connection.since(),
            };
        }
        ContextHealth::Connected
    }

    /// Whether `context_name`'s bound forward, if it has one, is currently `Up` or
    /// `Reconnecting` - `Cmd-W`'s test for whether closing this window would tear down
    /// a live tunnel. `false` for an unbound context, one with no session yet, or one
    /// whose forward never came up.
    pub fn has_active_tunnel(cx: &App, context_name: &str) -> bool {
        let Some(session) = cx
            .try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
        else {
            return false;
        };
        let Some(state) = session.connection.read(cx).forward_state() else {
            return false;
        };
        is_tunnel_active(*state.borrow())
    }

    /// Unsubscribes a panel from `context_name`'s shared Pods watch, tearing it down on
    /// the 1-to-0 transition.
    pub fn unsubscribe_pods(cx: &mut App, context_name: &str) {
        if !cx.has_global::<Self>() {
            return;
        }
        let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
            return;
        };
        if session.watchers.unsubscribe(&"pods") {
            session.pods_watch = None;
        }
    }
}

/// Whether a forward's state counts as "actively tunneling" for `Cmd-W`'s
/// tunnel-teardown warning - up, or reconnecting after having been up (still worth
/// warning about, since traffic was flowing through it a moment ago). Pulled out of
/// [`ClusterRegistry::has_active_tunnel`] as a plain value so it's testable without a
/// session, a connection, or a real forward.
fn is_tunnel_active(state: ForwardState) -> bool {
    matches!(state, ForwardState::Up | ForwardState::Reconnecting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;
    use kube::Config;

    #[test]
    fn tunnel_is_active_up_or_reconnecting_only() {
        assert!(is_tunnel_active(ForwardState::Up));
        assert!(is_tunnel_active(ForwardState::Reconnecting));
        assert!(!is_tunnel_active(ForwardState::Connecting));
        assert!(!is_tunnel_active(ForwardState::Disconnected));
    }

    #[gpui_kit::test]
    async fn has_active_tunnel_is_false_with_no_session_or_no_bound_forward(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);

        assert!(
            !cx.update(|cx| ClusterRegistry::has_active_tunnel(cx, "never-seen")),
            "a context with no session has no tunnel to warn about"
        );

        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connected(client))
        });
        assert!(
            !cx.update(|cx| ClusterRegistry::has_active_tunnel(cx, "kind-dev")),
            "an unbound context's connection has no forward at all"
        );
    }

    fn test_client(cx: &mut TestAppContext) -> Client {
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    }

    /// Tasks.md 1.2: `ClusterRegistry::hold`/`release`/`release_window`, keyed by
    /// window rather than by panel (design.md decision 2).
    mod hold_release {
        use super::*;
        use crate::config::tunnels::{TunnelAuth, TunnelConfig};
        use crate::k8s::cluster::connection::ClusterConnection;
        use crate::k8s::cluster::tunnel::{self, ForwardKey};
        use crate::tunnel::store::TunnelStore;
        use gpui_kit::WindowId;
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);

        fn temp_path(label: &str) -> std::path::PathBuf {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fernrohr-session-hold-release-{label}-{}-{n}",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&path);
            path
        }

        /// A one-context kubeconfig fixture, mirroring `k8s::cluster::tunnel`'s own
        /// test fixtures - the seam that lets `tunnel::acquire_for_context` resolve a
        /// target without touching this machine's real kubeconfig.
        fn kubeconfig_fixture(context_name: &str, server: &str) -> std::path::PathBuf {
            let path = temp_path("kubeconfig");
            let yaml = format!(
                "apiVersion: v1\nkind: Config\nclusters:\n  - name: {context_name}\n    cluster:\n      server: {server}\ncontexts:\n  - name: {context_name}\n    context:\n      cluster: {context_name}\n      user: {context_name}\nusers:\n  - name: {context_name}\n    user: {{}}\n"
            );
            std::fs::write(&path, yaml).unwrap();
            path
        }

        fn sample_tunnel(name: &str) -> TunnelConfig {
            TunnelConfig {
                name: name.to_string(),
                bastion_user: "deploy".to_string(),
                bastion_host: "bastion.example.invalid".to_string(),
                bastion_port: 22,
                jump_hosts: Vec::new(),
                auth: TunnelAuth::default(),
            }
        }

        #[gpui_kit::test]
        async fn two_windows_share_one_session_and_the_first_release_keeps_it(
            cx: &mut TestAppContext,
        ) {
            cx.executor().allow_parking();
            cx.update(crate::runtime::init);
            let client = test_client(cx);
            cx.update(|cx| {
                ClusterRegistry::insert_test_session(
                    cx,
                    "kind-dev",
                    ConnectionState::Connected(client),
                )
            });

            let window_a = WindowId::from(1);
            let window_b = WindowId::from(2);
            cx.update(|cx| ClusterRegistry::hold(cx, "kind-dev", window_a));
            cx.update(|cx| ClusterRegistry::hold(cx, "kind-dev", window_b));
            assert_eq!(
                cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
                2
            );

            cx.update(|cx| ClusterRegistry::release(cx, "kind-dev", window_a));
            assert_eq!(
                cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
                1,
                "the session, and window_b's watches, must survive one release"
            );
            assert!(cx.update(|cx| {
                cx.global::<ClusterRegistry>()
                    .sessions
                    .contains_key("kind-dev")
            }));

            cx.update(|cx| ClusterRegistry::release(cx, "kind-dev", window_b));
            assert_eq!(
                cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
                0,
                "the last release must drop the session"
            );
            assert!(cx.update(|cx| {
                !cx.global::<ClusterRegistry>()
                    .sessions
                    .contains_key("kind-dev")
            }));
        }

        /// `release` on a window that never held the context, or a context with no
        /// session, is a no-op rather than a panic - closing a picker-mode window (no
        /// context ever held) goes through the same `release_window` path.
        #[gpui_kit::test]
        async fn releasing_an_unknown_context_or_window_is_a_no_op(cx: &mut TestAppContext) {
            cx.update(|cx| {
                ClusterRegistry::release(cx, "never-held", WindowId::from(1));
                ClusterRegistry::release_window(cx, WindowId::from(1));
            });
            assert_eq!(
                cx.update(|cx| ClusterRegistry::holder_count(cx, "never-held")),
                0
            );
        }

        /// Tasks.md 1.3's session-level half: releasing every hold a window has, in
        /// one call, only ever drops the sessions that window was the last holder of.
        #[gpui_kit::test]
        async fn release_window_drops_only_sessions_it_was_the_last_holder_of(
            cx: &mut TestAppContext,
        ) {
            cx.executor().allow_parking();
            cx.update(crate::runtime::init);
            let client_a = test_client(cx);
            let client_b = test_client(cx);
            cx.update(|cx| {
                ClusterRegistry::insert_test_session(
                    cx,
                    "solo",
                    ConnectionState::Connected(client_a),
                );
                ClusterRegistry::insert_test_session(
                    cx,
                    "shared",
                    ConnectionState::Connected(client_b),
                );
            });

            let window_a = WindowId::from(10);
            let window_b = WindowId::from(20);
            cx.update(|cx| {
                ClusterRegistry::hold(cx, "solo", window_a);
                ClusterRegistry::hold(cx, "shared", window_a);
                ClusterRegistry::hold(cx, "shared", window_b);
            });

            cx.update(|cx| ClusterRegistry::release_window(cx, window_a));

            assert_eq!(
                cx.update(|cx| ClusterRegistry::holder_count(cx, "solo")),
                0,
                "window_a was solo's only holder"
            );
            assert_eq!(
                cx.update(|cx| ClusterRegistry::holder_count(cx, "shared")),
                1,
                "window_b still holds shared"
            );
        }

        /// Design.md's own risk note: a session's connection carries a real tunnel
        /// forward, so the last release must be observable one layer down too - the
        /// `ForwardRegistry` entry (`live_forward_keys`) disappearing, not just the
        /// session map entry.
        #[gpui_kit::test]
        async fn last_release_drops_the_sessions_tunnel_forward(cx: &mut TestAppContext) {
            cx.executor().allow_parking();
            cx.update(crate::runtime::init);

            let tunnels_path = temp_path("tunnels.toml");
            let kubeconfig_path = kubeconfig_fixture("kind-dev", "https://10.0.0.1:6443");
            let store = TunnelStore::new(tunnels_path.clone());
            store
                .create("test-bastion", sample_tunnel("test"), None)
                .unwrap();
            store.bind("kind-dev", "test-bastion").unwrap();

            let handle = cx
                .update(|cx| {
                    tunnel::acquire_for_context(
                        cx,
                        &tunnels_path,
                        Some(&kubeconfig_path),
                        "kind-dev",
                    )
                })
                .unwrap()
                .expect("kind-dev is bound to test-bastion");
            let key = ForwardKey {
                tunnel_id: "test-bastion".to_string(),
                host: "10.0.0.1".to_string(),
                port: 6443,
            };
            let client = test_client(cx);
            let connection = cx.new(|_| {
                ClusterConnection::test_with_state_and_forward(
                    ConnectionState::Connected(client),
                    handle,
                )
            });
            cx.update(|cx| {
                ClusterRegistry::insert_test_session_with_connection(cx, "kind-dev", connection)
            });

            let window_a = WindowId::from(1);
            let window_b = WindowId::from(2);
            cx.update(|cx| {
                ClusterRegistry::hold(cx, "kind-dev", window_a);
                ClusterRegistry::hold(cx, "kind-dev", window_b);
            });

            let live = cx.update(tunnel::live_forward_keys);
            assert!(live.borrow().contains(&key));

            cx.update(|cx| ClusterRegistry::release(cx, "kind-dev", window_a));
            cx.run_until_parked();
            assert!(
                live.borrow().contains(&key),
                "one window still holds kind-dev"
            );

            cx.update(|cx| ClusterRegistry::release(cx, "kind-dev", window_b));
            cx.run_until_parked();
            assert!(
                !live.borrow().contains(&key),
                "the last release must drop the tunnel forward too"
            );

            let _ = std::fs::remove_file(&tunnels_path);
            let _ = std::fs::remove_file(&kubeconfig_path);
        }
    }

    #[gpui_kit::test]
    async fn two_subscribers_share_one_pods_watch_and_table(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);

        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        let table_a =
            cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client.clone()));
        let table_b = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));
        assert_eq!(
            table_a.entity_id(),
            table_b.entity_id(),
            "two subscribers should render from the same table"
        );

        // First unsubscribe (2-to-1) must not tear the watch down; only the
        // second (1-to-0) should.
        cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, "kind-dev"));
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .watchers
                .refcount(&"pods")),
            1
        );
        cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, "kind-dev"));
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .watchers
                .refcount(&"pods")),
            0
        );
    }

    /// Two different context names get independent sessions: independent tables and
    /// independent watch refcounts, the point of rekeying `ClusterSession` by context.
    #[gpui_kit::test]
    async fn different_contexts_get_independent_sessions(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);

        let client_a = test_client(cx);
        let client_b = test_client(cx);
        let table_a = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "dev", client_a));
        let table_b = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "staging", client_b));

        assert_ne!(
            table_a.entity_id(),
            table_b.entity_id(),
            "different contexts must not share a table"
        );
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterRegistry>().sessions["dev"]
                .watchers
                .refcount(&"pods")),
            1
        );
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterRegistry>().sessions["staging"]
                .watchers
                .refcount(&"pods")),
            1
        );

        cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, "dev"));
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterRegistry>().sessions["dev"]
                .watchers
                .refcount(&"pods")),
            0
        );
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterRegistry>().sessions["staging"]
                .watchers
                .refcount(&"pods")),
            1,
            "unsubscribing one context must not affect another"
        );
    }

    /// Section 7.3's two pieces, tested independently and hermetically rather than through
    /// `handle_pods_unauthorized` itself: that function calls the real `kube::Config::infer`
    /// to refresh a credential, which would read this machine's actual kubeconfig and
    /// attempt a real network probe if driven directly in a test - not something a unit
    /// test should do. `apply_health_transition` (the pause step, section 7.2) and
    /// `refresh_pods_client` (the resume-with-a-new-client step) are exactly what
    /// `handle_pods_unauthorized` composes; each is exercised here on its own, the same
    /// seam section 6.2's `connect_and_probe` tests already drove one layer down.
    #[gpui_kit::test]
    async fn unauthorized_pause_stops_the_watch_and_refresh_resumes_it(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);

        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));
        assert!(cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .pods_watch
                .is_some()
        }));

        cx.update(|cx| {
            ClusterRegistry::apply_health_transition(
                cx,
                "kind-dev",
                HealthTransition::Pause(PauseReason::CredentialRefresh),
            )
        });
        assert!(cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .watchers
                .is_paused(&"pods")
        }));
        assert!(cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .pods_watch
                .is_none()
        }));

        let refreshed_client = test_client(cx);
        cx.update(|cx| ClusterRegistry::refresh_pods_client(cx, "kind-dev", refreshed_client));
        assert!(!cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .watchers
                .is_paused(&"pods")
        }));
        assert!(cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .pods_watch
                .is_some()
        }));
    }

    /// `refresh_pods_client` arriving after every panel already unsubscribed (the watch
    /// entry gone entirely, section 7.1's teardown-drops-the-flag behavior) must not
    /// resurrect a watch nobody is subscribed to.
    #[gpui_kit::test]
    async fn refresh_after_every_panel_unsubscribed_does_not_restart_the_watch(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);

        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));
        cx.update(|cx| {
            ClusterRegistry::apply_health_transition(
                cx,
                "kind-dev",
                HealthTransition::Pause(PauseReason::CredentialRefresh),
            )
        });
        cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, "kind-dev"));

        let refreshed_client = test_client(cx);
        cx.update(|cx| ClusterRegistry::refresh_pods_client(cx, "kind-dev", refreshed_client));

        assert!(cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .pods_watch
                .is_none()
        }));
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .watchers
                .refcount(&"pods")),
            0
        );
    }

    /// `connection-status-bar` task 1.1: `health` reads `Connected` for a session whose
    /// connection hasn't failed, isn't waiting for a tunnel, and has nothing paused -
    /// which includes `Connecting`, since the spec has no distinct row for it (see
    /// `context_health::ContextHealth`'s own doc comment).
    #[gpui_kit::test]
    async fn health_is_connected_when_nothing_is_wrong(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);
        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

        assert_eq!(
            cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")),
            ContextHealth::Connected
        );
    }

    #[gpui_kit::test]
    async fn health_reads_waiting_for_tunnel_from_the_connection(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);
        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

        let connection = cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .connection
                .clone()
        });
        cx.update(|cx| {
            connection.update(cx, |connection, cx| {
                connection.state = ConnectionState::WaitingForTunnel;
                cx.notify();
            })
        });

        assert!(matches!(
            cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")),
            ContextHealth::WaitingForTunnel { .. }
        ));
    }

    #[gpui_kit::test]
    async fn health_reads_failed_from_the_connection(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);
        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

        let connection = cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .connection
                .clone()
        });
        cx.update(|cx| {
            connection.update(cx, |connection, cx| {
                connection.state = ConnectionState::Failed("boom".to_string());
                cx.notify();
            })
        });

        match cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")) {
            ContextHealth::Failed { reason, .. } => assert_eq!(reason, "boom"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[gpui_kit::test]
    async fn health_reads_paused_on_the_pods_key(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);
        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

        cx.update(|cx| {
            ClusterRegistry::apply_health_transition(
                cx,
                "kind-dev",
                HealthTransition::Pause(PauseReason::Reconnecting),
            )
        });

        match cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")) {
            ContextHealth::Paused { reason, .. } => assert_eq!(reason, PauseReason::Reconnecting),
            other => panic!("expected Paused, got {other:?}"),
        }
    }

    /// `first_paused` (`watch_registry.rs`) is what makes this work for a kind other than
    /// `"pods"` - `health` never has a hardcoded key of its own.
    #[gpui_kit::test]
    async fn health_reads_paused_on_a_kind_other_than_pods(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);
        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

        cx.update(|cx| {
            let session = cx
                .global_mut::<ClusterRegistry>()
                .sessions
                .get_mut("kind-dev")
                .unwrap();
            session.watchers.subscribe("events");
            session
                .watchers
                .pause(&"events", PauseReason::CredentialRefresh);
        });

        match cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")) {
            ContextHealth::Paused { reason, .. } => {
                assert_eq!(reason, PauseReason::CredentialRefresh)
            }
            other => panic!("expected Paused, got {other:?}"),
        }
    }

    #[gpui_kit::test]
    async fn health_prefers_failed_over_paused(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);
        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

        cx.update(|cx| {
            ClusterRegistry::apply_health_transition(
                cx,
                "kind-dev",
                HealthTransition::Pause(PauseReason::Reconnecting),
            )
        });
        let connection = cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .connection
                .clone()
        });
        cx.update(|cx| {
            connection.update(cx, |connection, cx| {
                connection.state = ConnectionState::Failed("boom".to_string());
                cx.notify();
            })
        });

        assert!(matches!(
            cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")),
            ContextHealth::Failed { .. }
        ));
    }

    #[gpui_kit::test]
    async fn health_prefers_paused_over_waiting_for_tunnel(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);
        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

        let connection = cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .connection
                .clone()
        });
        cx.update(|cx| {
            connection.update(cx, |connection, cx| {
                connection.state = ConnectionState::WaitingForTunnel;
                cx.notify();
            })
        });
        cx.update(|cx| {
            ClusterRegistry::apply_health_transition(
                cx,
                "kind-dev",
                HealthTransition::Pause(PauseReason::Reconnecting),
            )
        });

        assert!(matches!(
            cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")),
            ContextHealth::Paused { .. }
        ));
    }

    /// `connection-status-bar` task 1.3's audit: every pause/resume write already goes
    /// through `cx.global_mut::<Self>()`, which pushes `Effect::NotifyGlobalObservers`
    /// unconditionally on every call (see `gpui`'s `App::global_mut`) - so
    /// `observe_global::<ClusterRegistry>` already sees every edge with no extra
    /// plumbing. Proven here against the real production entry point,
    /// `apply_health_transition`, rather than trusted from reading the source.
    ///
    /// This does not cover `ConnectionState` transitions (`WaitingForTunnel` -> `Failed`,
    /// for instance): those notify their own `ClusterConnection` entity, not this global,
    /// so `ui/status_bar.rs` additionally observes each shown context's connection
    /// entity directly - the "smaller and correct" fix design.md decision 4 allows for,
    /// since routing connection-state writes through the registry global as well would
    /// make `connection.rs` depend on `session.rs` for no benefit.
    #[gpui_kit::test]
    async fn pause_and_resume_each_notify_registry_observers(cx: &mut TestAppContext) {
        use std::cell::Cell;
        use std::rc::Rc;

        cx.executor().allow_parking();
        cx.update(crate::runtime::init);
        let client = test_client(cx);
        cx.update(|cx| {
            ClusterRegistry::insert_test_session(
                cx,
                "kind-dev",
                ConnectionState::Connected(client.clone()),
            )
        });
        cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

        let notifications = Rc::new(Cell::new(0));
        let observed = notifications.clone();
        let _subscription = cx.update(|cx| {
            cx.observe_global::<ClusterRegistry>(move |_cx| observed.set(observed.get() + 1))
        });

        cx.update(|cx| {
            ClusterRegistry::apply_health_transition(
                cx,
                "kind-dev",
                HealthTransition::Pause(PauseReason::Reconnecting),
            )
        });
        assert_eq!(
            notifications.get(),
            1,
            "pausing must notify registry observers"
        );

        cx.update(|cx| {
            ClusterRegistry::apply_health_transition(cx, "kind-dev", HealthTransition::Resume)
        });
        assert_eq!(
            notifications.get(),
            2,
            "resuming must notify registry observers"
        );
    }
}
