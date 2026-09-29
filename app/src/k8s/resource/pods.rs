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
fn format_age(age_secs: i64) -> String {
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
    rows.sort_by(|a, b| match sort.column.as_str() {
        "namespace" => a.namespace.cmp(&b.namespace),
        "ready" => a.ready.cmp(&b.ready),
        "status" => a.status.cmp(&b.status),
        "restarts" => a.restarts.cmp(&b.restarts),
        "age" => a.age_secs.cmp(&b.age_secs),
        _ => a.name.cmp(&b.name),
    });
    if !sort.ascending {
        rows.reverse();
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
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableEvent, TableState};
use gpui_kit::*;

actions!(pods, [WarpNamespace, DescribePod, ShowPodLogs, ShowPodYaml]);

pub const PANEL_KEY_CONTEXT: &str = "PodsPanel";

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

struct PodTableRow {
    row: PodRow,
    selection: PodSelection,
}

#[derive(Default)]
struct PodTableDelegate {
    rows: Vec<PodTableRow>,
}

impl TableDelegate for PodTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        8
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let (id, title, width) = match col_ix {
            0 => ("name", "Name", 220.),
            1 => ("namespace", "Namespace", 150.),
            2 => ("ready", "Ready", 80.),
            3 => ("status", "Status", 130.),
            4 => ("restarts", "Restarts", 90.),
            5 => ("age", "Age", 70.),
            6 => ("ip", "IP", 150.),
            _ => ("node", "Node", 180.),
        };
        Column::new(id, title).width(px(width))
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix].row;
        let value = match col_ix {
            0 => row.name.clone(),
            1 => row.namespace.clone(),
            2 => row.ready.clone(),
            3 => row.status.clone(),
            4 => row.restarts.to_string(),
            5 => row.age.clone(),
            6 => row.pod_ip.clone(),
            _ => row.node.clone(),
        };
        div().whitespace_nowrap().child(value)
    }
}

enum PodDetail {
    Description(String),
    Yaml(String),
}

#[derive(Default)]
pub struct SelectedPod(pub Option<PodSelection>);

impl Global for SelectedPod {}

/// A dock panel connecting to its scope's cluster and rendering its live,
/// all-namespaces Pods table.
pub struct PodsPanel {
    scope: PanelScope,
    connection: Entity<crate::k8s::cluster::connection::ClusterConnection>,
    table: Entity<PodsTable>,
    namespaces: Entity<crate::k8s::cluster::namespaces::NamespaceList>,
    subscribed: bool,
    focus_handle: FocusHandle,
    selected: Option<Pod>,
    detail: Option<PodDetail>,
    pod_table: Option<Entity<TableState<PodTableDelegate>>>,
}

impl PodsPanel {
    pub fn new(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let context_name = scope.context_name.clone();
        let connection = ClusterRegistry::connection(cx, &context_name);
        let namespaces =
            crate::k8s::cluster::namespaces::NamespaceRegistry::list(cx, &context_name);
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
            selected: None,
            detail: None,
            pod_table: None,
        };
        this.start_watch_if_connected(&connection, cx);
        this
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

    fn selected_pod(&self) -> Option<&Pod> {
        self.selected.as_ref()
    }

    fn on_action_warp_namespace(
        &mut self,
        _: &WarpNamespace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(namespace) = self
            .selected_pod()
            .and_then(|pod| pod.metadata.namespace.clone())
        else {
            return;
        };
        self.scope = self.scope.scoped_to(vec![namespace.clone()]);
        cx.emit(ScopeEvent::NamespacesChanged(vec![namespace]));
    }

    fn on_action_describe_pod(
        &mut self,
        _: &DescribePod,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.detail = self.selected_pod().map(|pod| {
            PodDetail::Description(format!(
                "Name: {}\nNamespace: {}\nStatus: {}\nNode: {}\nPod IP: {}",
                pod.metadata.name.as_deref().unwrap_or("Pod"),
                pod.metadata.namespace.as_deref().unwrap_or("default"),
                pod.status
                    .as_ref()
                    .and_then(|status| status.phase.as_deref())
                    .unwrap_or("Unknown"),
                pod.spec
                    .as_ref()
                    .and_then(|spec| spec.node_name.as_deref())
                    .unwrap_or("Unscheduled"),
                pod.status
                    .as_ref()
                    .and_then(|status| status.pod_ip.as_deref())
                    .unwrap_or("Unassigned"),
            ))
        });
        cx.notify();
    }

    fn on_action_show_pod_logs(
        &mut self,
        _: &ShowPodLogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_pod().is_some() {
            window.dispatch_action(Box::new(crate::ui::nav::ShowLogs), cx);
        }
    }

