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
use gpui_kit::component::dock::Panel;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use std::rc::Rc;

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

/// A detail panel header's title block led by its resource's kind icon, at
/// [`IconSize::Header`](crate::ui::icon::IconSize::Header) (`resource-kind-icons`
/// 3.5). Shared by the pod detail and object viewer headers.
pub fn with_header_icon(
    icon: crate::ui::icon::KindIcon,
    heading: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    use crate::ui::icon::{self, IconSize};
    let space = crate::ui::space::spacing(cx);
    div()
        .flex()
        .items_center()
        .gap(space.control_gap)
        .min_w_0()
        .child(
            div()
                .flex_none()
                .debug_selector(|| "detail-header-icon".into())
                .child(icon::kind_icon(icon, IconSize::Header, window, cx)),
        )
        .child(div().flex_1().min_w_0().child(heading))
        .into_any_element()
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

/// `text` as the panel's title element, with a "Context: <name>" tooltip (led
/// by the API group for a custom resource list, see [`tooltip::tooltip_lines`]). The dock
/// draws this in the tab (see [`tab_name`]) and in the title bar.
///
/// It is also the panel's focus indicator: underlined in the accent colour
/// (the user's system accent on macOS) while focus is anywhere inside the panel
/// `focus_handle` belongs to - `contains_focused`, not `is_focused`: a panel
/// whose content takes focus itself (a table row, a text input) moves the
/// window's focus to that child, and an indicator lit only while the panel's
/// own handle held focus would go dark the moment the panel was used. A press
/// on it focuses that panel.
///
/// The tab is the one place the dock lets a panel mark itself: the tab strip
/// draws this element, but reads no per-panel style (`Panel::title_style` only
/// reaches the single-panel title bar), so the tab's own background can't be
/// coloured from here.
///
/// The panel's kind icon leads the title (`resource-kind-icons` 3.1), sized to
/// the tab's `text_sm` label. It's decorative, so the tab's keyboard route is
/// unchanged.
pub fn title_element(
    scope: &PanelScope,
    text: String,
    focus_handle: &FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    use crate::ui::icon::{self, IconSize};
    let focused = focus_handle.contains_focused(window, cx);
    let tooltip_scope = scope.clone();
    let kind_icon = icon::kind_icon(icon::for_target(&scope.target), IconSize::Small, window, cx);
    div()
        .id(SharedString::from(format!(
            "panel-title-{}-{text}",
            scope.context_name
        )))
        // The underline's width is reserved while unfocused too, so moving
        // focus between panels doesn't shift any tab's label.
        .border_b_2()
        .border_color(focus_underline(focused, crate::ui::style::accent(cx)))
        .debug_selector(|| {
            let state = if focused { "focused" } else { "unfocused" };
            format!("panel-title-{text}-{state}")
        })
        .flex()
        .items_center()
        .gap_1p5()
        .child(kind_icon)
        .child(text)
        .tooltip(move |window, cx| title_tooltip(&tooltip_scope, window, cx))
        // gpui-base 0.7.0's `TabGroup::select_tab` (dock/tab_group.rs) returns
        // early when the clicked tab is already the active one, before its
        // `focus_active_panel`, so a click on the displayed tab focused nothing
        // - a new or restored tab took focus only after switching away and back.
        // Focusing here covers the label and icon. Once gpui-kit focuses on that
        // path itself, this can come out. Not stopping propagation: the tab still
        // selects, and a drag still starts, as before.
        .on_mouse_down(MouseButton::Left, {
            let focus_handle = focus_handle.clone();
            move |_, window, cx| window.focus(&focus_handle, cx)
        })
        .into_any_element()
}

/// The colour of a title's focus underline: `accent` (see
/// [`crate::ui::accent::focus_accent`]) while its panel holds focus, otherwise
/// nothing visible.
fn focus_underline(focused: bool, accent: Hsla) -> Hsla {
    if focused { accent } else { transparent_black() }
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

/// The close button every resource panel's title bar carries. It closes
/// `panel`, the panel whose title bar draws it.
///
/// It names its panel rather than dispatching the dock's `ClosePanel`, which
/// travels the focus path and so reached the *focused* group: with two groups
/// stacked, the lower group's close closed the upper group's focused panel
/// (`tab-close-buttons`). Closing by panel also reaches the window's last
/// panel, which the tab group's own close refuses to empty the dock for, so
/// the window can return to the picker.
pub fn close_button<P: Panel>(panel: Entity<P>) -> Button {
    let selector = format!("panel-close-{}", panel.entity_id());
    Button::new("panel-close")
        .icon(IconName::Close)
        .xsmall()
        .ghost()
        .tab_stop(false)
        .tooltip("Close panel")
        .debug_selector(move || selector.clone())
        .on_click(move |_event, window, cx| {
            crate::util::shell::close_panel(panel.clone(), window, cx)
        })
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
pub fn toolbar_buttons<P: Panel>(panel: Entity<P>) -> Option<Vec<Button>> {
    Some(vec![close_button(panel)])
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
    let space = crate::ui::space::spacing(cx);
    div()
        .size_full()
        .p(space.panel_inset)
        .flex()
        .flex_col()
        .gap(space.control_gap)
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

mod tooltip;
use tooltip::title_tooltip;
#[cfg(test)]
use tooltip::tooltip_lines;

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
