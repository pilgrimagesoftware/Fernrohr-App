use crate::resource_index::ResourceIndex;
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
        age_secs,
    }
}

/// The live Pods index for one watch, kept up to date by [`PodsTable::apply`]
/// as `watcher::Event`s arrive off the coalescing drain.
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

/// Applies `event` to `table` and notifies its observers, i.e. the entity
/// half of the (model, view) pair - a Pods panel holds `Entity<PodsTable>`
/// and calls this from the watch's drain callback.
pub fn apply_and_notify(
    table: &gpui_kit::Entity<PodsTable>,
    event: watcher::Event<Pod>,
    cx: &mut gpui_kit::App,
) {
    table.update(cx, |table, cx| {
        table.apply(event);
        cx.notify();
    });
}

/// Starts a `kube_runtime::watcher` for all Pods across every namespace on
/// `client` and applies its events to `table` as they arrive. Reconnect and
/// backoff after a stream error are `kube_runtime`'s own job (design D3);
/// this just keeps consuming the stream.
pub fn watch_all_namespaces(
    client: kube::Client,
    table: gpui_kit::Entity<PodsTable>,
    cx: &mut gpui_kit::App,
) -> gpui_kit::Task<()> {
    use futures_util::StreamExt;
    use kube::Api;

    let rx = crate::runtime::spawn_stream(cx, 64, move |tx| async move {
        let api: Api<Pod> = Api::all(client);
        let mut stream = Box::pin(watcher::watcher(api, watcher::Config::default()));
        while let Some(event) = stream.next().await {
            let Ok(event) = event else {
                continue;
            };
            if tx.send(event).await.is_err() {
                break;
            }
        }
    });
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |event| {
            let _ = table.update(cx, |table, cx| {
                table.apply(event);
                cx.notify();
            });
        })
        .await;
    })
}

use crate::config::workspace::{NamespaceScope, SortState};

pub fn matches_namespace(pod: &Pod, scope: &NamespaceScope) -> bool {
    match scope {
        NamespaceScope::All => true,
        NamespaceScope::Single(namespace) => {
            pod.metadata.namespace.as_deref() == Some(namespace.as_str())
        }
    }
}

fn matches_filter(row: &PodRow, filter: &str) -> bool {
    filter.is_empty() || row.name.contains(filter)
}

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

use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent};
use gpui_kit::*;

/// A dock panel connecting to the current kubeconfig context and rendering
/// its live, all-namespaces Pods table.
pub struct PodsPanel {
    connection: Entity<crate::cluster::connection::ClusterConnection>,
    table: Entity<PodsTable>,
    watch: Option<gpui_kit::Task<()>>,
    focus_handle: FocusHandle,
}

impl PodsPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        use crate::cluster::connection::ClusterConnection;

        let connection = ClusterConnection::connect(cx);
        let table = cx.new(|_| PodsTable::new());
        cx.observe(&connection, |this: &mut Self, connection, cx| {
            this.start_watch_if_connected(&connection, cx);
            cx.notify();
        })
        .detach();
        cx.observe(&table, |_, _, cx| cx.notify()).detach();

        let mut this = Self {
            connection: connection.clone(),
            table,
            watch: None,
            focus_handle: cx.focus_handle(),
        };
        this.start_watch_if_connected(&connection, cx);
        this
    }

    fn start_watch_if_connected(
        &mut self,
        connection: &Entity<crate::cluster::connection::ClusterConnection>,
        cx: &mut Context<Self>,
    ) {
        if self.watch.is_some() {
            return;
        }
        let crate::cluster::connection::ConnectionState::Connected(client) =
            &connection.read(cx).state
        else {
            return;
        };
        self.watch = Some(watch_all_namespaces(client.clone(), self.table.clone(), cx));
    }
}

impl Focusable for PodsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PodsPanel {}

impl Render for PodsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::cluster::connection::ConnectionState;

        match &self.connection.read(cx).state {
            ConnectionState::Connecting => div().size_full().child("Connecting..."),
            ConnectionState::Failed(reason) => div()
                .size_full()
                .child(format!("Connection failed: {reason}")),
            ConnectionState::Connected(_) => {
                let now = Timestamp::now();
                let rows = self
                    .table
                    .read(cx)
                    .pods()
                    .iter()
                    .map(|pod| pod_row(pod, now));
                div().size_full().children(rows.map(|row| {
                    div().child(format!(
                        "{}\t{}\t{}\t{}\t{}\t{}",
                        row.name, row.namespace, row.ready, row.status, row.restarts, row.age
                    ))
                }))
            }
        }
    }
}

impl BasePanel for PodsPanel {
    fn panel_name(&self) -> &'static str {
        "Pods"
    }
}

impl Panel for PodsPanel {}

#[cfg(test)]
mod tests {
    // Not `use super::*`: `gpui_kit::*` (imported above for `PodsPanel`)
    // re-exports its own `test` attribute macro, which would shadow
    // `core::prelude::v1::test` for these plain synchronous tests.
    use super::{NamespaceScope, Pod, PodsTable, SortState, pod_row, view_rows, watcher};
    use jiff::Timestamp;
    use k8s_openapi::api::core::v1::{ContainerStatus, PodStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};

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
}
