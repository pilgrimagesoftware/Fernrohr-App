use crate::ui::nav::NavTarget;
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use crate::util::resource_index::ResourceIndex;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;
use kube_runtime::watcher;

/// The subset of a `Pod` a list row needs, computed fresh from the object at
/// render time rather than stored separately - see design D2 on field
/// pruning.
#[derive(Debug, Clone, PartialEq)]
pub struct PodRow {
    pub name: String,
    pub namespace: String,
    pub ready: String,
    pub status: String,
    pub restarts: i32,
    pub age: String,
    pub pod_ip: String,
    pub node: String,
    /// Raw seconds behind `age`'s display string - kept separately because
    /// the display string ("9m" vs "10m") doesn't sort correctly as text.
    pub age_secs: i64,
}

fn uid(pod: &Pod) -> String {
    pod.metadata.uid.clone().unwrap_or_default()
}

/// Formats an age the way `kubectl get pods` does: the single largest unit,
/// seconds up to a minute, then minutes, hours, days.
pub(crate) fn format_age(age_secs: i64) -> String {
    let age_secs = age_secs.max(0);
    if age_secs < 60 {
        format!("{age_secs}s")
    } else if age_secs < 3600 {
        format!("{}m", age_secs / 60)
    } else if age_secs < 86400 {
        format!("{}h", age_secs / 3600)
    } else {
        format!("{}d", age_secs / 86400)
    }
}

/// Projects a `Pod` into its table row, using `now` for the age column
/// (injected rather than read from the clock, so callers can test with a
/// fixed instant).
pub fn pod_row(pod: &Pod, now: Timestamp) -> PodRow {
    let status = pod.status.as_ref();
    let container_statuses = status.and_then(|s| s.container_statuses.as_ref());
    let total = container_statuses.map_or(0, |c| c.len());
    let ready_count = container_statuses.map_or(0, |c| c.iter().filter(|c| c.ready).count());
    let restarts = container_statuses.map_or(0, |c| c.iter().map(|c| c.restart_count).sum());
    let age_secs = pod
        .metadata
        .creation_timestamp
        .as_ref()
        .map(|t| now.duration_since(t.0).as_secs_f64() as i64)
        .unwrap_or_default();

    PodRow {
        name: pod.metadata.name.clone().unwrap_or_default(),
        namespace: pod.metadata.namespace.clone().unwrap_or_default(),
        ready: format!("{ready_count}/{total}"),
        status: status.and_then(|s| s.phase.clone()).unwrap_or_default(),
        restarts,
        age: format_age(age_secs),
        pod_ip: status.and_then(|s| s.pod_ip.clone()).unwrap_or_default(),
        node: pod
            .spec
            .as_ref()
            .and_then(|spec| spec.node_name.clone())
            .unwrap_or_default(),
        age_secs,
    }
}

/// The live Pods index for one watch, kept up to date by [`PodsTable::apply`]
/// as `watcher::Event`s arrive off the drain.
#[derive(Default)]
pub struct PodsTable {
    index: ResourceIndex<Pod>,
    /// Uids seen so far in the current `Init..InitDone` relist, if one is in
    /// progress. `kube_runtime::watcher` handles the actual reconnect/relist
    /// (design D3) and restarts this cycle after a terminal error; a pod
    /// deleted while disconnected never gets an explicit `Delete` - it's
    /// simply absent from the relist - so `InitDone` sweeps anything not
    /// seen during the cycle.
    relisting: Option<std::collections::HashSet<String>>,
}

impl PodsTable {
    // UNWIRED: `PodsPanel::new` builds this via `PodsTable::default`; only
    // this module's own tests call `new`.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pods(&self) -> &[Pod] {
        self.index.items()
    }

    /// Applies one watch event. `Apply`/`InitApply` upsert by uid, `Delete`
    /// removes by uid. `Init` starts a relist cycle; `InitDone` ends it and
    /// removes anything present before the relist that wasn't re-seen during
    /// it, converging the table to the post-reconnect cluster state.
    pub fn apply(&mut self, event: watcher::Event<Pod>) {
        match event {
            watcher::Event::Apply(pod) => {
                self.index.apply_applied(uid(&pod), pod);
            }
            watcher::Event::InitApply(pod) => {
                let id = uid(&pod);
                if let Some(seen) = &mut self.relisting {
                    seen.insert(id.clone());
                }
                self.index.apply_applied(id, pod);
            }
            watcher::Event::Delete(pod) => {
                self.index.apply_deleted(&uid(&pod));
            }
            watcher::Event::Init => {
                self.relisting = Some(std::collections::HashSet::new());
            }
            watcher::Event::InitDone => {
                let Some(seen) = self.relisting.take() else {
                    return;
                };
                let stale: Vec<String> = self
                    .index
                    .items()
                    .iter()
                    .map(uid)
                    .filter(|id| !seen.contains(id))
                    .collect();
                for id in stale {
                    self.index.apply_deleted(&id);
                }
            }
        }
    }
}

/// True for a watch stream error that came back as an HTTP 401 - an expired or otherwise
/// rejected credential, as opposed to a transient network error `kube_runtime`'s own
/// backoff already retries transparently. Section 7.3: this is what routes a stream error
/// through the exec-plugin-refresh path instead of the ordinary swallow-and-retry one.
fn is_unauthorized(error: &watcher::Error) -> bool {
    let kube_error = match error {
        watcher::Error::InitialListFailed(error)
        | watcher::Error::WatchStartFailed(error)
        | watcher::Error::WatchFailed(error) => Some(error),
        watcher::Error::WatchError(_) | watcher::Error::NoResourceVersion => None,
    };
    matches!(kube_error, Some(kube::Error::Api(status)) if status.code == 401)
}

/// One outcome off the watch stream: a normal event to apply, or a 401 - which ends this
/// watch (the caller decides whether/how to restart it with a refreshed credential).
enum WatchOutcome {
    Event(Box<watcher::Event<Pod>>),
    Unauthorized,
}

/// Starts a `kube_runtime::watcher` for all Pods across every namespace on
/// `client` and applies its events to `table` as they arrive. Reconnect and
/// backoff after a transient stream error are `kube_runtime`'s own job (design D3); this
/// just keeps consuming the stream - except a 401, which this watch has no way to recover
/// from itself (the same expired credential would just come back), so it stops and calls
/// `on_unauthorized` once instead of retrying forever against a token that will never work.
pub fn watch_all_namespaces(
    client: kube::Client,
    table: gpui_kit::Entity<PodsTable>,
    on_unauthorized: impl FnOnce(&mut gpui_kit::App) + Send + 'static,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<()> {
    use futures_util::StreamExt;
    use kube::Api;

    let rx = crate::runtime::spawn_stream(cx, 64, move |tx| async move {
        let api: Api<Pod> = Api::all(client);
        let mut stream = Box::pin(watcher::watcher(api, watcher::Config::default()));
        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => {
                    if tx.send(WatchOutcome::Event(Box::new(event))).await.is_err() {
                        break;
                    }
                }
                Err(error) if is_unauthorized(&error) => {
                    let _ = tx.send(WatchOutcome::Unauthorized).await;
                    return;
                }
                Err(_) => continue,
            }
        }
    });
    cx.spawn(async move |cx| {
        let mut on_unauthorized = Some(on_unauthorized);
        crate::runtime::drain(rx, move |outcome| match outcome {
            WatchOutcome::Event(event) => {
                table.update(cx, |table, cx| {
                    table.apply(*event);
                    cx.notify();
                });
            }
            WatchOutcome::Unauthorized => {
                if let Some(on_unauthorized) = on_unauthorized.take() {
                    cx.update(|cx| on_unauthorized(cx));
                }
            }
        })
        .await;
    })
}

