//! A namespaced panel's namespace picker (`namespace-picker-filter`): the
//! title-bar button reading the panel's scope, opening the filterable
//! [`NamespaceFilter`] list in a popover.
//!
//! A toggle keeps the popover open, filter and all, so several namespaces can
//! be picked in a row. The filter is empty each time it opens. The panel's
//! scope stays the one source of truth: the picker is handed the scope and
//! the cluster's namespaces on every render ([`NamespacePickerSlot::element`])
//! and reports each toggle through the panel's `on_pick`, so a scope changed
//! elsewhere - a warp command - is what the next open shows.
//!
//! The keyboard opens it with [`PickNamespaces`], registered per panel by
//! [`pick_namespaces_command`] so the palette offers it in each panel that has
//! a picker. Focus lands in the filter; Escape clears it, then closes, and
//! focus goes back where it was.

use crate::command::Command;
use crate::ui::namespace_filter::{NamespaceFilter, OnPick, OnQuery};
use crate::ui::namespace_sets::store::NamespaceSets;
use crate::ui::panel_title::PanelScope;
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::*;
use std::rc::Rc;

actions!(namespace_picker, [PickNamespaces]);

/// [`PickNamespaces`]'s default key in each panel with a picker.
pub const PICK_NAMESPACES_KEY: &str = "n";

/// What a pick does to the panel: re-scope it to the namespaces given.
type OnScope = Rc<dyn Fn(Vec<String>, &mut App)>;

/// The Pick Namespaces command for the panel whose key context is `context`.
/// Not in the menu bar: it acts on the focused panel, and the menu bar holds
/// only commands that don't depend on focus.
pub fn pick_namespaces_command(
    id: &'static str,
    title: &'static str,
    context: &'static str,
) -> Command {
    Command {
        id,
        title,
        default_binding: PICK_NAMESPACES_KEY,
        context: Some(context),
        action: Box::new(PickNamespaces),
        menu: None,
    }
}

/// The label a namespace scope reads on the picker's button.
pub fn label_for(namespaces: &[String]) -> String {
    match namespaces {
        [] => "All namespaces".to_string(),
        [namespace] => namespace.clone(),
        namespaces => format!("{} namespaces", namespaces.len()),
    }
}

/// What the button reads: the saved namespace set holding exactly
/// `namespaces`, by name (`namespace-sets`), or else [`label_for`]. A panel
/// keeps the namespaces it was switched to, not the set, so a set edited or
/// deleted since reads as the count again.
pub fn scope_label(namespaces: &[String], cx: &App) -> String {
    NamespaceSets::get(cx)
        .matching(namespaces)
        .map(|set| set.name.clone())
        .unwrap_or_else(|| label_for(namespaces))
}

/// The picker: its popover's open state and filter, and what the panel last
/// handed it.
pub struct NamespacePicker {
    filter: NamespaceFilter,
    open: bool,
    current: Vec<String>,
    namespaces: Vec<String>,
    on_scope: OnScope,
}

impl NamespacePicker {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The button names a matching set, so a set's edit is redrawn here.
        cx.observe_global::<NamespaceSets>(|_, cx| cx.notify())
            .detach();
        Self {
            filter: NamespaceFilter::new(window, cx),
            open: false,
            current: Vec::new(),
            namespaces: Vec::new(),
            on_scope: Rc::new(|_, _| {}),
        }
    }

    /// Opens the popover with an empty filter.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_open(true, window, cx);
    }

    /// Whether the popover is open.
    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open && !self.open {
            self.filter.reset(window, cx);
        }
        self.open = open;
        cx.notify();
    }

    fn pick(&mut self, next: Vec<String>, cx: &mut Context<Self>) {
        // Shown at once; the panel's next render hands the same scope back.
        self.current = next.clone();
        (self.on_scope)(next, cx);
        cx.notify();
    }
}

impl Render for NamespacePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let button = Button::new("panel-namespace")
            .label(scope_label(&self.current, cx))
            .icon(IconName::ChevronDown)
            .xsmall()
            .ghost()
            .tab_stop(false)
            .tooltip("Namespace");
        let on_pick: OnPick = Rc::new({
            let this = this.clone();
            move |next, _window, cx| {
                let _ = this.update(cx, |picker, cx| picker.pick(next, cx));
            }
        });
        let on_query: OnQuery = Rc::new({
            let this = this.clone();
            move |_window, cx| {
                let _ = this.update(cx, |_, cx| cx.notify());
            }
        });
        let filter = self.filter.clone();
        let namespaces = self.namespaces.clone();
        let current = self.current.clone();
        Popover::new("panel-namespace-popover")
            .anchor(Anchor::TopRight)
            .open(self.open)
            .on_open_change(move |open, window, cx| {
                let _ = this.update(cx, |picker, cx| picker.set_open(*open, window, cx));
            })
            .trigger(button)
            .track_focus(&self.filter.focus_handle(cx))
            .content(move |_state, _window, cx| {
                div().w(px(280.)).child(filter.element(
                    &namespaces,
                    &current,
                    on_pick.clone(),
                    on_query.clone(),
                    cx,
                ))
            })
    }
}

/// A panel's picker, made on the panel's first render - it needs a window -
/// and kept for the panel's life.
#[derive(Default)]
pub struct NamespacePickerSlot(Option<Entity<NamespacePicker>>);

impl NamespacePickerSlot {
    /// The picker for `scope` over `namespaces`, or nothing for a
    /// cluster-scoped kind. `on_pick` re-scopes the panel.
    ///
    /// The returned `None` is what omits the picker for cluster-scoped kinds,
    /// so the presence rule and the rendering are one decision rather than two
    /// that can disagree.
    pub fn element<V: 'static>(
        &mut self,
        scope: &PanelScope,
        namespaces: &[String],
        on_pick: impl Fn(Vec<String>, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Option<AnyElement> {
        if !scope.is_namespaced() {
            return None;
        }
        let picker = self
            .0
            .get_or_insert_with(|| cx.new(|cx| NamespacePicker::new(window, cx)))
            .clone();
        picker.update(cx, |picker, _| {
            picker.current = scope.namespaces.clone();
            picker.namespaces = namespaces.to_vec();
            picker.on_scope = Rc::new(on_pick);
        });
        Some(picker.into_any_element())
    }

    /// Opens the picker - [`PickNamespaces`]. Nothing before it is first drawn.
    pub fn open(&self, window: &mut Window, cx: &mut App) {
        if let Some(picker) = &self.0 {
            picker.update(cx, |picker, cx| picker.open(window, cx));
        }
    }

    /// The picker, once drawn.
    #[cfg(test)]
    pub fn picker(&self) -> Option<&Entity<NamespacePicker>> {
        self.0.as_ref()
    }
}

#[cfg(test)]
mod tests;
