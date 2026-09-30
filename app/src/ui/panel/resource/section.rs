//! Partitions discovered kinds into the fixed-order category sections.
//!
//! GPUI-free by design, so `resource-panel-grouping` section 2's "no kind
//! lost" and "fixed order" guarantees are testable without a window.

use super::category::Category;
use crate::k8s::cluster::discovery::DiscoveredKind;

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
