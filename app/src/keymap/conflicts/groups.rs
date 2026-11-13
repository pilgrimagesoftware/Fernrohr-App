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
    let bulk = crate::ui::bulk::key_contexts;
    let kind = crate::ui::kind_actions::gates;
    let group = |own: &[&'static str], shared: Vec<&'static str>| -> BTreeSet<&'static str> {
        own.iter().copied().chain(shared).collect()
    };
    vec![
        group(crate::util::logs::KEY_CONTEXT_GROUP, Vec::new()),
        group(object_detail::KEY_CONTEXT_GROUP, kind(false)),
        group(pod_detail::KEY_CONTEXT_GROUP, Vec::new()),
        group(object_list::TABLE_KEY_CONTEXT_GROUP, Vec::new()),
        group(
            object_list::PANEL_KEY_CONTEXT_GROUP,
            bulk().into_iter().chain(kind(true)).collect(),
        ),
        group(pods::KEY_CONTEXT_GROUP, bulk()),
        group(events_browser::KEY_CONTEXT_GROUP, Vec::new()),
        group(exec::KEY_CONTEXT_GROUP, Vec::new()),
    ]
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
