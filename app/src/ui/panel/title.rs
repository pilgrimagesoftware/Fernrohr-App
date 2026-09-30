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
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use std::rc::Rc;

/// Wraps dock-panel content.
///
/// Previously drew a bordered box inset within the panel as a focus
/// indicator; dropped per Paul's review (2026-09-29) - a border framing the
/// content read as unwanted "inner framing" (visible in a Logs panel
/// screenshot: a boxed rule around the log lines, separate from the panel's
/// own edge). The real replacement - coloring the panel's own tab when it has
/// focus - needs a per-tab style hook the dock's tab strip does not expose:
/// `Panel::title_style` only applies to the single-panel, no-tab-strip case
/// (`TabPanel::render_title`, not `render_tabs`), confirmed by reading the
/// vendored `gpui-component` 0.6.6 source. Tracked alongside the per-tab
/// close button finding in `openspec/changes/per-tab-close-button`, since
/// both are the same "the tab strip isn't per-panel customizable" gap in the
/// same vendored crate.
pub fn focus_frame(
    content: impl IntoElement,
    _focus_handle: &FocusHandle,
    _window: &Window,
    _cx: &App,
) -> impl IntoElement {
    div().size_full().child(content)
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
    /// treated as namespaced - a pod always is. So is a pod's detail panel,
    /// though a panel over one pod has no namespace scope for a picker to
    /// change and so does not ask for one.
    pub fn is_namespaced(&self) -> bool {
        match &self.target {
            NavTarget::Kind(kind) => kind.namespaced,
            NavTarget::Logs | NavTarget::Pod(_) => true,
            NavTarget::Object(object) => object.kind.namespaced,
        }
    }
}

/// The title bar's name for the panel.
///
/// Plural for a list, singular-plus-name for a panel over one item - both read
/// off the target, so a caller cannot label a list "Pod" or a detail panel
/// "Pods" by forgetting to pass a flag.
///
/// The cluster is never part of the title: the tab and title bar stay short, and
/// [`title_element`]'s tooltip and each panel's own [`context_label`] say which
/// cluster a panel reads.
pub fn title(scope: &PanelScope) -> String {
    scope.target.item_label()
}

/// A pod-scoped panel's content heading: the item (pod, or pod / container) in a
/// title font, then "(<context>)" in a subtitle font when `context` is given - the
/// caller passes it only while the window uses more than one context
/// (`util::shell::window_context_count`). Tab titles never carry the context.
///
/// The name and the context each ellipsize when the panel is too narrow for
/// them - shrinking in proportion to their widths, so neither is squeezed out
/// by the other - and then, only then, carry their full text in a tooltip.
pub fn item_heading(name: String, context: Option<String>, muted: Hsla) -> impl IntoElement {
    div()
        .flex()
        .items_baseline()
        .gap_2()
        .min_w_0()
        .overflow_hidden()
        .child(
            heading_name_box()
                .debug_selector(|| "item-heading-name".into())
                .child(ellipsized_text(
                    SharedString::from(format!("item-heading-{name}")),
                    name,
                )),
        )
        .children(context.map(|context| {
            ellipsizing_box()
                .debug_selector(|| "item-heading-context".into())
                .text_sm()
                .text_color(muted)
                .child(ellipsized_text(
                    SharedString::from(format!("item-heading-context-{context}")),
                    format!("({context})"),
                ))
        }))
}

/// `text` for an [`ellipsizing_box`], with a tooltip of the full text that
/// appears only while the box has ellipsized it.
fn ellipsized_text(id: SharedString, text: String) -> InteractiveText {
    let styled = StyledText::new(text.clone());
    // Shares its state with the element's own layout, so by the time a hover
    // asks for a tooltip it holds the text as drawn: the full text, or the
    // ellipsized one.
    let layout = styled.layout().clone();
    InteractiveText::new(id, styled).tooltip(move |_index, window, cx| {
        is_truncated(&layout, &text).then(|| Tooltip::new(text.clone()).build(window, cx))
    })
}

/// One line that may shrink below its text's width, ellipsizing it.
fn ellipsizing_box() -> Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
}

/// The box an item heading's name sits in: an [`ellipsizing_box`] in the
/// title font.
fn heading_name_box() -> Div {
    ellipsizing_box()
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
}

/// Whether `layout`, once drawn, shows less than `full` - that is, the text
/// was ellipsized to fit.
fn is_truncated(layout: &TextLayout, full: &str) -> bool {
    layout.text() != full
}

