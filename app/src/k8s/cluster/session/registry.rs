//! The session map itself: `ClusterSession` and `ClusterRegistry`'s definitions, plus the
//! bootstrapping that lazily creates a context's session on first use.

use super::*;

/// Per-context cluster state: the connection plus the shared, refcounted watch per
/// resource kind, so two panels showing the same resource kind for the same cluster
/// see the same data off one underlying `kube_runtime` stream.
pub(super) struct ClusterSession {
    pub(super) connection: Entity<ClusterConnection>,
    pub(super) pods: Entity<PodsTable>,
    pub(super) pods_watch: Option<gpui_kit::Task<()>>,
    /// The client last used to start the Pods watch - kept so section 7.2's
    /// `ConnectionHealth` can restart it on resume without a panel re-subscribing.
    pub(super) client: Option<Client>,
    /// Every other kind a panel watches on this context: one shared table and task
    /// per kind, present while at least one panel subscribes to it.
    pub(super) kinds: HashMap<DiscoveredKind, KindWatch>,
    /// The Event watch and its table, present while an events browser (or any
    /// other Event consumer) subscribes to it.
    pub(super) events: Option<EventsWatch>,
    pub(super) watchers: WatchRegistry<WatchKey>,
    // Kept alive for as long as the session exists; aborts on drop like every other
    // owned background task. `None` for an unbound context, which has no forward to watch.
    _health: Option<gpui_kit::Task<()>>,
    /// `window-context-bar` design.md decision 2: which windows currently use this
    /// context. The session (and so its connection and tunnel forward) lives exactly
    /// as long as this set is non-empty - see [`ClusterRegistry::hold`]/[`release`](
    /// ClusterRegistry::release).
    pub(super) holders: HashSet<WindowId>,
}

/// One kind's shared watch on a context: the table its panels render from, and the
/// task consuming the stream - `None` while the watch is paused.
pub(super) struct KindWatch {
    pub(super) table: Entity<ObjectsTable>,
    pub(super) task: Option<gpui_kit::Task<()>>,
}

/// A context's shared Event watch: the table its consumers render from, and the
/// task consuming the stream - `None` while the watch is paused.
pub(super) struct EventsWatch {
    pub(super) table: Entity<EventsTable>,
    pub(super) task: Option<gpui_kit::Task<()>>,
}

/// App-scoped (not per-window) cluster state, keyed by context name so two windows
/// (or two panels in the same window) connected to different contexts each get their
/// own connection and watches, while both connected to the same context share one.
#[derive(Default)]
pub struct ClusterRegistry {
    pub(super) sessions: HashMap<String, ClusterSession>,
}

impl Global for ClusterRegistry {}

impl ClusterRegistry {
    pub(super) fn ensure_init(cx: &mut App, context_name: &str) {
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
                client: None,
                kinds: HashMap::new(),
                events: None,
                watchers: WatchRegistry::new(),
                _health: health,
                holders: HashSet::new(),
            },
        );
    }

    /// Registers `context_name` with a connection fixed at `state`, so tests never race
    /// a real `ClusterConnection::connect` finishing (and overwriting the state with
    /// `Failed`) at an arbitrary point. Call before anything else looks the context up.
    #[cfg(test)]
    pub(crate) fn insert_test_session(
        cx: &mut App,
        context_name: &str,
        state: ConnectionState,
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

    /// How many panels subscribe to `key`'s shared watch on `context_name`, for
    /// tests outside this module checking a panel released its subscription.
    #[cfg(test)]
    pub(crate) fn subscribers(cx: &App, context_name: &str, key: &WatchKey) -> usize {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .map_or(0, |session| session.watchers.refcount(key))
    }

    /// `context_name`'s connection if a session for it is open. Unlike
    /// [`Self::connection`], never connects: for a reader (`agent-mcp`'s tools)
    /// that must only use what the user already opened.
    pub fn existing_connection(cx: &App, context_name: &str) -> Option<Entity<ClusterConnection>> {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .map(|session| session.connection.clone())
    }

    /// Every context with an open session, in no particular order.
    pub fn open_contexts(cx: &App) -> Vec<String> {
        cx.try_global::<Self>()
            .map(|registry| registry.sessions.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Returns `context_name`'s cluster connection, connecting lazily on first use.
    pub fn connection(cx: &mut App, context_name: &str) -> Entity<ClusterConnection> {
        Self::ensure_init(cx, context_name);
        cx.global::<Self>().sessions[context_name]
            .connection
            .clone()
    }
}
