//! Whether two commands' key contexts can be live on the same focus path at
//! once (#331). It owns that one question, read from the context predicates
//! themselves; what counts as a clash, given an overlap, is `conflicts`'.
//!
//! A command's context is a GPUI predicate such as `PodsPanel` or
//! `TypedLabelLogs && !Input`. Every one registered is a conjunction of
//! identifiers, some negated, so each reduces to a [`Clause`]: the identifiers
//! the matching context must carry, and the ones the focus path must not.
//! Two clauses overlap when one's required identifiers include all of the
//! other's - the same base context, one possibly narrower - and neither
//! forbids an identifier the other requires. Different base contexts are
//! different panels, which are never both the focus path's deepest context,
//! so they don't overlap, as before.
//!
//! A predicate that isn't a plain conjunction (`||`, `>`, `==`) falls back to
//! comparing the text exactly, the old rule.

use gpui_kit::{KeyBindingContextPredicate, SharedString};
use std::collections::BTreeSet;

/// Whether keys in the scopes `mine` and `theirs` (`None` = global) can both be
/// live at once. A global key is live everywhere.
pub(super) fn overlap(mine: Option<&str>, theirs: Option<&str>) -> bool {
    match (mine, theirs) {
        (Some(mine), Some(theirs)) => contexts_overlap(mine, theirs),
        _ => true,
    }
}

/// Whether the two context predicates can match the same focus path at once.
pub(super) fn contexts_overlap(mine: &str, theirs: &str) -> bool {
    if mine == theirs {
        return true;
    }
    match (Clause::parse(mine), Clause::parse(theirs)) {
        (Some(mine), Some(theirs)) => mine.overlaps(&theirs),
        // Not a conjunction this can reason about: exact text only, which
        // already failed above.
        _ => false,
    }
}

/// A context predicate that is a conjunction of identifiers and negated
/// identifiers.
#[derive(Debug, Default, PartialEq, Eq)]
struct Clause {
    /// Identifiers the matching context carries.
    required: BTreeSet<SharedString>,
    /// Identifiers no context on the focus path carries.
    forbidden: BTreeSet<SharedString>,
}

impl Clause {
    fn parse(source: &str) -> Option<Self> {
        let predicate = KeyBindingContextPredicate::parse(source).ok()?;
        let mut clause = Self::default();
        clause.add(&predicate).then_some(clause)
    }

    /// Adds `predicate`'s literals; `false` if it isn't a plain conjunction.
    fn add(&mut self, predicate: &KeyBindingContextPredicate) -> bool {
        match predicate {
            KeyBindingContextPredicate::Identifier(name) => {
                self.required.insert(name.clone());
                true
            }
            KeyBindingContextPredicate::Not(inner) => match inner.as_ref() {
                KeyBindingContextPredicate::Identifier(name) => {
                    self.forbidden.insert(name.clone());
                    true
                }
                _ => false,
            },
            KeyBindingContextPredicate::And(left, right) => self.add(left) && self.add(right),
            KeyBindingContextPredicate::Equal(..)
            | KeyBindingContextPredicate::NotEqual(..)
            | KeyBindingContextPredicate::Descendant(..)
            | KeyBindingContextPredicate::Or(..) => false,
        }
    }

    fn overlaps(&self, other: &Self) -> bool {
        let same_base =
            self.required.is_subset(&other.required) || other.required.is_subset(&self.required);
        let contradicts = !self.forbidden.is_disjoint(&other.required)
            || !other.forbidden.is_disjoint(&self.required);
        same_base && !contradicts
    }
}

#[cfg(test)]
mod tests {
    use super::{contexts_overlap, overlap};

    #[test]
    fn a_narrower_clause_on_the_same_context_overlaps() {
        assert!(contexts_overlap(
            "TypedLabelLogs",
            "TypedLabelLogs && !Input"
        ));
        assert!(contexts_overlap(
            "TypedLabelLogs && !Input",
            "TypedLabelLogs"
        ));
        assert!(contexts_overlap("PodsPanel", "PodsPanel && PodShellable"));
        assert!(contexts_overlap(
            "ServicesList && !Input",
            "!Input && ServicesList"
        ));
    }

    #[test]
    fn different_contexts_and_contradictions_do_not() {
        assert!(!contexts_overlap("PodsPanel", "LogsPanel"));
        assert!(!contexts_overlap(
            "PodsPanel && !Input",
            "LogsPanel && !Input"
        ));
        assert!(!contexts_overlap("Editor", "Editor && !Editor"));
        assert!(!contexts_overlap(
            "ObjectListPanel && !Input",
            "ObjectListPanel && Input"
        ));
    }

    #[test]
    fn global_overlaps_everything() {
        assert!(overlap(None, None));
        assert!(overlap(None, Some("PodsPanel && !Input")));
        assert!(overlap(Some("PodsPanel"), None));
    }

    #[test]
    fn anything_but_a_conjunction_compares_as_text() {
        assert!(contexts_overlap("A || B", "A || B"));
        assert!(!contexts_overlap("A || B", "A"));
        assert!(!contexts_overlap("Workspace > PodsPanel", "PodsPanel"));
    }
}