use crate::config::workspace::{NamespaceScope, SortState};

// UNWIRED: `view_rows` below composes this into the view pipeline; `PodsPanel::render`
// doesn't call `view_rows` yet (it renders `pods()` unfiltered/unsorted), so neither
// reaches the bin target. First real caller is whatever wires namespace-scope/sort
// UI state into the panel.
#[allow(dead_code)]
pub fn matches_namespace(pod: &Pod, scope: &NamespaceScope) -> bool {
    match scope {
        NamespaceScope::All => true,
        NamespaceScope::Single(namespace) => {
            pod.metadata.namespace.as_deref() == Some(namespace.as_str())
        }
    }
}

pub fn matches_namespaces(pod: &Pod, namespaces: &[String]) -> bool {
    namespaces.is_empty()
        || pod
            .metadata
            .namespace
            .as_ref()
            .is_some_and(|namespace| namespaces.contains(namespace))
}

// UNWIRED: see `matches_namespace` above.
#[allow(dead_code)]
fn matches_filter(row: &PodRow, filter: &str) -> bool {
    filter.is_empty() || row.name.contains(filter)
}

// UNWIRED: see `matches_namespace` above.
#[allow(dead_code)]
fn sort_rows(rows: &mut [PodRow], sort: &SortState) {
    let col = pods_table::PodColumn::from_id(&sort.column);
    if sort.ascending {
        rows.sort_by(|a, b| pods_table::compare(a, b, col));
    } else {
        // A reversed comparator, not `.reverse()` on the slice - see
        // `PodTableDelegate::apply_sort` for why that matters with ties.
        rows.sort_by(|a, b| pods_table::compare(b, a, col));
    }
}

/// The full view pipeline for a Pods panel: scope to a namespace, project to
/// rows, apply the name filter, then sort. Pure and GPUI-free so it's
/// directly unit-testable as "the view model".
// UNWIRED: see `matches_namespace` above - `PodsPanel::render` doesn't call this yet.
#[allow(dead_code)]
pub fn view_rows(
    pods: &[Pod],
    now: Timestamp,
    namespace: &NamespaceScope,
    name_filter: &str,
    sort: &SortState,
) -> Vec<PodRow> {
    let mut rows: Vec<PodRow> = pods
        .iter()
        .filter(|pod| matches_namespace(pod, namespace))
        .map(|pod| pod_row(pod, now))
        .filter(|row| matches_filter(row, name_filter))
        .collect();
    sort_rows(&mut rows, sort);
    rows
}

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::table::{DataTable, TableEvent, TableState};
use gpui_kit::*;

use super::pods_table::{self, PodTableDelegate, PodTableRow};

actions!(pods, [WarpNamespace, DescribePod, ShowPodLogs, ShowPodYaml]);

pub const PANEL_KEY_CONTEXT: &str = "PodsPanel";

/// The keys the panel's shortcut hint bar prints, and the keys the panel's
/// bindings use. One set of letters, named once, because the hint bar reads
/// the keymap and falls back to a literal: if the two lists drift, the panel
/// advertises a shortcut it does not have.
const NAMESPACE_KEY: &str = "w";
const DESCRIBE_KEY: &str = "d";
const LOGS_KEY: &str = "l";
const YAML_KEY: &str = "y";

/// The panel's own keybindings, in the panel's key context.
///
/// In the context rather than global on purpose: `d` means "describe the
/// selected pod" while a Pods panel is on the focus path, and means nothing
/// anywhere else. A context binding matches at any depth of the focus path,
/// so these still fire once a table row has taken focus from the panel. The
/// bindings are registered with the window's keymap rather than assumed: a
/// panel that prints a key has not thereby bound it.
pub fn panel_bindings() -> [KeyBinding; 4] {
    [
        KeyBinding::new(NAMESPACE_KEY, WarpNamespace, Some(PANEL_KEY_CONTEXT)),
        KeyBinding::new(DESCRIBE_KEY, DescribePod, Some(PANEL_KEY_CONTEXT)),
        KeyBinding::new(LOGS_KEY, ShowPodLogs, Some(PANEL_KEY_CONTEXT)),
        KeyBinding::new(YAML_KEY, ShowPodYaml, Some(PANEL_KEY_CONTEXT)),
    ]
}

pub fn register_restore(cx: &mut App) {
    register_panel(cx, "Pods", |context, _window, cx| {
        let PanelInfo::Panel(state) = context.info() else {
            panic!("Pods layout state must be a panel");
        };
        let context_name = state["context_name"]
            .as_str()
            .expect("Pods layout state must name its cluster")
            .to_string();
        let namespaces = serde_json::from_value(state["namespaces"].clone()).unwrap_or_default();
        let scope = PanelScope::new(NavTarget::pods(), context_name).scoped_to(namespaces);
        panel_handle(cx.new(|cx| PodsPanel::new(scope, cx)))
    });
}

/// The pod a Logs panel should stream, set by clicking a row in a Pods
/// panel. App-scoped rather than a direct link between the two panels, since
/// either can live in any dock split of any window.
#[derive(Debug, Clone, PartialEq)]
pub struct PodSelection {
    pub namespace: String,
    pub name: String,
    pub containers: Vec<String>,
}

#[derive(Default)]
pub struct SelectedPod(pub Option<PodSelection>);

impl Global for SelectedPod {}

/// Tells a Pods table which pod its own row click picked, so a later sort or
/// row update can move the highlight with that pod (`pods_table::reselect`).
fn remember_selection<V>(
    table: &Entity<TableState<PodTableDelegate>>,
    selection: &PodSelection,
    cx: &mut Context<V>,
) {
    let selection = selection.clone();
    table.update(cx, |table, _| {
        table.delegate_mut().remember_selection(Some(selection));
    });
}

/// A dock panel connecting to its scope's cluster and rendering its live,
/// all-namespaces Pods table.
pub struct PodsPanel {
    scope: PanelScope,
    connection: Entity<crate::k8s::cluster::connection::ClusterConnection>,
    table: Entity<PodsTable>,
    namespaces: Entity<crate::k8s::cluster::namespaces::NamespaceList>,
    subscribed: bool,
    focus_handle: FocusHandle,
    pod_table: Option<Entity<TableState<PodTableDelegate>>>,
}

impl PodsPanel {
    pub fn new(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let context_name = scope.context_name.clone();
        let connection = ClusterRegistry::connection(cx, &context_name);
        let namespaces =
            crate::k8s::cluster::namespaces::NamespaceRegistry::list(cx, &context_name);
        Self::with_connection(scope, connection, namespaces, cx)
    }

    /// Construction from an explicit connection and namespace list, so tests
    /// can hand in stubs. [`Self::new`] has to source both from their
    /// registries, which starts a *real* connect - a tokio task gpui's test
    /// scheduler reports as cross-thread nondeterminism if it is still in
    /// flight when the test ends. The handler tests never need that, so they
    /// pass a connection that stays in a non-`Connected` state.
    fn with_connection(
        scope: PanelScope,
        connection: Entity<crate::k8s::cluster::connection::ClusterConnection>,
        namespaces: Entity<crate::k8s::cluster::namespaces::NamespaceList>,
        cx: &mut Context<Self>,
    ) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let context_name = scope.context_name.clone();
        cx.observe(&connection, |this: &mut Self, connection, cx| {
            this.start_watch_if_connected(&connection, cx);
            cx.notify();
        })
        .detach();
        cx.observe(&namespaces, |_, _, cx| cx.notify()).detach();
        cx.on_release({
            let context_name = context_name.clone();
            move |this: &mut Self, cx| {
                if this.subscribed {
                    ClusterRegistry::unsubscribe_pods(cx, &context_name);
                }
            }
        })
        .detach();