/// The context to show beside an item heading: the scope's context while the window
/// uses more than one, otherwise none.
pub fn heading_context(scope: &PanelScope, window_contexts: usize) -> Option<String> {
    (window_contexts > 1).then(|| scope.context_name.clone())
}

/// `text` as the panel's title element, with a "Context: <name>" tooltip. The dock
/// draws this in the tab (see [`tab_name`]) and in the title bar.
pub fn title_element(scope: &PanelScope, text: String) -> AnyElement {
    let tooltip = format!("Context: {}", scope.context_name);
    div()
        .id(SharedString::from(format!(
            "panel-title-{}-{text}",
            scope.context_name
        )))
        .child(text)
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .into_any_element()
}

/// The "Context: <name>" line a panel shows at the top of its content, truncated
/// with an ellipsis when the name is long, with the full name in a tooltip.
pub fn context_label(scope: &PanelScope, muted: Hsla) -> impl IntoElement {
    let full = format!("Context: {}", scope.context_name);
    let tooltip = full.clone();
    div()
        .id(SharedString::from(format!(
            "panel-context-{}",
            scope.context_name
        )))
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_sm()
        .text_color(muted)
        .child(full)
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
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

/// Always `None`: the dock then draws the tab from the panel's title element, so
/// the tab gets [`title_element`]'s context tooltip - a plain tab name can't carry
/// one - and the tab and the title bar can't drift apart.
pub fn tab_name(_scope: &PanelScope) -> Option<SharedString> {
    None
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

/// A panel's failure content: a human-readable message, then - when there is
/// one - the failure's full technical detail underneath it, muted and
/// monospace. Both render through `gpui-component`'s own selectable
/// [`gpui_kit::component::text::TextView`] (`.selectable(true)`), the same
/// primitive its own dialogs and message views use for copyable prose - not a
/// hand-rolled selection - so a user can drag-select either line and press
/// `Cmd+C`/`Ctrl+C` (the component's own binding, `text::state::CONTEXT`) to
/// copy it. `1-window-context-bar` bug 2: a raw `{error:?}` dump was neither
/// readable nor selectable; this is Logs' and Pod detail's shared fix for
/// both halves at once, so the two panels cannot drift back apart on either.
pub fn error_content(message: String, detail: Option<String>, cx: &App) -> impl IntoElement {
    div()
        .size_full()
        .p_3()
        .flex()
        .flex_col()
        .gap_2()
        .child(gpui_kit::component::text::markdown(escape_markdown(&message)).selectable(true))
        .children(detail.map(|detail| {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(gpui_kit::component::text::markdown(code_block(&detail)).selectable(true))
        }))
}

/// `text` with every markdown-significant character backslash-escaped, so the only
/// selectable-text primitive gpui-component offers (markdown) shows it verbatim.
fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if "\\`*_{}[]()<>#+-.!|~".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// `text` as a fenced code block - rendered verbatim and in monospace. The fence is
/// longer than any backtick run inside `text`, so nothing in it can close the block.
fn code_block(text: &str) -> String {
    let longest_run = text.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest_run.max(2) + 1);
    format!("{fence}text\n{text}\n{fence}")
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
    use super::{
        PanelScope, heading_name_box, is_truncated, item_heading, label_for, namespaces_offered,
        title,
    };
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use crate::ui::nav::NavTarget;
    use gpui_kit::{
        AvailableSpace, Context, IntoElement, Pixels, Render, StyledText, TestAppContext,
        TextLayout, Window, black, div, point, px, size,
    };
    use gpui_kit::{ParentElement as _, Styled as _};
    use kube::core::GroupVersionKind;

    /// Draws `name` in the heading's name box at `width`, returning the
    /// layout the heading's tooltip consults.
    fn draw_heading_name(cx: &mut TestAppContext, name: &str, width: Pixels) -> TextLayout {
        let cx = cx.add_empty_window();
        let text = StyledText::new(name.to_string());
        let layout = text.layout().clone();
        cx.draw(
            point(px(0.), px(0.)),
            size(AvailableSpace::Definite(width), AvailableSpace::MinContent),
            |_window, _cx| heading_name_box().w(width).child(text),
        );
        layout
    }

    /// The heading's tooltip appears only when the name did not fit: a narrow
    /// box ellipsizes it, a wide one draws it whole.
    #[gpui_kit::test]
    fn heading_name_reports_truncation_only_when_ellipsized(cx: &mut TestAppContext) {
        let name = "gke-metrics-agent-bl7vc";

        let narrow = draw_heading_name(cx, name, px(40.));
        assert!(is_truncated(&narrow, name), "drawn as {:?}", narrow.text());

        let wide = draw_heading_name(cx, name, px(2000.));
        assert!(!is_truncated(&wide, name), "drawn as {:?}", wide.text());
    }

    /// In a heading too narrow for both, the name and the context share the
    /// width: neither is squeezed to nothing, and the context stays inside the
    /// heading rather than drawing over whatever sits beside it.
    /// A view drawing only an item heading, 128px wide.
    struct NarrowHeading;

    impl Render for NarrowHeading {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().w(px(128.)).child(item_heading(
                "gke-metrics-agent-bl7vc".to_string(),
                Some("carefulcrab-staging-us-west1".to_string()),
                black(),
            ))
        }
    }

    #[gpui_kit::test]
    fn a_narrow_heading_keeps_name_and_context_inside_it(cx: &mut TestAppContext) {
        let width = px(128.);
        // A view rather than `draw`: the heading's text is stateful
        // (`InteractiveText`), and element state lives under a view.
        let (_view, cx) = cx.add_window_view(|_window, _cx| NarrowHeading);
        cx.run_until_parked();

        let name = cx.debug_bounds("item-heading-name").expect("name drawn");
        let context = cx
            .debug_bounds("item-heading-context")
            .expect("context drawn");
        assert!(name.size.width >= width / 4., "name squeezed out: {name:?}");
        assert!(
            context.size.width > px(0.),
            "context squeezed out: {context:?}"
        );
        assert!(
            context.right() <= width,
            "context overflows the heading: {context:?}"
        );
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

    /// With several connections the title still leaves the cluster out; the tab's
    /// tooltip and the panel's "Context:" line name it instead.
    #[test]
    fn several_connections_still_leave_the_cluster_out_of_the_title() {
        assert_eq!(title(&scope(kind("Pod", true), 2)), "Pods");
        assert_eq!(title(&scope(kind("Deployment", true), 3)), "Deployments");
    }

    /// Error text goes through markdown (the only selectable-text primitive), so
    /// it must come out verbatim: escaped prose, and detail fenced so backticks and
    /// backslashes survive.
    #[test]
    fn error_text_is_rendered_verbatim() {
        use super::{code_block, escape_markdown};
        assert_eq!(escape_markdown("continue_: None"), "continue\\_: None");
        assert_eq!(escape_markdown("a*b"), "a\\*b");
        assert_eq!(code_block(r#"pods \"x\""#), "```text\npods \\\"x\\\"\n```");
        assert_eq!(code_block("a ```` b"), "`````text\na ```` b\n`````");
    }

    /// An item heading names the context only while the window uses several.
    #[test]
    fn a_heading_names_the_context_only_for_several() {
        use super::heading_context;
        let pod = scope(NavTarget::pod("default", "web-1"), 1);
        assert_eq!(heading_context(&pod, 1), None);
        assert_eq!(heading_context(&pod, 2), Some("kind-dev".to_string()));
    }

    /// The Logs view is a target of its own and gets the same rule.
    #[test]
    fn the_logs_view_titles_by_the_same_rule() {
        assert_eq!(title(&scope(NavTarget::Logs, 1)), "Logs");
        assert_eq!(title(&scope(NavTarget::Logs, 2)), "Logs");
    }

    /// Section 2.2: a panel over one pod names the pod, so two pods' detail
    /// panels are told apart in the dock's tabs rather than both reading "Pod".
    /// The plural-for-a-list rule still holds next to it.
    #[test]
    fn a_pods_detail_panel_names_its_pod() {
        let pod_scope = scope(NavTarget::pod("default", "api-7d9f-ftg5t"), 1);
        assert_eq!(title(&pod_scope), "Pod: api-7d9f-ftg5t");

        let other = scope(NavTarget::pod("default", "web-1"), 1);
        assert_ne!(title(&pod_scope), title(&other), "two pods, two names");

        let with_cluster = scope(NavTarget::pod("default", "api-7d9f-ftg5t"), 2);
        assert_eq!(title(&with_cluster), "Pod: api-7d9f-ftg5t");
    }

    /// A pod is namespaced whatever discovery said about any kind, so a panel
    /// over one is namespaced rather than silently scoping itself out of one.
    #[test]
    fn a_pod_detail_panel_is_namespaced() {
        assert!(scope(NavTarget::pod("kube-system", "coredns-1"), 1).is_namespaced());
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
