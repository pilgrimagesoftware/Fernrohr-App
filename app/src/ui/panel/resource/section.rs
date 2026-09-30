//! Partitions discovered kinds into the fixed-order category sections, and
//! answers which sections and rows should render given the current filter
//! and per-window collapse state.
//!
//! GPUI-free by design: [`visible_sections`] is the one function `resource.rs`
//! calls from `render` and (from section 4 on) the keyboard handlers, so the
//! two can never compute two different answers for "what is on screen right
//! now".

use super::category::Category;
use crate::k8s::cluster::discovery::DiscoveredKind;
use std::collections::HashSet;

/// One non-empty category's kinds, in the fixed section order.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Section {
    pub(super) category: Category,
    pub(super) kinds: Vec<DiscoveredKind>,
}

/// Partitions `kinds` by [`Category::for_gvk`], drops categories with no
/// kinds, and orders the rest by [`Category::ALL`]'s fixed order.
///
/// No kind is lost: the union of every returned section's kinds is exactly
/// `kinds` (spec's "Grouping loses no kind").
pub(super) fn group_kinds(kinds: &[DiscoveredKind]) -> Vec<Section> {
    Category::ALL
        .into_iter()
        .filter_map(|category| {
            let matching: Vec<DiscoveredKind> = kinds
                .iter()
                .filter(|kind| Category::for_gvk(&kind.gvk.group, &kind.plural) == category)
                .cloned()
                .collect();
            (!matching.is_empty()).then_some(Section {
                category,
                kinds: matching,
            })
        })
        .collect()
}

/// Case-insensitive substring match against a kind's label, kind name,
/// plural and API group - design.md's "Matching" rule. An empty `query`
/// matches everything, so "no filter" needs no special case at the call
/// site. Matching the group too is what lets `batch` find `CronJob` even
/// though neither word appears in its label or kind name.
pub(super) fn matches_filter(kind: &DiscoveredKind, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    [
        kind.label(),
        kind.gvk.kind.clone(),
        kind.plural.clone(),
        kind.gvk.group.clone(),
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(&query))
}

/// One section as it should render right now.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct VisibleSection {
    pub(super) category: Category,
    /// How many kinds the category holds, filter aside - the header keeps
    /// reporting this even while collapsed (section 2.2's "still says how
    /// much is inside it"), and while filtered (design.md does not ask the
    /// count to track the filter, only the row list).
    pub(super) total: usize,
    /// The category's kinds that currently pass the filter, in discovery
    /// order. Every kind when there is no filter.
    pub(super) matches: Vec<DiscoveredKind>,
    /// Whether this section's rows should be shown right now: the stored
    /// collapse state, unless a filter is active - then expanded whenever it
    /// has a match, per design.md's "It expands."
    pub(super) expanded: bool,
}

/// The sections to render, in fixed order, given `collapsed` (per-window
/// state, left untouched by filtering) and `filter` (empty means no filter is
/// active).
///
/// A section with no match while filtering is dropped entirely rather than
/// kept as an empty heading (design.md: "sections with no matches are hidden
/// entirely"). Filtering never mutates `collapsed`, which is what lets the
/// caller restore the exact pre-filter shape once the filter clears.
pub(super) fn visible_sections(
    kinds: &[DiscoveredKind],
    collapsed: &HashSet<Category>,
    filter: &str,
) -> Vec<VisibleSection> {
    let filtering = !filter.is_empty();
    group_kinds(kinds)
        .into_iter()
        .filter_map(|section| {
            let matches: Vec<DiscoveredKind> = section
                .kinds
                .iter()
                .filter(|kind| matches_filter(kind, filter))
                .cloned()
                .collect();
            if filtering && matches.is_empty() {
                return None;
            }
            let expanded = filtering || !collapsed.contains(&section.category);
            Some(VisibleSection {
                category: section.category,
                total: section.kinds.len(),
                matches,
                expanded,
            })
        })
        .collect()
}