    fn on_action_show_pod_yaml(
        &mut self,
        _: &ShowPodYaml,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.detail = self
            .selected_pod()
            .and_then(|pod| serde_yaml_ng::to_string(pod).ok().map(PodDetail::Yaml));
        cx.notify();
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
                    .sortable(false)
                    .col_movable(false)
                    .col_resizable(false)
            });
            cx.subscribe(&table, |this, table, event, cx| {
                let TableEvent::SelectRow(row_ix) = event else {
                    return;
                };
                let Some(selection) = table
                    .read(cx)
                    .delegate()
                    .rows
                    .get(*row_ix)
                    .map(|row| row.selection.clone())
                else {
                    return;
                };
                cx.set_global(SelectedPod(Some(selection.clone())));
                this.selected = this
                    .table
                    .read(cx)
                    .pods()
                    .iter()
                    .find(|pod| {
                        pod.metadata.name.as_deref() == Some(&selection.name)
                            && pod.metadata.namespace.as_deref() == Some(&selection.namespace)
                    })
                    .cloned();
                cx.notify();
            })
            .detach();
            self.pod_table = Some(table);
        }
        let table = self.pod_table.as_ref().unwrap().clone();
        table.update(cx, |table, cx| {
            table.delegate_mut().rows = rows;
            table.refresh(cx);
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
                            Kbd::new(Keystroke::parse("w").expect("valid keybinding"))
                        });
                let describe_key =
                    Kbd::binding_for_action(&DescribePod, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse("d").expect("valid keybinding"))
                        });
                let logs_key =
                    Kbd::binding_for_action(&ShowPodLogs, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse("l").expect("valid keybinding"))
                        });
                let yaml_key =
                    Kbd::binding_for_action(&ShowPodYaml, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse("y").expect("valid keybinding"))
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
                    .children(self.detail.as_ref().map(|detail| {
                        match detail {
                            PodDetail::Description(detail) => {
                                div().mb_3().child(detail.clone()).into_any_element()
                            }
                            PodDetail::Yaml(yaml) => div()
                                .mb_3()
                                .max_h(px(240.))
                                .font_family(cx.theme().mono_font_family.clone())
                                .whitespace_nowrap()
                                .child(yaml.clone())
                                .overflow_scrollbar()
                                .into_any_element(),
                        }
                    }))
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

        div()
            .size_full()
            .key_context(PANEL_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_warp_namespace))
            .on_action(cx.listener(Self::on_action_describe_pod))
            .on_action(cx.listener(Self::on_action_show_pod_logs))
            .on_action(cx.listener(Self::on_action_show_pod_yaml))
            .child(panel_title::focus_frame(
                content,
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

    fn title_suffix(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let this = cx.weak_entity();
        let namespaces = self.namespaces.read(cx).names();
        panel_title::namespace_picker(&self.scope, namespaces, move |namespaces, cx| {
            let _ = this.update(cx, |this: &mut Self, cx| {
                this.scope = this.scope.scoped_to(namespaces.clone());
                cx.emit(ScopeEvent::NamespacesChanged(namespaces));
            });
        })
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
        NamespaceScope, PanelScope, Pod, PodsPanel, PodsTable, ShowPodYaml, SortState,
        is_unauthorized, matches_namespaces, pod_row, view_rows, watcher,
    };
    use crate::ui::nav::NavTarget;
    use gpui_kit::{AppContext as _, ParentElement as _, Styled as _};
    use jiff::Timestamp;
    use k8s_openapi::api::core::v1::{ContainerStatus, PodStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};
    use kube::core::response::Status;

    struct PanelHarness {
        first: gpui_kit::Entity<PodsPanel>,
        second: gpui_kit::Entity<PodsPanel>,
    }

    impl gpui_kit::Render for PanelHarness {
        fn render(
            &mut self,
            _window: &mut gpui_kit::Window,
            _cx: &mut gpui_kit::Context<Self>,
        ) -> impl gpui_kit::IntoElement {
            gpui_kit::div()
                .size_full()
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

    #[gpui_kit::test]
    async fn pod_shortcut_dispatches_only_to_the_focused_panel(cx: &mut gpui_kit::TestAppContext) {
        let (first, second) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            let first =
                cx.new(|cx| PodsPanel::new(PanelScope::new(NavTarget::pods(), "dev".into()), cx));
            let second =
                cx.new(|cx| PodsPanel::new(PanelScope::new(NavTarget::pods(), "prod".into()), cx));
            first.update(cx, |panel, _| panel.selected = Some(Pod::default()));
            second.update(cx, |panel, _| panel.selected = Some(Pod::default()));
            (first, second)
        });
        let window = cx.add_window(|_, _| PanelHarness {
            first: first.clone(),
            second: second.clone(),
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

        assert!(first.read_with(cx, |panel, _| panel.detail.is_some()));
        assert!(second.read_with(cx, |panel, _| panel.detail.is_none()));
    }
}
