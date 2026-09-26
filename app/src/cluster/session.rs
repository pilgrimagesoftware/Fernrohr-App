use super::connection::ClusterConnection;
use super::health::{self, HealthTransition};
use super::watch_registry::{PauseReason, WatchRegistry};
use crate::pods::{PodsTable, watch_all_namespaces};
use gpui_kit::{App, AppContext as _, Entity, Global};
use kube::Client;

/// App-scoped (not per-window) cluster state: the shared connection plus the
/// shared, refcounted watch per resource kind, so two panels showing the
/// same resource kind for the same cluster see the same data off one
/// underlying `kube_runtime` stream.
pub struct ClusterSession {
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
}

impl Global for ClusterSession {}

impl ClusterSession {
    fn ensure_init(cx: &mut App) {
        if !cx.has_global::<Self>() {
            let connection = ClusterConnection::connect(cx);
            let pods = cx.new(|_| PodsTable::default());
            let health = connection.read(cx).forward_state().map(|state_rx| {
                let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
                    health::drive(state_rx, tx).await;
                });
                cx.spawn(async move |cx| {
                    crate::runtime::drain(rx, move |edge| {
                        cx.update(move |cx| Self::apply_health_transition(cx, edge));
                    })
                    .await;
                })
            });
            cx.set_global(Self {
                connection,
                pods,
                pods_watch: None,
                pods_client: None,
                watchers: WatchRegistry::new(),
                _health: health,
            });
        }
    }

    /// Applies one health edge to the Pods watch - the only watched kind today. Pausing
    /// drops the watch task (stops consuming without unsubscribing); resuming restarts it
    /// from the last client used, and only if a panel is still subscribed - a health edge
    /// arriving after every panel unsubscribed has nothing to pause or resume.
    fn apply_health_transition(cx: &mut App, edge: HealthTransition) {
        if !cx.has_global::<Self>() {
            return;
        }
        match edge {
            HealthTransition::Pause(reason) => {
                if cx.global_mut::<Self>().watchers.pause(&"pods", reason) {
                    cx.global_mut::<Self>().pods_watch = None;
                }
            }
            HealthTransition::Resume => {
                let should_restart = cx.global_mut::<Self>().watchers.resume(&"pods")
                    && cx.global::<Self>().watchers.refcount(&"pods") > 0;
                if should_restart && let Some(client) = cx.global::<Self>().pods_client.clone() {
                    let watch = Self::start_pods_watch(cx, client);
                    cx.global_mut::<Self>().pods_watch = Some(watch);
                }
            }
        }
    }

    /// Returns the app's shared cluster connection, connecting lazily on
    /// first use.
    pub fn connection(cx: &mut App) -> Entity<ClusterConnection> {
        Self::ensure_init(cx);
        cx.global::<Self>().connection.clone()
    }

    /// Subscribes a panel to the shared Pods watch, starting it on the
    /// 0-to-1 transition. Returns the shared table to render from.
    pub fn subscribe_pods(cx: &mut App, client: Client) -> Entity<PodsTable> {
        Self::ensure_init(cx);
        let table = cx.global::<Self>().pods.clone();
        if cx.global_mut::<Self>().watchers.subscribe("pods") {
            cx.global_mut::<Self>().pods_client = Some(client.clone());
            let watch = Self::start_pods_watch(cx, client);
            cx.global_mut::<Self>().pods_watch = Some(watch);
        }
        table
    }

    /// Starts the Pods watch against `client`, routing a 401 through
    /// [`Self::handle_pods_unauthorized`] - the one place that knows how to pause and
    /// attempt a credential refresh. Shared by the initial subscribe and every restart
    /// (health resume, successful credential refresh) so both go through one path.
    fn start_pods_watch(cx: &mut App, client: Client) -> gpui_kit::Task<()> {
        let table = cx.global::<Self>().pods.clone();
        watch_all_namespaces(client, table, Self::handle_pods_unauthorized, cx)
    }

    /// Section 7.3: a watch stream reported a 401. Pauses through the same path a forward
    /// flap uses (section 7.2), then attempts one credential refresh by re-resolving the
    /// context's config and probing again - the same sequence the original connect used,
    /// reused via [`super::connection::connect_and_probe`] rather than duplicated. A
    /// successful probe resumes through that same path with the refreshed client; a failed
    /// one leaves the watch paused - closing every subscribed panel is still what releases
    /// it (`unsubscribe_pods`, already unconditional on health/auth state).
    fn handle_pods_unauthorized(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        Self::apply_health_transition(cx, HealthTransition::Pause(PauseReason::CredentialRefresh));

        let connection = cx.global::<Self>().connection.clone();
        let forward_wait = connection.read(cx).forward_wait();
        let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
            let config_result = kube::Config::infer()
                .await
                .map_err(|error| error.to_string());
            super::connection::connect_and_probe(config_result, forward_wait, tx).await;
        });
        cx.spawn(async move |cx| {
            crate::runtime::drain(rx, move |state| {
                if let super::connection::ConnectionState::Connected(client) = state {
                    cx.update(move |cx| Self::refresh_pods_client(cx, client));
                }
            })
            .await;
        })
        .detach();
    }

    /// A credential refresh succeeded: record the new client and resume through the same
    /// pause/resume path section 7.2 established, restarting the watch from it.
    fn refresh_pods_client(cx: &mut App, client: Client) {
        if !cx.has_global::<Self>() {
            return;
        }
        cx.global_mut::<Self>().pods_client = Some(client);
        Self::apply_health_transition(cx, HealthTransition::Resume);
    }

    /// Why the Pods watch is currently paused and for how long, for section 7.4's panel
    /// display. `None` when the watch is active or nobody is subscribed.
    pub fn pods_pause_info(cx: &App) -> Option<(PauseReason, std::time::Duration)> {
        if !cx.has_global::<Self>() {
            return None;
        }
        cx.global::<Self>().watchers.pause_info(&"pods")
    }

    /// Unsubscribes a panel from the shared Pods watch, tearing it down on
    /// the 1-to-0 transition.
    pub fn unsubscribe_pods(cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        if cx.global_mut::<Self>().watchers.unsubscribe(&"pods") {
            cx.global_mut::<Self>().pods_watch = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;
    use kube::Config;

    fn test_client(cx: &mut TestAppContext) -> Client {
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    }

    #[gpui_kit::test]
    async fn two_subscribers_share_one_pods_watch_and_table(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);

        let client = test_client(cx);
        let table_a = cx.update(|cx| ClusterSession::subscribe_pods(cx, client.clone()));
        let table_b = cx.update(|cx| ClusterSession::subscribe_pods(cx, client));
        assert_eq!(
            table_a.entity_id(),
            table_b.entity_id(),
            "two subscribers should render from the same table"
        );

        // First unsubscribe (2-to-1) must not tear the watch down; only the
        // second (1-to-0) should.
        cx.update(ClusterSession::unsubscribe_pods);
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterSession>().watchers.refcount(&"pods")),
            1
        );
        cx.update(ClusterSession::unsubscribe_pods);
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterSession>().watchers.refcount(&"pods")),
            0
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
        cx.update(|cx| ClusterSession::subscribe_pods(cx, client));
        assert!(cx.update(|cx| cx.global::<ClusterSession>().pods_watch.is_some()));

        cx.update(|cx| {
            ClusterSession::apply_health_transition(
                cx,
                HealthTransition::Pause(PauseReason::CredentialRefresh),
            )
        });
        assert!(cx.update(|cx| cx.global::<ClusterSession>().watchers.is_paused(&"pods")));
        assert!(cx.update(|cx| cx.global::<ClusterSession>().pods_watch.is_none()));

        let refreshed_client = test_client(cx);
        cx.update(|cx| ClusterSession::refresh_pods_client(cx, refreshed_client));
        assert!(!cx.update(|cx| cx.global::<ClusterSession>().watchers.is_paused(&"pods")));
        assert!(cx.update(|cx| cx.global::<ClusterSession>().pods_watch.is_some()));
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
        cx.update(|cx| ClusterSession::subscribe_pods(cx, client));
        cx.update(|cx| {
            ClusterSession::apply_health_transition(
                cx,
                HealthTransition::Pause(PauseReason::CredentialRefresh),
            )
        });
        cx.update(ClusterSession::unsubscribe_pods);

        let refreshed_client = test_client(cx);
        cx.update(|cx| ClusterSession::refresh_pods_client(cx, refreshed_client));

        assert!(cx.update(|cx| cx.global::<ClusterSession>().pods_watch.is_none()));
        assert_eq!(
            cx.update(|cx| cx.global::<ClusterSession>().watchers.refcount(&"pods")),
            0
        );
    }
}
