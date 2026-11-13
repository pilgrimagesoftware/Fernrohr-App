//! Remembering a context's folded groups across panels and restarts (#357):
//! the panel restores them from [`ResourceGroupMemory`] when it starts on, or
//! switches to, a context, and records them there on every fold. This module
//! owns the conversion between the panel's sets and the saved lists; which
//! groups a fold changes stays with the handlers in `actions` and the parent.

use super::ResourcePanel;
use super::category::Category;
use crate::util::shell::ResourceGroupMemory;
use gpui_kit::{App, Context};

impl ResourcePanel {
    /// Replaces this panel's folds with those remembered for its context, or
    /// the defaults (sections expanded, subgroups collapsed) when none are.
    /// A section name this build doesn't know is skipped.
    pub(super) fn restore_groups(&mut self, cx: &App) {
        let groups = ResourceGroupMemory::get(cx, &self.context_name).unwrap_or_default();
        self.collapsed = groups
            .collapsed_sections
            .iter()
            .filter_map(|key| Category::from_key(key))
            .collect();
        self.expanded_subgroups = groups.expanded_subgroups.into_iter().collect();
    }

    /// Records `fold` - only that - in its context's remembered groups, then
    /// re-renders: every fold's last step. Only the changed key is written,
    /// so two windows on one context each keep what the other folded.
    pub(super) fn groups_changed(&mut self, fold: Fold, cx: &mut Context<Self>) {
        ResourceGroupMemory::update(cx, &self.context_name, |groups| match fold {
            Fold::Section(category) => set_member(
                &mut groups.collapsed_sections,
                category.key(),
                self.collapsed.contains(&category),
            ),
            Fold::Subgroup(group) => set_member(
                &mut groups.expanded_subgroups,
                &group,
                self.expanded_subgroups.contains(&group),
            ),
            Fold::AllSubgroups => {
                groups.expanded_subgroups = self.expanded_subgroups.iter().cloned().collect();
            }
        });
        cx.notify();
    }
}

/// What one fold changed.
pub(super) enum Fold {
    /// One top-level section, collapsed or expanded.
    Section(Category),
    /// One Custom Resources subgroup, by API group.
    Subgroup(String),
    /// Collapse or expand all: every subgroup at once.
    AllSubgroups,
}

/// Puts `key` in `list` or takes it out.
fn set_member(list: &mut Vec<String>, key: &str, member: bool) {
    list.retain(|entry| entry != key);
    if member {
        list.push(key.to_string());
    }
}