        let mut this = Self {
            scope,
            connection: connection.clone(),
            table: cx.new(|_| PodsTable::default()),
            namespaces,
            subscribed: false,
            focus_handle: cx.focus_handle(),
            pod_table: None,
        };
        this.start_watch_if_connected(&connection, cx);
        this
    }

    /// A panel over stub cluster state: the connection never leaves
    /// `Connecting`, so no watch, discovery, or namespace list is started.
    /// For tests that exercise the panel's own behaviour rather than a live
    /// cluster.
    #[cfg(test)]
    fn with_stubs(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
        use crate::k8s::cluster::namespaces::NamespaceList;

        let connection =
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting));
        let namespaces = cx.new(|_| NamespaceList::empty());
        Self::with_connection(scope, connection, namespaces, cx)
    }

    fn start_watch_if_connected(
        &mut self,
        connection: &Entity<crate::k8s::cluster::connection::ClusterConnection>,
        cx: &mut Context<Self>,
    ) {
        use crate::k8s::cluster::session::ClusterRegistry;

        if self.subscribed {
            return;
        }
        let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
            &connection.read(cx).state
        else {
            return;
        };
        let client = client.clone();
        self.table = ClusterRegistry::subscribe_pods(cx, &self.scope.context_name, client);
        cx.observe(&self.table, |_, _, cx| cx.notify()).detach();
        self.subscribed = true;
    }

    /// The pod the table's last row-selection chose, from the app-scoped
    /// global `ShowLogs` also reads. The panel keeps no copy of its own: the
    /// selection is a window-wide fact, and the rows it indexes come and go
    /// with the watch.
    fn selected(cx: &App) -> Option<PodSelection> {
        cx.try_global::<SelectedPod>()
            .and_then(|selected| selected.0.clone())
    }

    fn on_action_warp_namespace(
        &mut self,
        _: &WarpNamespace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(namespace) = Self::selected(cx).map(|selection| selection.namespace) else {
            return;
        };
        self.scope = self.scope.scoped_to(vec![namespace.clone()]);
        cx.emit(ScopeEvent::NamespacesChanged(vec![namespace]));
    }

    /// `DescribePod` (`d`) and the row context menu's "Open" both ask for the
    /// pod's detail panel on the structured field view.
    fn on_action_describe_pod(
        &mut self,
        _: &DescribePod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Self::selected(cx).is_some() {
            window.dispatch_action(Box::new(crate::ui::nav::ShowPodDetail), cx);
        }
    }

    fn on_action_show_pod_logs(
        &mut self,
        _: &ShowPodLogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Self::selected(cx).is_some() {
            window.dispatch_action(Box::new(crate::ui::nav::ShowLogs), cx);
        }
    }

    /// `ShowPodYaml` (`y`) asks for the same panel as `d`, on the other view.
    /// It is a distinct app-level action rather than the same one, because `y`
    /// means "the YAML, now": routing it through `ShowPodDetail` would open
    /// the field list and leave the shortcut's second half to the user.
    fn on_action_show_pod_yaml(
        &mut self,
        _: &ShowPodYaml,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Self::selected(cx).is_some() {
            window.dispatch_action(Box::new(crate::ui::nav::ShowPodDetailYaml), cx);
        }
    }

    fn sync_table(
        &mut self,
        rows: Vec<PodTableRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TableState<PodTableDelegate>> {
        if self.pod_table.is_none() {
            let table = cx.new(|cx| {
                TableState::new(PodTableDelegate::default(), window, cx)
                    .row_selectable(true)
                    .col_selectable(false)
                    .sortable(true)
                    .col_movable(true)
                    .col_resizable(true)
            });
            cx.subscribe_in(&table, window, |_this, table, event, window, cx| {
                let row_ix = match event {
                    TableEvent::SelectRow(row_ix) => *row_ix,
                    // A single click only selects (drives WarpNamespace/
                    // ShowPodLogs, which read SelectedPod); opening the
                    // detail panel is the double-click, matching the
                    // resource panel's own row convention (section 9.1).
                    TableEvent::DoubleClickedRow(row_ix) => {
                        let Some(selection) = table
                            .read(cx)
                            .delegate()
                            .rows()
                            .get(*row_ix)
                            .map(|row| row.selection.clone())
                        else {
                            return;
                        };
                        remember_selection(table, &selection, cx);
                        cx.set_global(SelectedPod(Some(selection)));
                        window.dispatch_action(Box::new(crate::ui::nav::ShowPodDetail), cx);
                        return;
                    }
                    _ => return,
                };
                let Some(selection) = table
                    .read(cx)
                    .delegate()
                    .rows()
                    .get(row_ix)
                    .map(|row| row.selection.clone())
                else {
                    return;
                };
                remember_selection(table, &selection, cx);
                cx.set_global(SelectedPod(Some(selection)));
                cx.notify();
            })
            .detach();
            self.pod_table = Some(table);
        }
        let table = self.pod_table.as_ref().unwrap().clone();
        table.update(cx, |table, cx| {
            table.delegate_mut().set_rows(rows);
            // A row update can move or drop the pod at the previously
            // selected index (see `pods_table::reselect`'s doc comment) -
            // called directly, not deferred, since `table` here is already
            // the full `&mut TableState`, unlike inside `perform_sort`.
            pods_table::reselect(table, cx);
            cx.notify();
        });
        table
    }
}

impl Focusable for PodsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PodsPanel {}
impl EventEmitter<ScopeEvent> for PodsPanel {}

