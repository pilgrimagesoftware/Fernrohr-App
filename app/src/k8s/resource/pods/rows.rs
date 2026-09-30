//! A pod as a list row, and the view pipeline over rows: namespace scope, name filter and sort.

use super::*;

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

pub(super) fn uid(pod: &Pod) -> String {
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
pub(super) fn matches_filter(row: &PodRow, filter: &str) -> bool {
    filter.is_empty() || row.name.contains(filter)
}

// UNWIRED: see `matches_namespace` above.
#[allow(dead_code)]
pub(super) fn sort_rows(rows: &mut [PodRow], sort: &SortState) {
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

#[cfg(test)]
mod tests;
