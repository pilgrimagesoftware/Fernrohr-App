//! Remembering a context's folded groups across panels and restarts (#357):
//! the panel restores them from [`ResourceGroupMemory`] when it starts on, or
//! switches to, a context, and records them there on every fold. This module
//! owns the conversion between the panel's sets and the saved lists; which
//! groups a fold changes stays with the handlers in `actions` and the parent.

use super::ResourcePanel;
use super::category::Category;
use crate::config::workspace::ResourceGroups;
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

    /// Records this panel's folds for its context, then re-renders - every
    /// fold's last step.
    pub(super) fn groups_changed(&mut self, cx: &mut Context<Self>) {
        let mut collapsed_sections: Vec<String> = self
            .collapsed
            .iter()
            .map(|category| category.key().to_string())
            .collect();
        collapsed_sections.sort();
        let mut expanded_subgroups: Vec<String> = self.expanded_subgroups.iter().cloned().collect();
        expanded_subgroups.sort();
        ResourceGroupMemory::record(
            cx,
            &self.context_name,
            ResourceGroups {
                collapsed_sections,
                expanded_subgroups,
            },
        );
        cx.notify();
    }
}
