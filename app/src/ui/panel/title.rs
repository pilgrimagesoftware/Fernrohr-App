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

use crate::ui::nav::NavTarget;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::*;
use std::rc::Rc;

/// Wraps dock-panel content with the same focus treatment used by every
/// resource view. The dock skin owns title-bar chrome but has no focus-aware
/// panel-style hook, so the panel body supplies the visible focus boundary.
pub fn focus_frame(
    content: impl IntoElement,
    focus_handle: &FocusHandle,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let border = focus_border(focus_handle.is_focused(window), theme.primary, theme.border);

    // Inset rather than flush with the outer edge: a panel at the window's
    // bottom edge would otherwise have this border's square corners clipped
    // by macOS's rounded window mask.
    div().size_full().child(
        div()
            .size_full()
            .m(px(2.))
            .border_1()
            .border_color(border)
            .child(content),
    )
}

fn focus_border(focused: bool, primary: Hsla, border: Hsla) -> Hsla {
    if focused { primary } else { border }
}

/// Everything a panel needs to draw its title bar, and everything the window
/// keys an open panel on.
///
/// One struct rather than loose fields, because the title bar and the window's
/// dedup key want the same facts and must not disagree: the bar says "Pod ·
/// staging" and the key that finds that panel again has to carry the same
/// staging. [`PanelKey`](crate::util::shell::PanelKey) is derived from this, not
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
    /// Namespaces this panel is scoped to. An empty list means all namespaces.
    pub namespaces: Vec<String>,
}

impl PanelScope {
    pub fn new(target: NavTarget, context_name: String) -> Self {
        Self {
            target,
            context_name,
            connection_count: 1,
            namespaces: Vec::new(),
        }
    }