impl Render for PodsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::k8s::cluster::connection::ConnectionState;

        let content = match &self.connection.read(cx).state {
            ConnectionState::Connecting => div().size_full().p_3().child("Connecting..."),
            ConnectionState::WaitingForTunnel => {
                div().size_full().p_3().child("Waiting for tunnel...")
            }
            ConnectionState::Failed(reason) => div()
                .size_full()
                .p_3()
                .child(format!("Connection failed: {reason}")),
            ConnectionState::Connected(_) => {
                use crate::k8s::cluster::session::ClusterRegistry;
                use crate::k8s::cluster::watch_registry::PauseReason;

                let pause_banner = ClusterRegistry::pods_pause_info(cx, &self.scope.context_name)
                    .map(|(reason, elapsed)| {
                        let reason = match reason {
                            PauseReason::Reconnecting => "tunnel reconnecting",
                            PauseReason::CredentialRefresh => "refreshing credentials",
                        };
                        div().child(format!(
                            "Paused ({reason}) - {} ago",
                            format_age(elapsed.as_secs() as i64)
                        ))
                    });

                let now = Timestamp::now();
                let namespaces = &self.scope.namespaces;
                let items: Vec<PodTableRow> = self
                    .table
                    .read(cx)
                    .pods()
                    .iter()
                    .filter(|pod| matches_namespaces(pod, namespaces))
                    .map(|pod| {
                        let containers = pod
                            .spec
                            .as_ref()
                            .map(|spec| spec.containers.iter().map(|c| c.name.clone()).collect())
                            .unwrap_or_default();
                        let selection = PodSelection {
                            namespace: pod.metadata.namespace.clone().unwrap_or_default(),
                            name: pod.metadata.name.clone().unwrap_or_default(),
                            containers,
                        };
                        PodTableRow {
                            row: pod_row(pod, now),
                            selection,
                        }
                    })
                    .collect();
                let table = self.sync_table(items, window, cx);
                let namespace_key =
                    Kbd::binding_for_action(&WarpNamespace, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse(NAMESPACE_KEY).expect("valid keybinding"))
                        });
                let describe_key =
                    Kbd::binding_for_action(&DescribePod, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse(DESCRIBE_KEY).expect("valid keybinding"))
                        });
                let logs_key =
                    Kbd::binding_for_action(&ShowPodLogs, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse(LOGS_KEY).expect("valid keybinding"))
                        });
                let yaml_key =
                    Kbd::binding_for_action(&ShowPodYaml, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse(YAML_KEY).expect("valid keybinding"))
                        });
                let shortcuts = div()
                    .flex()
                    .gap_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .child(namespace_key)
                            .child("Namespace"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .child(describe_key)
                            .child("Describe"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .child(logs_key)
                            .child("Logs"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .child(yaml_key)
                            .child("YAML"),
                    );
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .p_3()
                    .children(pause_banner)
                    .child(
                        div().flex_1().min_h_0().child(
                            DataTable::new(&table)
                                .stripe(true)
                                .bordered(true)
                                .scrollbar_visible(true, true),
                        ),
                    )
                    .child(div().mt_2().child(shortcuts))
            }
        };

        let this = cx.weak_entity();
        let namespaces = self.namespaces.read(cx).names();
        let namespace_bar =
            panel_title::namespace_picker(&self.scope, namespaces, move |namespaces, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.scope = this.scope.scoped_to(namespaces.clone());
                    cx.emit(ScopeEvent::NamespacesChanged(namespaces));
                });
            })
            .map(|picker| {
                div()
                    .flex()
                    .justify_end()
                    .p_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(picker)
            });

        div()
            .size_full()
            .key_context(PANEL_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_warp_namespace))
            .on_action(cx.listener(Self::on_action_describe_pod))
            .on_action(cx.listener(Self::on_action_show_pod_logs))
            .on_action(cx.listener(Self::on_action_show_pod_yaml))
            .child(panel_title::focus_frame(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .children(namespace_bar)
                    .child(div().flex_1().min_h_0().child(content)),
                &self.focus_handle,
                window,
                cx,
            ))
    }
}

impl BasePanel for PodsPanel {
    fn panel_name(&self) -> &'static str {
        "Pods"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": self.scope.context_name,
                "namespaces": self.scope.namespaces,
            })),
        }
    }
}

