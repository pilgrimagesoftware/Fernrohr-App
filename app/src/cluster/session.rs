use super::connection::ClusterConnection;
use super::health::{self, HealthTransition};
use super::watch_registry::WatchRegistry;
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
            HealthTransition::Pause => {
                if cx.global_mut::<Self>().watchers.pause(&"pods") {
                    cx.global_mut::<Self>().pods_watch = None;
                }
            }
            HealthTransition::Resume => {
                let should_restart = cx.global_mut::<Self>().watchers.resume(&"pods")
                    && cx.global::<Self>().watchers.refcount(&"pods") > 0;
                if should_restart && let Some(client) = cx.global::<Self>().pods_client.clone() {
                    let table = cx.global::<Self>().pods.clone();
                    let watch = watch_all_namespaces(client, table, cx);
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
            let watch = watch_all_namespaces(client, table.clone(), cx);
            cx.global_mut::<Self>().pods_watch = Some(watch);
        }
        table
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
}