    /// The same scope pointed at a different namespace - what the title bar's
    /// namespace picker produces.
    pub fn scoped_to(&self, mut namespaces: Vec<String>) -> Self {
        namespaces.sort_unstable();
        namespaces.dedup();
        Self {
            namespaces,
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
        format!("{} · {}", scope.target.list_label(), scope.context_name)
    } else {
        scope.target.list_label()
    }
}

/// The label a namespace scope reads on the picker's button.
fn label_for(namespaces: &[String]) -> String {
    match namespaces {
        [] => "All namespaces".to_string(),
        [namespace] => namespace.clone(),
        namespaces => format!("{} namespaces", namespaces.len()),
    }
}

type OnPick = Rc<dyn Fn(Vec<String>, &mut App)>;

/// The namespace scopes a picker offers: all namespaces, then the cluster's
/// sorted namespace names.
pub fn namespaces_offered(namespaces: &[String]) -> Vec<Option<String>> {
    std::iter::once(None)
        .chain(namespaces.iter().cloned().map(Some))
        .collect()
}

/// The tab name the dock labels the panel's tab with, which is its title.
///
/// Its own function because the dock asks for the title and the tab name
/// separately, and a panel supplying one and not the other would show a
/// different name in the tab than in the bar.
pub fn tab_name(scope: &PanelScope) -> Option<SharedString> {
    Some(title(scope).into())
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

/// The namespace picker, or nothing for a cluster-scoped kind.
///
/// Pinned to the trailing end of the title bar by `Panel::title_suffix`. The
/// returned `None` is what omits the picker for cluster-scoped kinds, so the
/// presence rule and the rendering are one decision rather than two that can
/// disagree.
pub fn namespace_picker(
    scope: &PanelScope,
    namespaces: &[String],
    on_pick: impl Fn(Vec<String>, &mut App) + 'static,
) -> Option<AnyElement> {
    if !scope.is_namespaced() {
        return None;
    }
    // The menu closure is `'static`, so everything it reads is captured by
    // value: the menu outlives this render, and the panel it belongs to may be
    // dropped before the menu is.
    let current = scope.namespaces.clone();
    let offered = namespaces_offered(namespaces);
    let on_pick: OnPick = Rc::new(on_pick);
    let picker = Button::new("panel-namespace")
        .label(label_for(&current))
        .icon(IconName::ChevronDown)
        .xsmall()
        .ghost()
        .tab_stop(false)
        .tooltip("Namespace")
        .dropdown_menu(move |menu, _window, _cx| {
            // Built per open rather than hoisted: `PopupMenuItem` is not
            // `Clone`, and this closure is `Fn` so it can run more than once.
            let mut menu = menu;
            for offered in &offered {
                let on_pick = on_pick.clone();
                let offered = offered.clone();
                let checked = match &offered {
                    None => current.is_empty(),
                    Some(namespace) => current.contains(namespace),
                };
                let label = match &offered {
                    None => "All namespaces".to_string(),
                    Some(namespace) => namespace.clone(),
                };
                let next = match &offered {
                    None => Vec::new(),
                    Some(namespace) if current.contains(namespace) => current
                        .iter()
                        .filter(|selected| *selected != namespace)
                        .cloned()
                        .collect(),
                    Some(namespace) => {
                        let mut selected = current.clone();
                        selected.push(namespace.clone());
                        selected.sort_unstable();
                        selected.dedup();
                        selected
                    }
                };
                menu = menu.item(
                    PopupMenuItem::new(label)
                        .checked(checked)
                        .on_click(move |_event, _window, cx| on_pick(next.clone(), cx)),
                );
            }
            menu
        });
    Some(picker.into_any_element())
}

/// The controls at the trailing end of a resource panel's title bar.
///
/// The dock draws the controls menu (`IconName::Ellipsis`) itself for every
/// panel that has a title bar, so what a panel owes the bar is the close
/// control beside it.
pub fn toolbar_buttons() -> Option<Vec<Button>> {
    Some(vec![close_button()])
}

/// A panel changed the scope it shows.
///
/// The window keys each open panel on its scope, so it has to hear this: a panel
/// re-scoped in place is no longer the panel its old key names, and without the
/// event the window would go on focusing the wrong one the next time that key
/// came up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopeEvent {
    NamespacesChanged(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::{PanelScope, focus_border, label_for, namespaces_offered, title};
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use crate::ui::nav::NavTarget;
    use gpui_kit::Hsla;
    use kube::core::GroupVersionKind;

    #[test]
    fn a_focused_panel_uses_the_primary_border() {
        let primary = Hsla {
            h: 0.,
            s: 1.,
            l: 0.5,
            a: 1.,
        };
        let border = Hsla {
            h: 0.5,
            s: 1.,
            l: 0.5,
            a: 1.,
        };

        assert_eq!(focus_border(true, primary, border), primary);
        assert_eq!(focus_border(false, primary, border), border);
    }

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
        assert_eq!(title(&scope(kind("Pod", true), 1)), "Pods");
    }

    /// Section 10.1: with more than one connection the panels stop being
    /// interchangeable, so each says which cluster it reads.
    #[test]
    fn several_connections_name_the_cluster() {
        assert_eq!(title(&scope(kind("Pod", true), 2)), "Pods · kind-dev");
        assert_eq!(
            title(&scope(kind("Deployment", true), 3)),
            "Deployments · kind-dev"
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
        assert_eq!(label_for(&s.namespaces), "All namespaces");
        s = s.scoped_to(vec!["staging".to_string()]);
        assert_eq!(label_for(&s.namespaces), "staging");
        s = s.scoped_to(vec!["staging".to_string(), "default".to_string()]);
        assert_eq!(label_for(&s.namespaces), "2 namespaces");
        s = s.scoped_to(Vec::new());
        assert_eq!(label_for(&s.namespaces), "All namespaces");
    }

    /// A window can only represent the scopes the picker offers, so the default
    /// it starts on has to be among them.
    #[test]
    fn the_default_scope_is_one_the_picker_offers() {
        assert!(
            namespaces_offered(&[]).contains(&None),
            "the scope a panel starts on must be selectable"
        );
    }

    /// The title bar and the panel key have to agree about scope, so scoping is
    /// a change of scope and nothing else.
    #[test]
    fn scoping_changes_only_the_namespace() {
        let s = scope(kind("Pod", true), 2);
        let scoped = s.scoped_to(vec!["staging".to_string(), "default".to_string()]);
        assert_eq!(scoped.namespaces, ["default", "staging"]);
        assert_eq!(scoped.target, s.target);
        assert_eq!(scoped.context_name, s.context_name);
        assert_eq!(scoped.connection_count, s.connection_count);
    }
}
