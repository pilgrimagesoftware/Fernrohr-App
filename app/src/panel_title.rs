//! What every resource panel puts in its title bar, and the rules deciding
//! which parts of it apply.
//!
//! Section 10 of the `cluster-picker-and-navigation` change. The title bar is
//! drawn by `DockArea`, not by the panel body: a panel supplies the pieces
//! through gpui-kit's `Panel` trait (`title`, `title_suffix`, `toolbar_buttons`,
//! `dropdown_menu`) and the dock lays them out. What lives here is the *content*
//! and the three rules that decide it, so every panel draws the same bar from
//! the same inputs rather than each one re-deriving "should this panel name its
//! cluster?" in its own render.

use crate::nav::NavTarget;
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

/// Everything a panel needs to draw its title bar, and everything the window
/// keys an open panel on.
///
/// One struct rather than loose fields, because the title bar and the window's
/// dedup key want the same facts and must not disagree: the bar says "Pod ·
/// staging" and the key that finds that panel again has to carry the same
/// staging. [`PanelKey`](crate::shell::PanelKey) is derived from this, not
/// maintained beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelScope {
    /// What the panel shows - the discovered kind, or the log view.
    pub target: NavTarget,
    /// The cluster context this panel reads from.
    pub context_name: String,
    /// How many cluster connections *this window* has open. One today: adding a
    /// second connection within a connected window is an explicit non-goal of
    /// this change (see `design.md`), so the window passes its count rather
    /// than the bar assuming one.
    pub connection_count: usize,
    /// The namespace this panel is scoped to. `None` is "all namespaces", the
    /// only scope a window can represent today - see [`namespaces_offered`].
    pub namespace: Option<String>,
}

impl PanelScope {
    pub fn new(target: NavTarget, context_name: String) -> Self {
        Self {
            target,
            context_name,
            connection_count: 1,
            namespace: None,
        }
    }

    /// The same scope pointed at a different namespace - what the title bar's
    /// namespace picker produces.
    pub fn scoped_to(&self, namespace: Option<String>) -> Self {
        Self {
            namespace,
            ..self.clone()
        }
    }

    /// Whether the kind is scoped to a namespace, which is what decides if the
    /// title bar carries a namespace picker. `NavTarget::Logs` is a view onto a
    /// single pod rather than a kind, so it follows the pod's kind and is
    /// treated as namespaced - a pod always is.
    pub fn is_namespaced(&self) -> bool {
        match &self.target {
            NavTarget::Kind(kind) => kind.namespaced,
            NavTarget::Logs => true,
        }
    }
}

/// The title bar's name for the panel.
///
/// The cluster is named only when the window has more than one connection open.
/// With one connection it is the only cluster there is, so repeating its name
/// in every panel's title is noise - and the resource panel's own header already
/// says which cluster the window is on.
pub fn title(scope: &PanelScope) -> String {
    if scope.connection_count > 1 {
        format!("{} · {}", scope.target.label(), scope.context_name)
    } else {
        scope.target.label()
    }
}

/// The namespace the title bar's picker is currently set to, as the button
/// reads.
pub fn namespace_label(scope: &PanelScope) -> String {
    scope
        .namespace
        .clone()
        .unwrap_or_else(|| "All namespaces".to_string())
}

/// The namespace scopes this build's picker offers.
///
/// "All namespaces" and nothing else, on purpose. Listing a cluster's
/// namespaces needs a namespace list/watch that this change does not add
/// (see `design.md`'s non-goals), and section 10.2's contract is that the
/// picker *appears* for namespaced kinds and is absent for cluster-scoped ones
/// - not that it enumerates anything. Growing this list is the namespace
/// discovery work's job, and it lands as data, not as a change here.
pub fn namespaces_offered() -> Vec<Option<String>> {
    vec![None]
}

/// The close button every resource panel's title bar carries.
///
/// It dispatches the dock's own `ClosePanel` action rather than reaching for
/// the panel's id: the dock is what owns the layout, `ClosePanel` removes
/// whichever panel the group is displaying, and a container may still refuse
/// (the last group of a dock cannot be closed). A panel that removed itself
/// would have to reimplement that refusal rule.
pub fn close_button() -> Button {
    use gpui_kit::component::dock::ClosePanel;

    Button::new("panel-close")
        .icon(IconName::Close)
        .xsmall()
        .ghost()
        .tab_stop(false)
        .tooltip("Close panel")
        .on_click(|_event, window, cx| window.dispatch_action(Box::new(ClosePanel), cx))
}

