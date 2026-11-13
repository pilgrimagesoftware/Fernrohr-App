//! Which key-context identifiers can sit on one element at once (#331): one
//! group per element whose `key_context()` builder adds several. A panel's
//! element carries its own identifier plus flags - Logs carries `LogsPanel`,
//! `LabelLogs` and `TypedLabelLogs` - so commands gated to any of them are
//! live together. Each builder exports its group beside itself, with the
//! shared vocabularies (bulk, kind actions) added here; this module only
//! collects them.

use crate::k8s::resource::{events_browser, exec, object_detail, object_list, pod_detail, pods};
use std::collections::BTreeSet;
use std::sync::LazyLock;

/// Every element's identifier group.
pub(super) fn groups() -> &'static [BTreeSet<&'static str>] {
    static GROUPS: LazyLock<Vec<BTreeSet<&'static str>>> = LazyLock::new(collect);
    &GROUPS
}

fn collect() -> Vec<BTreeSet<&'static str>> {
    let bulk = crate::ui::bulk::key_contexts();
    let group = |parts: &[&[&'static str]]| -> BTreeSet<&'static str> {
        parts.iter().flat_map(|part| part.iter().copied()).collect()
    };
    let list_panel: Vec<&'static str> = object_list::PANEL_KEY_CONTEXT_GROUP
        .iter()
        .copied()
        .chain(bulk.iter().copied())
        .collect();
    let mut groups = vec![
        group(&[crate::util::logs::KEY_CONTEXT_GROUP]),
        group(&[object_detail::KEY_CONTEXT_GROUP]),
        group(&[pod_detail::KEY_CONTEXT_GROUP]),
        group(&[object_list::TABLE_KEY_CONTEXT_GROUP]),
        group(&[&list_panel]),
        group(&[pods::KEY_CONTEXT_GROUP, &bulk]),
        group(&[events_browser::KEY_CONTEXT_GROUP]),
        group(&[crate::k8s::resource::helm::list::KEY_CONTEXT_GROUP]),
        group(&[exec::KEY_CONTEXT_GROUP]),
    ];
    // A kind action's gate joins its panel's element only with the gates its
    // object is offered with, never with every kind's (`OFFERED_TOGETHER`).
    let kind_actions = crate::ui::kind_actions::gate_sets;
    groups.extend(
        kind_actions(false)
            .iter()
            .map(|gates| group(&[object_detail::KEY_CONTEXT_GROUP, gates])),
    );
    groups.extend(
        kind_actions(true)
            .iter()
            .map(|gates| group(&[&list_panel, gates])),
    );
    groups
}

/// Whether every one of `identifiers` fits one element's group.
pub(super) fn share_an_element<'a>(identifiers: impl IntoIterator<Item = &'a str> + Clone) -> bool {
    groups().iter().any(|group| {
        identifiers
            .clone()
            .into_iter()
            .all(|identifier| group.contains(identifier))
    })
}

/// Identifiers whose element carries no other identifier, so a command gated
/// to one overlaps only a clause on the same identifier. Every identifier a
/// registered command requires is in a group or here (#391); one in neither
/// fails `every_identifier_is_grouped_or_standalone`, so a builder that gains
/// a flag gets its group updated.
#[cfg(test)]
pub(super) const STANDALONE: &[&str] = &[
    "AddContextWindow",
    // gpui-component's dialog's own context.
    "Dialog",
    "Dock",
    "ExecLibraryEntry",
    "ExecLibraryWindow",
    // The Helm release detail panel's element carries only its own (#370).
    "HelmReleasePanel",
    "KeyboardShortcuts",
    "NamespacedList",
    "NodePodsTable",
    "PlaceholderPanel",
    // The Pods table's inner element, apart from the panel's.
    "PodShellable",
    "ReportIssueWindow",
    "ResourcePanel",
    "SavedLayoutsPicker",
    "SettingsWindow",
];

#[cfg(test)]
mod tests;
