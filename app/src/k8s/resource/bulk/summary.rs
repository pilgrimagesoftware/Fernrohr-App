//! Naming a checked set in a confirmation: how many of which kind
//! (`"2 Deployments, 1 DaemonSet"`), in the order the kinds were first checked.

use crate::k8s::cluster::discovery::DiscoveredKind;

/// `count` of `kind`: `"1 Pod"`, `"4 Pods"`.
pub(crate) fn plural_count(count: usize, kind: &DiscoveredKind) -> String {
    if count == 1 {
        format!("1 {}", kind.gvk.kind)
    } else {
        format!("{count} {}", kind.plural_name())
    }
}

/// How many of each kind `kinds` holds, kinds in first-seen order:
/// `"2 Deployments, 1 DaemonSet"`.
pub(crate) fn count_of_kinds<'a>(kinds: impl IntoIterator<Item = &'a DiscoveredKind>) -> String {
    let mut counts: Vec<(&DiscoveredKind, usize)> = Vec::new();
    for kind in kinds {
        match counts.iter_mut().find(|(seen, _)| *seen == kind) {
            Some((_, count)) => *count += 1,
            None => counts.push((kind, 1)),
        }
    }
    counts
        .into_iter()
        .map(|(kind, count)| plural_count(count, kind))
        .collect::<Vec<_>>()
        .join(", ")
}