#[cfg(test)]
mod tests {
    use super::{PanelScope, namespace_label, namespaces_offered, title};
    use crate::cluster::discovery::DiscoveredKind;
    use crate::nav::NavTarget;
    use kube::core::GroupVersionKind;

    fn kind(kind: &str, namespaced: bool) -> NavTarget {
        NavTarget::Kind(DiscoveredKind {
            gvk: GroupVersionKind::gvk("", "v1", kind),
            plural: format!("{}s", kind.to_lowercase()),
            namespaced,
        })
    }

    fn scope(target: NavTarget, connections: usize) -> PanelScope {
        PanelScope {
            connection_count: connections,
            ..PanelScope::new(target, "kind-dev".to_string())
        }
    }

    /// Section 10.1: one connection means the cluster name is noise - every
    /// panel in the window would repeat the only cluster there is.
    #[test]
    fn a_single_connection_leaves_the_cluster_out_of_the_title() {
        assert_eq!(title(&scope(kind("Pod", true), 1)), "Pod");
    }

    /// Section 10.1: with more than one connection the panels stop being
    /// interchangeable, so each says which cluster it reads.
    #[test]
    fn several_connections_name_the_cluster() {
        assert_eq!(title(&scope(kind("Pod", true), 2)), "Pod · kind-dev");
        assert_eq!(
            title(&scope(kind("Deployment", true), 3)),
            "Deployment · kind-dev"
        );
    }

    /// The Logs view is a target of its own and gets the same rule.
    #[test]
    fn the_logs_view_titles_by_the_same_rule() {
        assert_eq!(title(&scope(NavTarget::Logs, 1)), "Logs");
        assert_eq!(title(&scope(NavTarget::Logs, 2)), "Logs · kind-dev");
    }

    /// Section 10.2: a namespaced kind gets a picker, a cluster-scoped kind
    /// does not. This is the rule, read straight off discovery's own scope.
    #[test]
    fn the_namespace_picker_follows_the_kinds_scope() {
        assert!(scope(kind("Pod", true), 1).is_namespaced());
        assert!(!scope(kind("Node", false), 1).is_namespaced());
        assert!(!scope(kind("Namespace", false), 1).is_namespaced());
        // The log view is over one pod, so it is namespaced whatever the
        // discovery entry for a hypothetical cluster-scoped kind says.
        assert!(scope(NavTarget::Logs, 1).is_namespaced());
    }

    /// The picker's current value reads as the scope it represents, and the
    /// default reads as "all namespaces" rather than as an empty control.
    #[test]
    fn the_picker_reads_its_current_scope() {
        let mut s = scope(kind("Pod", true), 1);
        assert_eq!(namespace_label(&s), "All namespaces");
        s = s.scoped_to(Some("staging".to_string()));
        assert_eq!(namespace_label(&s), "staging");
        s = s.scoped_to(None);
        assert_eq!(namespace_label(&s), "All namespaces");
    }

    /// A window can only represent the scopes the picker offers, so the default
    /// it starts on has to be among them.
    #[test]
    fn the_default_scope_is_one_the_picker_offers() {
        let s = scope(kind("Pod", true), 1);
        assert!(
            namespaces_offered().contains(&s.namespace),
            "the scope a panel starts on must be selectable"
        );
    }

    /// The title bar and the panel key have to agree about scope, so scoping is
    /// a change of scope and nothing else.
    #[test]
    fn scoping_changes_only_the_namespace() {
        let s = scope(kind("Pod", true), 2);
        let scoped = s.scoped_to(Some("staging".to_string()));
        assert_eq!(scoped.namespace.as_deref(), Some("staging"));
        assert_eq!(scoped.target, s.target);
        assert_eq!(scoped.context_name, s.context_name);
        assert_eq!(scoped.connection_count, s.connection_count);
    }
}
