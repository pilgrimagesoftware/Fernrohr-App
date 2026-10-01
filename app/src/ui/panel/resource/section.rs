//! Partitions discovered kinds into the fixed-order category sections, and
//! answers which sections and rows should render given the current filter
//! and per-window collapse state.
//!
//! GPUI-free by design: [`visible_sections`] is the one function `resource.rs`
//! calls from `render` and from the keyboard handlers, so the two can never
//! compute two different answers for "what is on screen right now".

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

/// One API group's kinds inside the Custom Resources section
/// (`custom-resource-grouping`): that section holds every CRD a cluster
/// serves, which as one flat list mixes unrelated APIs together.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Subgroup {
    /// The API group, `""` for the core group.
    pub(super) group: String,
    pub(super) kinds: Vec<DiscoveredKind>,
}

/// Buckets the Custom Resources section's `kinds` by API group: core-group
/// kinds first (built-ins [`Category::for_gvk`]'s table hasn't named yet),
/// then the other groups alphabetically. Each group's kinds keep the order
/// they arrive in - `discover_kinds`' group-then-kind sort, reused rather than
/// redone - and no kind is dropped or repeated.
pub(super) fn custom_subgroups(kinds: &[DiscoveredKind]) -> Vec<Subgroup> {
    let mut subgroups: Vec<Subgroup> = Vec::new();
    for kind in kinds {
        match subgroups
            .iter_mut()
            .find(|subgroup| subgroup.group == kind.gvk.group)
        {
            Some(subgroup) => subgroup.kinds.push(kind.clone()),
            None => subgroups.push(Subgroup {
                group: kind.gvk.group.clone(),
                kinds: vec![kind.clone()],
            }),
        }
    }
    // Core (`""`) sorts first, then by name. Stable, so equal keys - there
    // are none, each group is one subgroup - would keep their order.
    subgroups.sort_by(|a, b| (!a.group.is_empty(), &a.group).cmp(&(!b.group.is_empty(), &b.group)));
    subgroups
}

/// One section as it should render right now.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct VisibleSection {
    pub(super) category: Category,
    /// How many kinds the category holds, filter aside - the header keeps
    /// reporting this even while collapsed (spec's "still says how much is
    /// inside it"), and while filtered (design.md does not ask the count to
    /// track the filter, only the row list).
    pub(super) total: usize,
    /// The category's kinds that currently pass the filter, in discovery
    /// order. Every kind when there is no filter.
    pub(super) matches: Vec<DiscoveredKind>,
    /// Whether this section's rows should be shown right now: the stored
    /// collapse state, unless a filter is active - then expanded whenever it
    /// has a match, per design.md's "It expands."
    pub(super) expanded: bool,
    /// Custom Resources' API-group subgroups (`custom-resource-grouping`),
    /// built from `matches`; empty for every other section, whose rows are
    /// `matches` directly.
    pub(super) subgroups: Vec<VisibleSubgroup>,
}

/// One API group inside Custom Resources, as it should render right now.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct VisibleSubgroup {
    /// The API group, `""` for the core group.
    pub(super) group: String,
    /// The section's kinds in this group that pass the filter.
    pub(super) matches: Vec<DiscoveredKind>,
    /// Whether its rows show: not in the per-window collapsed set, or a
    /// filter is active (which forces every matching subgroup open).
    pub(super) expanded: bool,
}

impl VisibleSection {
    /// The kinds a user can reach right now, in render order: none while
    /// the section is collapsed; in Custom Resources, only expanded
    /// subgroups' kinds; elsewhere every match. What the keyboard steps
    /// through, so a row hidden by collapsing can't be selected.
    pub(super) fn navigable(&self) -> impl Iterator<Item = &DiscoveredKind> {
        let shown = self.expanded;
        let (flat, grouped): (&[DiscoveredKind], &[VisibleSubgroup]) = if self.subgroups.is_empty()
        {
            (&self.matches, &[])
        } else {
            (&[], &self.subgroups)
        };
        flat.iter()
            .chain(
                grouped
                    .iter()
                    .filter(|subgroup| subgroup.expanded)
                    .flat_map(|subgroup| subgroup.matches.iter()),
            )
            .filter(move |_| shown)
    }
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
    collapsed_subgroups: &HashSet<String>,
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
            let subgroups = if section.category == Category::CustomResources {
                custom_subgroups(&matches)
                    .into_iter()
                    .map(|subgroup| VisibleSubgroup {
                        // Like a section: a filter opens every subgroup with a
                        // match (one with none isn't built at all), without
                        // touching the stored set - so clearing it restores
                        // the collapse as it was.
                        expanded: filtering || !collapsed_subgroups.contains(&subgroup.group),
                        group: subgroup.group,
                        matches: subgroup.kinds,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            Some(VisibleSection {
                category: section.category,
                total: section.kinds.len(),
                matches,
                expanded,
                subgroups,
            })
        })
        .collect()
}