/// Section 10: the title bar, supplied to the dock rather than drawn here, so
/// this panel and a placeholder get an identical bar.
impl Panel for PodsPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title(&self.scope)
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn toolbar_buttons(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Vec<Button>> {
        panel_title::toolbar_buttons()
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: `gpui_kit::*` (imported above for `PodsPanel`)
    // re-exports its own `test` attribute macro, which would shadow
    // `core::prelude::v1::test` for these plain synchronous tests.
    use super::{
        DescribePod, NamespaceScope, PanelScope, Pod, PodRow, PodSelection, PodsPanel, PodsTable,
        SelectedPod, ShowPodYaml, SortState, is_unauthorized, matches_namespaces, pod_row,
        view_rows, watcher,
    };
    use crate::k8s::resource::pod_detail::DetailView;
    use crate::ui::nav::NavTarget;
    use gpui_kit::{AppContext as _, InteractiveElement as _, ParentElement as _, Styled as _};
    use jiff::Timestamp;

    use k8s_openapi::api::core::v1::{ContainerStatus, PodStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};
    use kube::core::response::Status;

    struct PanelHarness {
        first: gpui_kit::Entity<PodsPanel>,
        second: gpui_kit::Entity<PodsPanel>,
        /// Counts the app-level detail requests that bubble past both panels
        /// to the window root - i.e. the requests the panels actually emitted,
        /// kept apart by view so a test can tell `d` from `y`.
        dispatches: std::rc::Rc<std::cell::RefCell<Vec<DetailView>>>,
    }

    impl gpui_kit::Render for PanelHarness {
        fn render(
            &mut self,
            _window: &mut gpui_kit::Window,
            _cx: &mut gpui_kit::Context<Self>,
        ) -> impl gpui_kit::IntoElement {
            let dispatches = self.dispatches.clone();
            let structured = dispatches.clone();
            let yaml = dispatches.clone();
            gpui_kit::div()
                .size_full()
                .on_action(move |_: &crate::ui::nav::ShowPodDetail, _window, _cx| {
                    structured.borrow_mut().push(DetailView::Structured);
                })
                .on_action(move |_: &crate::ui::nav::ShowPodDetailYaml, _window, _cx| {
                    yaml.borrow_mut().push(DetailView::Yaml);
                })
                .child(self.first.clone())
                .child(self.second.clone())
        }
    }

    fn api_error(code: u16) -> kube::Error {
        kube::Error::Api(Box::new(Status {
            code,
            ..Default::default()
        }))
    }

    #[test]
    fn watch_failed_401_is_unauthorized() {
        assert!(is_unauthorized(&watcher::Error::WatchFailed(api_error(
            401
        ))));
    }

    #[test]
    fn initial_list_failed_401_is_unauthorized() {
        assert!(is_unauthorized(&watcher::Error::InitialListFailed(
            api_error(401)
        )));
    }

    #[test]
    fn a_403_is_not_unauthorized() {
        assert!(!is_unauthorized(&watcher::Error::WatchFailed(api_error(
            403
        ))));
    }

    #[test]
    fn no_resource_version_is_not_unauthorized() {
        assert!(!is_unauthorized(&watcher::Error::NoResourceVersion));
    }

    fn pod(uid: &str, name: &str) -> Pod {
        pod_in("default", uid, name, 0)
    }

    fn pod_in(namespace: &str, uid: &str, name: &str, created_at_secs: i64) -> Pod {
        Pod {
            metadata: ObjectMeta {
                uid: Some(uid.into()),
                name: Some(name.into()),
                namespace: Some(namespace.into()),
                creation_timestamp: Some(Time(Timestamp::from_second(created_at_secs).unwrap())),
                ..Default::default()
            },
            status: Some(PodStatus {
                phase: Some("Running".into()),
                container_statuses: Some(vec![ContainerStatus {
                    name: "app".into(),
                    ready: true,
                    restart_count: 2,
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn pod_row_reports_ready_status_restarts_and_age() {
        let now = Timestamp::from_second(90).unwrap();
        let row = pod_row(&pod("u1", "web-1"), now);

        assert_eq!(row.name, "web-1");
        assert_eq!(row.namespace, "default");
        assert_eq!(row.ready, "1/1");
        assert_eq!(row.status, "Running");
        assert_eq!(row.restarts, 2);
        assert_eq!(row.age, "1m");
    }

    #[test]
    fn initial_stream_populates_rows() {
        let mut table = PodsTable::new();

        table.apply(watcher::Event::Init);
        table.apply(watcher::Event::InitApply(pod("u1", "web-1")));
        table.apply(watcher::Event::InitApply(pod("u2", "web-2")));
        table.apply(watcher::Event::InitDone);

        assert_eq!(table.pods().len(), 2);
    }

    #[test]
    fn later_apply_and_delete_update_rows_within_the_drain_cycle() {
        let mut table = PodsTable::new();
        table.apply(watcher::Event::InitApply(pod("u1", "web-1")));
        table.apply(watcher::Event::InitApply(pod("u2", "web-2")));

        table.apply(watcher::Event::Apply(pod("u3", "web-3")));
        assert_eq!(table.pods().len(), 3);

        table.apply(watcher::Event::Delete(pod("u1", "web-1")));
        assert_eq!(table.pods().len(), 2);
        assert!(
            table
                .pods()
                .iter()
                .all(|p| p.metadata.uid.as_deref() != Some("u1"))
        );
    }

    fn mixed_namespace_fixture() -> Vec<Pod> {
        vec![
            pod_in("default", "u1", "web-1", 0),
            pod_in("kube-system", "u2", "coredns-1", 0),
            pod_in("kube-system", "u3", "kube-proxy-1", 0),
        ]
    }

    fn default_sort() -> SortState {
        SortState {
            column: "name".into(),
            ascending: true,
        }
    }

    #[test]
    fn single_namespace_scope_shows_only_its_pods() {
        let pods = mixed_namespace_fixture();
        let now = Timestamp::from_second(0).unwrap();

        let rows = view_rows(
            &pods,
            now,
            &NamespaceScope::Single("kube-system".into()),
            "",
            &default_sort(),
        );

        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.namespace == "kube-system"));
    }

    #[test]
    fn all_namespace_scope_shows_every_pod() {
        let pods = mixed_namespace_fixture();
        let now = Timestamp::from_second(0).unwrap();

        let rows = view_rows(&pods, now, &NamespaceScope::All, "", &default_sort());

        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn multiple_namespace_scope_includes_each_selected_namespace() {
        let pods = mixed_namespace_fixture();
        let namespaces = vec!["default".to_string(), "kube-system".to_string()];
        let selected: Vec<&Pod> = pods
            .iter()
            .filter(|pod| matches_namespaces(pod, &namespaces))
            .collect();
        assert_eq!(selected.len(), 3);
        assert!(!matches_namespaces(
            &pod_in("other", "u4", "ignored", 0),
            &namespaces
        ));
    }

    #[test]
    fn name_filter_hides_non_matching_rows_and_clearing_restores_them() {
        let pods = vec![
            pod("u1", "nginx-1"),
            pod("u2", "nginx-2"),
            pod("u3", "web-1"),
        ];
        let now = Timestamp::from_second(0).unwrap();

        let filtered = view_rows(&pods, now, &NamespaceScope::All, "nginx", &default_sort());
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|r| r.name.contains("nginx")));

        let cleared = view_rows(&pods, now, &NamespaceScope::All, "", &default_sort());
        assert_eq!(cleared.len(), 3);
    }

    #[test]
    fn age_sort_toggles_direction() {
        let pods = vec![
            pod_in("default", "u1", "oldest", 0),
            pod_in("default", "u2", "middle", 100),
            pod_in("default", "u3", "newest", 200),
        ];
        let now = Timestamp::from_second(1000).unwrap();

        let ascending = view_rows(
            &pods,
            now,
            &NamespaceScope::All,
            "",
            &SortState {
                column: "age".into(),
                ascending: true,
            },
        );
        let names: Vec<_> = ascending.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["newest", "middle", "oldest"]);

        let descending = view_rows(
            &pods,
            now,
            &NamespaceScope::All,
            "",
            &SortState {
                column: "age".into(),
                ascending: false,
            },
        );
        let names: Vec<_> = descending.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["oldest", "middle", "newest"]);
    }

    #[test]
    fn reconnect_converges_to_the_post_interruption_state() {
        let mut table = PodsTable::new();

        // Initial connection: three pods.
        table.apply(watcher::Event::Init);
        table.apply(watcher::Event::InitApply(pod("u1", "web-1")));
        table.apply(watcher::Event::InitApply(pod("u2", "web-2")));
        table.apply(watcher::Event::InitApply(pod("u3", "web-3")));
        table.apply(watcher::Event::InitDone);
        assert_eq!(table.pods().len(), 3);

        // Stream interrupted and reconnects: web-1 was deleted while
        // disconnected (no explicit Delete event ever arrives for it), and
        // web-4 appeared. The relist only re-sends what's there now.
        table.apply(watcher::Event::Init);
        table.apply(watcher::Event::InitApply(pod("u2", "web-2")));
        table.apply(watcher::Event::InitApply(pod("u3", "web-3")));
        table.apply(watcher::Event::InitApply(pod("u4", "web-4")));
        table.apply(watcher::Event::InitDone);

        let mut names: Vec<_> = table
            .pods()
            .iter()
            .map(|p| p.metadata.name.clone().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, vec!["web-2", "web-3", "web-4"]);
    }

    /// `d`/`y` are panel-scoped: only the focused panel's handler runs, and it
    /// forwards an app-level detail request to the window. This pins all three
    /// halves - the focused panel asks once, it asks only when a pod is
    /// selected, and each shortcut asks for its own view rather than both
    /// asking for the same one.
    #[gpui_kit::test]
    async fn pod_shortcut_dispatches_only_to_the_focused_panel(cx: &mut gpui_kit::TestAppContext) {
        use std::cell::RefCell;
        use std::rc::Rc;

        let dispatches = Rc::new(RefCell::new(Vec::new()));
        let (first, second) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            cx.set_global(SelectedPod(Some(PodSelection {
                namespace: "default".into(),
                name: "web-1".into(),
                containers: vec!["web".into()],
            })));
            let first = cx.new(|cx| {
                PodsPanel::with_stubs(PanelScope::new(NavTarget::pods(), "dev".into()), cx)
            });
            let second = cx.new(|cx| {
                PodsPanel::with_stubs(PanelScope::new(NavTarget::pods(), "prod".into()), cx)
            });
            (first, second)
        });
        let window = cx.add_window({
            let dispatches = dispatches.clone();
            let harness_first = first.clone();
            let harness_second = second.clone();
            move |_, _| PanelHarness {
                first: harness_first.clone(),
                second: harness_second.clone(),
                dispatches: dispatches.clone(),
            }
        });
        cx.run_until_parked();

        window
            .update(cx, |_, window, cx| {
                let focus_handle = first.read(cx).focus_handle.clone();
                focus_handle.focus(window, cx);
                window.dispatch_action(Box::new(ShowPodYaml), cx);
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(
            *dispatches.borrow(),
            vec![DetailView::Yaml],
            "only the focused panel forwards the request, and `y` asks for the \
             YAML rather than the panel's default view"
        );

        // `d` is the same panel by a different route, so it asks for the field
        // list. If both shortcuts emitted one action there would be nothing
        // left to distinguish them at the far end.
        dispatches.borrow_mut().clear();
        window
            .update(cx, |_, window, cx| {
                let focus_handle = first.read(cx).focus_handle.clone();
                focus_handle.focus(window, cx);
                window.dispatch_action(Box::new(DescribePod), cx);
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(
            *dispatches.borrow(),
            vec![DetailView::Structured],
            "`d` asks for the structured view"
        );

        // No selection means nothing to open, so the shortcut is inert rather
        // than opening a detail panel for whatever was selected last.
        cx.update(|cx| cx.set_global(SelectedPod(None)));
        dispatches.borrow_mut().clear();
        window
            .update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(ShowPodYaml), cx);
            })
            .unwrap();
        cx.run_until_parked();
        assert!(
            dispatches.borrow().is_empty(),
            "an unselected pod opens nothing"
        );
    }
    /// Every key the hint bar prints resolves to the action it names, in the
    /// panel's context. The hint bar reads the keymap and falls back to a
    /// literal, so a binding naming the wrong action, or leaving the context,
    /// is invisible until a keystroke does the wrong thing.
    ///
    /// At the tail of the module, with its imports local, so it neither
    /// depends on nor disturbs the shared import block above.
    #[test]
    fn panel_bindings_pair_each_key_with_the_action_the_hint_bar_names() {
        use super::{ShowPodLogs, WarpNamespace, panel_bindings};
        use gpui_kit::AsKeystroke as _;

        let bindings = panel_bindings();
        let actual: Vec<(String, &dyn gpui_kit::Action)> = bindings
            .iter()
            .map(|binding| {
                // The keystroke as typed, not as displayed: the hint bar prints
                // the platform's upper-case key form ("W"), but the binding has
                // to answer to "w".
                let keys: Vec<String> = binding
                    .keystrokes()
                    .iter()
                    .map(|key| key.as_keystroke().key.clone())
                    .collect();
                assert_eq!(
                    keys.len(),
                    1,
                    "a panel shortcut should be a single key, got {keys:?}"
                );
                (keys[0].clone(), binding.action())
            })
            .collect();
        let expected: Vec<(String, &dyn gpui_kit::Action)> = vec![
            ("w".to_string(), &WarpNamespace),
            ("d".to_string(), &DescribePod),
            ("l".to_string(), &ShowPodLogs),
            ("y".to_string(), &ShowPodYaml),
        ];
        assert_eq!(
            actual
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>()
        );
        for (index, (key, action)) in actual.iter().enumerate() {
            assert!(
                action.partial_eq(expected[index].1),
                "key `{key}` is bound to the wrong action"
            );
        }
    }

    /// The shortcut has to resolve with a *child* scope on the focus path, not
    /// only when the panel itself holds focus.
    ///
    /// Selecting a row hands focus to the table inside the panel, and that is
    /// the case the shortcut broke on: the panel's own handle is no longer the
    /// focused element, so a binding scoped too tightly, or a listener that
    /// only the panel answers, would both look correct until someone clicked a
    /// row. A context binding matches at the depth of the focused context, so
    /// the panel's bindings must come back for a stack that ends in the table.
    #[test]
    fn panel_bindings_resolve_beneath_a_focused_child_scope() {
        use super::{DescribePod, PANEL_KEY_CONTEXT, panel_bindings};
        use gpui_kit::{KeyContext, Keymap, Keystroke};

        /// A one-element input slice, borrowed: the API takes a slice so a key
        /// can be mid-chord, and cloning a keystroke to satisfy that is noise.
        fn slice(key: &Keystroke) -> &[Keystroke] {
            std::slice::from_ref(key)
        }

        let keymap = Keymap::new(panel_bindings().to_vec());
        let panel = KeyContext::try_from(PANEL_KEY_CONTEXT).expect("a valid context name");
        let table = KeyContext::try_from("DataTable").expect("a valid context name");
        let d = Keystroke::parse("d").expect("a valid keystroke");
        let one = |context: &KeyContext| [context.clone()];

        let with_panel_focused = keymap.bindings_for_input(slice(&d), &one(&panel));
        let with_table_focused = keymap.bindings_for_input(slice(&d), &[panel, table]);

        for (depth, (matched, _)) in [("panel", with_panel_focused), ("table", with_table_focused)]
        {
            assert!(
                matched
                    .iter()
                    .any(|binding| binding.action().partial_eq(&DescribePod)),
                "`d` does not resolve to DescribePod with the {depth} scope focused"
            );
        }

        // And it stops resolving once the panel is off the path, which is what
        // makes these the panel's keys rather than the window's: with the logs
        // panel focused, `d` is nobody's shortcut. A binding registered without
        // a context would fail here, having passed the two cases above.
        let logs = KeyContext::try_from("LogsPanel").expect("a valid context name");
        let (elsewhere, _) = keymap.bindings_for_input(slice(&d), &one(&logs));
        assert!(
            !elsewhere
                .iter()
                .any(|binding| binding.action().partial_eq(&DescribePod)),
            "`d` still opens a pod's detail from another panel, so the binding \
             is not scoped to the pods panel"
        );
    }

    /// Four rows with a distinct, non-alphabetical value in every column, so
    /// sorting by any one of them actually reorders the set rather than
    /// leaving it looking like the input by coincidence.
    fn pod_table_rows_fixture() -> Vec<crate::k8s::resource::pods_table::PodTableRow> {
        use crate::k8s::resource::pods_table::PodTableRow;

        [
            (
                "b-name", "ns-c", "1/2", "Running", 3, 300, "10.0.0.3", "node-c",
            ),
            (
                "d-name", "ns-a", "2/2", "Pending", 1, 100, "10.0.0.1", "node-a",
            ),
            (
                "a-name", "ns-d", "0/2", "Failed", 4, 400, "10.0.0.4", "node-d",
            ),
            (
                "c-name",
                "ns-b",
                "3/3",
                "Succeeded",
                2,
                200,
                "10.0.0.2",
                "node-b",
            ),
        ]
        .into_iter()
        .map(
            |(name, namespace, ready, status, restarts, age_secs, ip, node)| PodTableRow {
                row: PodRow {
                    name: name.into(),
                    namespace: namespace.into(),
                    ready: ready.into(),
                    status: status.into(),
                    restarts,
                    age: String::new(),
                    pod_ip: ip.into(),
                    node: node.into(),
                    age_secs,
                },
                selection: PodSelection {
                    namespace: namespace.into(),
                    name: name.into(),
                    containers: Vec::new(),
                },
            },
        )
        .collect()
    }

    /// The pure per-column comparator `PodTableDelegate::resort` and
    /// `sort_rows` both build on: this pins the column-to-field mapping for
    /// every column, Ip and Node included, directly - each is exercised
    /// against a pair of rows where it alone determines the order.
    #[test]
    fn compare_orders_rows_by_every_column() {
        use crate::k8s::resource::pods_table::{PodColumn, compare};
        use std::cmp::Ordering;

        // Every field of `a` sorts before the matching field of `b`, so each
        // column's comparator can be checked against the same pair.
        let a = PodRow {
            name: "a-name".into(),
            namespace: "ns-a".into(),
            ready: "0/2".into(),
            status: "Failed".into(),
            restarts: 1,
            age: String::new(),
            pod_ip: "10.0.0.1".into(),
            node: "node-a".into(),
            age_secs: 100,
        };
        let b = PodRow {
            name: "b-name".into(),
            namespace: "ns-b".into(),
            ready: "1/2".into(),
            status: "Running".into(),
            restarts: 2,
            age: String::new(),
            pod_ip: "10.0.0.2".into(),
            node: "node-b".into(),
            age_secs: 200,
        };

        for col in PodColumn::DEFAULT_ORDER {
            assert_eq!(
                compare(&a, &b, col),
                Ordering::Less,
                "{col:?} should order `a` before `b`"
            );
            assert_eq!(
                compare(&b, &a, col),
                Ordering::Greater,
                "{col:?} should order `b` after `a`"
            );
        }
    }

    /// `PodTableDelegate::resort` mirrors the header-click cycle
    /// (`Default -> Descending -> Ascending -> Default`, see
    /// `TableState::perform_sort`): each direction reorders `rows` by that
    /// column, and cycling back to `Default` restores the order rows were
    /// last supplied in - not merely whatever order sorting happened to leave
    /// them in - for every sortable column.
    #[test]
    fn resort_orders_rows_and_default_restores_the_supplied_order() {
        use crate::k8s::resource::pods_table::{PodColumn, PodTableDelegate, compare};
        use gpui_kit::component::table::ColumnSort;

        fn names(rows: &[crate::k8s::resource::pods_table::PodTableRow]) -> Vec<&str> {
            rows.iter().map(|r| r.row.name.as_str()).collect()
        }

        for (col_ix, col) in PodColumn::DEFAULT_ORDER.into_iter().enumerate() {
            let natural = pod_table_rows_fixture();
            let mut delegate = PodTableDelegate::default();
            delegate.set_rows(natural.clone());

            delegate.resort(col_ix, ColumnSort::Descending);
            let mut want_desc = natural.clone();
            want_desc.sort_by(|a, b| compare(&b.row, &a.row, col));
            assert_eq!(
                names(delegate.rows()),
                names(&want_desc),
                "{col:?} descending"
            );

            delegate.resort(col_ix, ColumnSort::Ascending);
            let mut want_asc = natural.clone();
            want_asc.sort_by(|a, b| compare(&a.row, &b.row, col));
            assert_eq!(
                names(delegate.rows()),
                names(&want_asc),
                "{col:?} ascending"
            );

            delegate.resort(col_ix, ColumnSort::Default);
            assert_eq!(
                names(delegate.rows()),
                names(&natural),
                "{col:?} default should restore the supplied order"
            );
        }
    }

    /// A rows-only update - what `PodsPanel::sync_table` does every render -
    /// must not undo an active sort: `set_rows` is `sync_table`'s call, and
    /// unlike a full `TableState::refresh()` it has to keep applying the
    /// sort that was active before the new rows arrived.
    #[test]
    fn set_rows_keeps_an_active_sort_applied() {
        use crate::k8s::resource::pods_table::{PodTableDelegate, compare};
        use gpui_kit::component::table::ColumnSort;

        let mut delegate = PodTableDelegate::default();
        delegate.set_rows(pod_table_rows_fixture());
        delegate.resort(0, ColumnSort::Ascending); // Name, ascending.

        // A fresh row arrives in an arbitrary position, as a live watch
        // update would deliver it - not already in sorted order.
        let mut updated = pod_table_rows_fixture();
        updated.insert(
            1,
            crate::k8s::resource::pods_table::PodTableRow {
                row: PodRow {
                    name: "aa-name".into(),
                    namespace: "ns-e".into(),
                    ready: "1/1".into(),
                    status: "Running".into(),
                    restarts: 0,
                    age: String::new(),
                    pod_ip: "10.0.0.5".into(),
                    node: "node-e".into(),
                    age_secs: 50,
                },
                selection: PodSelection {
                    namespace: "ns-e".into(),
                    name: "aa-name".into(),
                    containers: Vec::new(),
                },
            },
        );
        delegate.set_rows(updated.clone());

        let mut want = updated;
        want.sort_by(|a, b| {
            compare(
                &a.row,
                &b.row,
                crate::k8s::resource::pods_table::PodColumn::Name,
            )
        });
        let got: Vec<&str> = delegate
            .rows()
            .iter()
            .map(|r| r.row.name.as_str())
            .collect();
        let want_names: Vec<&str> = want.iter().map(|r| r.row.name.as_str()).collect();
        assert_eq!(
            got, want_names,
            "the sort applied before the row update still holds"
        );
    }

    /// After two column moves, the lookup `render_td` renders cells from
    /// (`cell_text_at`) reads each visual position's *own* column - not the
    /// column that used to sit there before the moves.
    #[test]
    fn moving_columns_renders_each_visual_position_from_its_own_column() {
        use crate::k8s::resource::pods_table::PodTableDelegate;

        let mut delegate = PodTableDelegate::default();
        delegate.set_rows(vec![crate::k8s::resource::pods_table::PodTableRow {
            row: PodRow {
                name: "web-1".into(),
                namespace: "default".into(),
                ready: "1/1".into(),
                status: "Running".into(),
                restarts: 0,
                age: "1m".into(),
                pod_ip: "10.0.0.9".into(),
                node: "node-z".into(),
                age_secs: 60,
            },
            selection: PodSelection {
                namespace: "default".into(),
                name: "web-1".into(),
                containers: Vec::new(),
            },
        }]);

        // Default order: [Name, Namespace, Ready, Status, Restarts, Age, Ip, Node].
        delegate.reorder_columns(7, 0); // Node to the front.
        delegate.reorder_columns(2, 0); // Namespace (now at index 2) to the front.
        // Now: [Namespace, Node, Name, Ready, Status, Restarts, Age, Ip].
        assert_eq!(
            delegate.cell_text_at(0, 0),
            "default",
            "col 0 is now Namespace"
        );
        assert_eq!(delegate.cell_text_at(0, 1), "node-z", "col 1 is now Node");
        assert_eq!(delegate.cell_text_at(0, 2), "web-1", "col 2 is now Name");
    }

    /// `column(ix)` marks every column sortable, but only the active column's
    /// direction survives - the other seven read back `ColumnSort::Default`
    /// even while one column is actively sorted, which is what lets a future
    /// `TableState::refresh()` redraw the same single indicator.
    #[gpui_kit::test]
    async fn column_reports_the_active_sort_on_the_active_column_only(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use crate::k8s::resource::pods_table::PodTableDelegate;
        use gpui_kit::component::table::{ColumnSort, TableDelegate as _};

        cx.update(|cx| {
            gpui_kit::init(cx);
            let mut delegate = PodTableDelegate::default();
            delegate.resort(2, ColumnSort::Descending);

            for col_ix in 0..delegate.columns_count(cx) {
                let column = delegate.column(col_ix, cx);
                if col_ix == 2 {
                    assert_eq!(column.sort, Some(ColumnSort::Descending));
                } else {
                    assert_eq!(
                        column.sort,
                        Some(ColumnSort::Default),
                        "column {col_ix} should not report the active column's sort"
                    );
                }
            }
        });
    }

    /// The same sort-survives-a-row-update guarantee as
    /// `set_rows_keeps_an_active_sort_applied`, but driven through a real
    /// `TableState<PodTableDelegate>` in a window - the same object
    /// `PodsPanel::sync_table` drives - rather than the delegate alone.
    #[gpui_kit::test]
    async fn a_row_update_keeps_a_real_table_states_rows_sorted(cx: &mut gpui_kit::TestAppContext) {
        use crate::k8s::resource::pods_table::PodTableDelegate;
        use gpui_kit::component::table::{ColumnSort, TableState};

        cx.update(gpui_kit::init);
        let window = cx.add_window(|window, cx| {
            TableState::new(PodTableDelegate::default(), window, cx)
                .sortable(true)
                .col_movable(true)
                .col_resizable(true)
        });

        window
            .update(cx, |table, _window, cx| {
                table.delegate_mut().set_rows(pod_table_rows_fixture());
                table.delegate_mut().resort(0, ColumnSort::Ascending);
                cx.notify();
            })
            .unwrap();

        window
            .update(cx, |table, _window, cx| {
                table.delegate_mut().set_rows(pod_table_rows_fixture());
                cx.notify();
            })
            .unwrap();

        let names: Vec<String> = window
            .update(cx, |table, _window, _cx| {
                table
                    .delegate()
                    .rows()
                    .iter()
                    .map(|r| r.row.name.clone())
                    .collect()
            })
            .unwrap();
        assert_eq!(
            names,
            vec!["a-name", "b-name", "c-name", "d-name"],
            "the sort applied before the row update still holds"
        );
    }

    /// `PodTableDelegate::index_of` matches by namespace and name, not
    /// position - the lookup `reselect` relies on to survive a sort or a row
    /// update reordering `rows` underneath it.
    #[test]
    fn index_of_finds_the_row_by_namespace_and_name() {
        use crate::k8s::resource::pods_table::PodTableDelegate;

        let mut delegate = PodTableDelegate::default();
        delegate.set_rows(pod_table_rows_fixture());

        assert_eq!(
            delegate.index_of(&PodSelection {
                namespace: "ns-a".into(),
                name: "d-name".into(),
                containers: Vec::new(),
            }),
            Some(1),
        );
        assert_eq!(
            delegate.index_of(&PodSelection {
                namespace: "ns-a".into(),
                name: "missing".into(),
                containers: Vec::new(),
            }),
            None,
            "a name absent from that namespace's row should not match"
        );
    }

    /// `TableState` tracks its selection as a bare row index (see
    /// `gpui-component`'s `TableState::selected_row`); a sort that moves the
    /// selected pod to a different index must move the highlight with it,
    /// via `PodTableDelegate::perform_sort`'s deferred `reselect` - the same
    /// path a real header click drives.
    #[gpui_kit::test]
    async fn perform_sort_reselects_the_pod_that_moved(cx: &mut gpui_kit::TestAppContext) {
        use crate::k8s::resource::pods_table::PodTableDelegate;
        use gpui_kit::component::table::{ColumnSort, TableDelegate as _, TableState};

        cx.update(gpui_kit::init);
        let window = cx.add_window(|window, cx| {
            TableState::new(PodTableDelegate::default(), window, cx)
                .row_selectable(true)
                .sortable(true)
        });

        // "b-name" sits at natural index 0 (see `pod_table_rows_fixture`);
        // select it there, matching what a prior row click would have done.
        window
            .update(cx, |table, _window, cx| {
                table.delegate_mut().set_rows(pod_table_rows_fixture());
                table.set_selected_row(0, cx);
                cx.notify();
            })
            .unwrap();
        window
            .update(cx, |table, _window, _cx| {
                table.delegate_mut().remember_selection(Some(PodSelection {
                    namespace: "ns-c".into(),
                    name: "b-name".into(),
                    containers: Vec::new(),
                }));
            })
            .unwrap();

        window
            .update(cx, |table, window, cx| {
                // Ascending by Name (column 0): "a-name" now sorts first,
                // pushing "b-name" from index 0 to index 1.
                table
                    .delegate_mut()
                    .perform_sort(0, ColumnSort::Ascending, window, cx);
            })
            .unwrap();
        cx.run_until_parked();

        let selected_name = window
            .update(cx, |table, _window, _cx| {
                table
                    .selected_row()
                    .map(|ix| table.delegate().rows()[ix].row.name.clone())
            })
            .unwrap();
        assert_eq!(
            selected_name.as_deref(),
            Some("b-name"),
            "selection should follow the pod that moved, not stay pinned to its old index"
        );
    }

    /// The same follow-the-pod guarantee as `perform_sort_reselects_the_pod_that_moved`,
    /// but for a watch-driven row replacement (`PodsPanel::sync_table`'s call to
    /// `set_rows`) rather than a sort - `reselect` is called directly there, not
    /// deferred, since the caller already holds `&mut TableState`.
    #[gpui_kit::test]
    async fn set_rows_reselects_the_pod_that_moved(cx: &mut gpui_kit::TestAppContext) {
        use crate::k8s::resource::pods_table::{PodTableDelegate, reselect};
        use gpui_kit::component::table::TableState;

        cx.update(gpui_kit::init);
        let window = cx.add_window(|window, cx| {
            TableState::new(PodTableDelegate::default(), window, cx).row_selectable(true)
        });

        window
            .update(cx, |table, _window, cx| {
                table.delegate_mut().set_rows(pod_table_rows_fixture());
                table.set_selected_row(0, cx); // "b-name".
                cx.notify();
            })
            .unwrap();
        window
            .update(cx, |table, _window, _cx| {
                table.delegate_mut().remember_selection(Some(PodSelection {
                    namespace: "ns-c".into(),
                    name: "b-name".into(),
                    containers: Vec::new(),
                }));
            })
            .unwrap();

        // Another Pods panel selecting a different pod publishes it app-wide;
        // this table must keep following its own pick, not the global.
        cx.update(|cx| {
            cx.set_global(SelectedPod(Some(PodSelection {
                namespace: "ns-a".into(),
                name: "d-name".into(),
                containers: Vec::new(),
            })));
        });

        // A fresh row arrives ahead of it, pushing "b-name" from index 0 to 1
        // - the same shape of change a live watch update delivers.
        let mut updated = pod_table_rows_fixture();
        updated.insert(
            0,
            crate::k8s::resource::pods_table::PodTableRow {
                row: PodRow {
                    name: "aa-name".into(),
                    namespace: "ns-e".into(),
                    ready: "1/1".into(),
                    status: "Running".into(),
                    restarts: 0,
                    age: String::new(),
                    pod_ip: "10.0.0.5".into(),
                    node: "node-e".into(),
                    age_secs: 50,
                },
                selection: PodSelection {
                    namespace: "ns-e".into(),
                    name: "aa-name".into(),
                    containers: Vec::new(),
                },
            },
        );

        window
            .update(cx, |table, _window, cx| {
                table.delegate_mut().set_rows(updated);
                reselect(table, cx);
                cx.notify();
            })
            .unwrap();

        let selected_name = window
            .update(cx, |table, _window, _cx| {
                table
                    .selected_row()
                    .map(|ix| table.delegate().rows()[ix].row.name.clone())
            })
            .unwrap();
        assert_eq!(
            selected_name.as_deref(),
            Some("b-name"),
            "selection should follow the pod to its new index, not stay pinned to the old one"
        );
    }

    /// When the selected pod is no longer among the new rows, `reselect`
    /// clears the table's selection rather than leaving the highlight on
    /// whichever pod now occupies the old index.
    #[gpui_kit::test]
    async fn set_rows_clears_selection_when_the_selected_pod_is_gone(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use crate::k8s::resource::pods_table::{PodTableDelegate, reselect};
        use gpui_kit::component::table::TableState;

        cx.update(gpui_kit::init);
        let window = cx.add_window(|window, cx| {
            TableState::new(PodTableDelegate::default(), window, cx).row_selectable(true)
        });

        window
            .update(cx, |table, _window, cx| {
                table.delegate_mut().set_rows(pod_table_rows_fixture());
                table.set_selected_row(0, cx); // "b-name".
                cx.notify();
            })
            .unwrap();
        window
            .update(cx, |table, _window, _cx| {
                table.delegate_mut().remember_selection(Some(PodSelection {
                    namespace: "ns-c".into(),
                    name: "b-name".into(),
                    containers: Vec::new(),
                }));
            })
            .unwrap();

        // "b-name" is gone from the new rows entirely (e.g. the pod was
        // deleted between watch updates).
        let remaining: Vec<_> = pod_table_rows_fixture()
            .into_iter()
            .filter(|row| row.row.name != "b-name")
            .collect();

        window
            .update(cx, |table, _window, cx| {
                table.delegate_mut().set_rows(remaining);
                reselect(table, cx);
                cx.notify();
            })
            .unwrap();

        let selected_row = window
            .update(cx, |table, _window, _cx| table.selected_row())
            .unwrap();
        assert_eq!(
            selected_row, None,
            "a selected pod missing from the new rows should clear the selection, \
             not highlight whatever pod now sits at the old index"
        );
    }
}
