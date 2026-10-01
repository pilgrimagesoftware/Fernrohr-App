//! The Resource panel: a connected window's list of every resource kind its
//! cluster's API discovery reports, CRDs included.
//!
//! Section 8.1 of the `cluster-picker-and-navigation` change replaces the fixed
//! Pods/Logs sidebar with this. It owns no connection of its own - it reaches
//! the window's context through [`ClusterRegistry`], so the list and the panels
//! it opens all share one `ClusterSession` and switching between them never
//! reconnects.
//!
//! `resource-panel-grouping` groups the flat list into fixed-order category
//! sections ([`category`]/[`section`]) with a bottom-pinned filter, and makes
//! the whole thing keyboard-operable ([`keyboard`]/[`actions`]). This file stays
//! wiring and state; [`render`] draws it and [`actions`] answers the keyboard.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::{DiscoveredKind, discover_kinds};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use category::Category;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use std::collections::HashSet;

mod actions;
mod category;
mod keyboard;
mod render;
mod section;

pub(crate) use actions::{FocusResources, register_commands};

/// The panel's own keybindings (Up/Down/Enter/Left/Right), in its own key
/// context - `/` is not here, see `actions::register_commands`'s doc comment.
pub fn panel_bindings() -> [KeyBinding; 2] {
    keyboard::panel_bindings()
}

/// Emitted when the user picks a row, so the window can open that kind's
/// panel. Which panel that is stays [`NavTarget`]'s decision - the Resource
/// panel only reports what was picked.
#[derive(Clone)]
pub enum ResourceEvent {
    /// A kind the user selected, wrapped as the target the window should show.
    Open(NavTarget),
    /// `window-context-bar` design.md decision 4: the cluster dropdown picked a
    /// different context. The window is the one source of truth for `active` (the
    /// context bar's chips write it too), so this only *asks* - `MainWindow` calls
    /// back through [`Self::set_active_context`] to actually apply it, the same way
    /// a chip click does.
    SwitchContext(String),
}

/// How far the list has got. Discovery needs a client, so the panel spends its
/// first moments in `WaitingForConnection` - between the user picking a context
/// and the probe landing - and only then asks what kinds exist.
enum ResourceState {
    WaitingForConnection,
    Loading,
    Loaded(Vec<DiscoveredKind>),
    Failed(String),
}

pub struct ResourcePanel {
    context_name: String,
    contexts: Vec<String>,
    state: ResourceState,
    /// Whether a discovery request is in flight. The request itself is
    /// detached - nothing needs to cancel it, the panel is its only reader -
    /// so this flag is what stops a connection that flaps from racing two
    /// results into `state`.
    loading: bool,
    /// The kind the window's active panel is showing, so its row is marked
    /// active. Set by the window whenever it switches panels.
    selected: Option<NavTarget>,
    /// The row the keyboard cursor is on and the last click landed on -
    /// `.claude/rules/keyboard-first.md`'s "one selection model", distinct
    /// from `selected` (the window's *open* panel): a row can be highlighted
    /// without being open yet, and Enter opens whatever is highlighted.
    highlighted: Option<NavTarget>,
    /// Categories the user collapsed, this window only. Section 2.3: default
    /// expanded, never written to the preference file.
    collapsed: HashSet<Category>,
    /// API groups the user collapsed inside Custom Resources
    /// (`custom-resource-grouping`), this window only - beside `collapsed`, and
    /// like it starting empty (every subgroup expanded) and never saved.
    collapsed_subgroups: HashSet<String>,
    /// The bottom-pinned filter box's text field (section 3.1).
    filter_input: Entity<InputState>,
    focus_handle: FocusHandle,
    /// Kept, rather than `.detach()`ed, so [`Self::set_active_context`] can
    /// replace it: switching the active context means observing a *different*
    /// connection, and the old subscription must stop firing into a state that
    /// no longer describes what `header` shows.
    _connection_observation: Subscription,
    /// Re-renders on every keystroke in the filter. `InputState::set_value`
    /// (used to clear it on Escape - see `actions::on_action_clear_filter`)
    /// does not emit `InputEvent::Change`, so that path calls `cx.notify()`
    /// itself instead of relying on this.
    _filter_observation: Subscription,
}

impl ResourcePanel {
    /// Moves keyboard focus onto the kind list, so Up/Down/Enter/Left/Right and `/`
    /// work without a click first.
    pub(crate) fn focus_list(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
    }

    /// The panel's focus handle: the one Focus Next / Previous Panel steps to,
    /// and which the filter field sits inside.
    pub(crate) fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    /// Test-only: shows `kinds` as discovered, so a test outside this module
    /// gets the loaded panel - list and filter field - without a cluster.
    #[cfg(test)]
    pub(crate) fn test_show_kinds(&mut self, kinds: Vec<DiscoveredKind>, cx: &mut Context<Self>) {
        self.state = ResourceState::Loaded(kinds);
        cx.notify();
    }

    /// Whether the kind list holds keyboard focus.
    #[cfg(test)]
    pub(crate) fn is_list_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    /// `contexts` is the window's full context list (`window-context-bar` design.md
    /// decision 4), so the cluster dropdown always lists every context the window
    /// uses, not just the one `context_name` starts on.
    pub fn new(
        context_name: String,
        contexts: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let connection = ClusterRegistry::connection(cx, &context_name);
        Self::with_connection(context_name, contexts, connection, window, cx)
    }

    /// Construction from an explicit connection, so tests can hand in a stub.
    ///
    /// [`Self::new`] has to source the connection from `ClusterRegistry`, which
    /// starts a *real* connect: a tokio task that gpui's test scheduler reports as
    /// cross-thread nondeterminism if it's still in flight when the test ends, and
    /// whose timing depends on the machine's real kubeconfig (a context that fails
    /// fast on CI, a live one locally). Tests drive `state` directly and never
    /// need that, so they pass a connection that stays in a non-`Connected` state -
    /// which also keeps `sync` from kicking off real discovery.
    fn with_connection(
        context_name: String,
        contexts: Vec<String>,
        connection: Entity<ClusterConnection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let observation = Self::observe(&connection, cx);
        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter kinds..."));
        let filter_observation =
            cx.subscribe(&filter_input, |_this: &mut Self, _input, event, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            });
        let mut this = Self {
            context_name,
            contexts,
            state: ResourceState::WaitingForConnection,
            loading: false,
            selected: None,
            highlighted: None,
            collapsed: HashSet::new(),
            collapsed_subgroups: HashSet::new(),
            filter_input,
            focus_handle: cx.focus_handle(),
            _connection_observation: observation,
            _filter_observation: filter_observation,
        };
        this.sync(&connection, cx);
        this
    }

    fn observe(connection: &Entity<ClusterConnection>, cx: &mut Context<Self>) -> Subscription {
        cx.observe(connection, |this: &mut Self, connection, cx| {
            this.sync(&connection, cx)
        })
    }

    /// `window-context-bar` design.md decision 4: switches which context the panel
    /// lists kinds for - a chip click, or this panel's own cluster dropdown, both
    /// land here through `MainWindow` (the one place `active` is written, so the
    /// two stay in sync). A no-op when `context_name` is already the one showing,
    /// so a dropdown pick of the current cluster doesn't restart discovery.
    ///
    /// `contexts` is taken too because the window's context list can have changed
    /// in the same edit that switched `active` (adding or disconnecting a
    /// context), and the dropdown's own list - which reads `contexts` - must never
    /// show a context the window no longer uses, or omit one it just added.
    pub(crate) fn set_active_context(
        &mut self,
        context_name: String,
        contexts: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.contexts = contexts;
        if self.context_name == context_name {
            cx.notify();
            return;
        }
        self.context_name = context_name;
        self.state = ResourceState::WaitingForConnection;
        self.loading = false;
        self.selected = None;
        self.highlighted = None;
        let connection = ClusterRegistry::connection(cx, &self.context_name);
        self._connection_observation = Self::observe(&connection, cx);
        self.sync(&connection, cx);
        cx.notify();
    }

    #[cfg(test)]
    fn with_contexts(
        context_name: String,
        contexts: Vec<String>,
        connection: Entity<ClusterConnection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_connection(context_name, contexts, connection, window, cx)
    }

    fn shows_cluster_dropdown(&self) -> bool {
        self.contexts.len() > 1
    }

    /// Marks `target`'s row active, or clears the mark when given `None` (the
    /// window has no panel open for this context any more).
    pub fn set_selected(&mut self, target: Option<NavTarget>, cx: &mut Context<Self>) {
        if self.selected == target {
            return;
        }
        self.selected = target;
        cx.notify();
    }

    /// Moves the keyboard/click cursor to `target` without opening anything -
    /// what a single click and Up/Down both do (section 4.1's "one selection").
    fn set_highlighted(&mut self, target: Option<NavTarget>, cx: &mut Context<Self>) {
        if self.highlighted == target {
            return;
        }
        self.highlighted = target;
        cx.notify();
    }

    /// The kinds currently listed. Read by the tests, which assert the list
    /// rather than the rendered rows - the rows themselves are only reachable
    /// by simulating a click.
    #[cfg(test)]
    pub fn kinds(&self) -> Option<&[DiscoveredKind]> {
        match &self.state {
            ResourceState::Loaded(kinds) => Some(kinds),
            _ => None,
        }
    }

    /// The currently highlighted row, if any - what a keystroke test asserts
    /// moved.
    #[cfg(test)]
    pub(crate) fn highlighted(&self) -> Option<&NavTarget> {
        self.highlighted.as_ref()
    }

    /// Whether `kind`'s own section is collapsed right now - the stored
    /// collapse state a test reads after Left/Right or a header click, kept
    /// separate from what a filter is currently forcing `render` to show.
    #[cfg(test)]
    pub(crate) fn is_section_collapsed(&self, kind: &DiscoveredKind) -> bool {
        self.collapsed
            .contains(&Category::for_gvk(&kind.gvk.group, &kind.plural))
    }

    fn loaded_kinds(&self) -> &[DiscoveredKind] {
        match &self.state {
            ResourceState::Loaded(kinds) => kinds,
            _ => &[],
        }
    }

    fn filter_text(&self, cx: &App) -> String {
        self.filter_input.read(cx).value().to_string()
    }

    /// The sections to render right now: `loaded_kinds` partitioned by
    /// category, with the current filter and collapse state applied. The
    /// single source both `render` and the keyboard handlers read, so Up/Down
    /// can never step through a row `render` would not draw.
    fn visible_sections(&self, cx: &App) -> Vec<section::VisibleSection> {
        section::visible_sections(
            self.loaded_kinds(),
            &self.collapsed,
            &self.collapsed_subgroups,
            &self.filter_text(cx),
        )
    }

    /// The kind `highlighted` points at, or `None` when nothing is
    /// highlighted - every row this panel lists is a [`NavTarget::Kind`], so
    /// this is the one match arm that can ever be `Some`.
    fn highlighted_kind(&self) -> Option<DiscoveredKind> {
        match &self.highlighted {
            Some(NavTarget::Kind(kind)) => Some(kind.clone()),
            _ => None,
        }
    }

    /// The highlighted row's own section - what Left/Right collapse or expand.
    fn highlighted_category(&self) -> Option<Category> {
        self.highlighted_kind()
            .map(|kind| Category::for_gvk(&kind.gvk.group, &kind.plural))
    }

    /// Flips `category`'s collapsed state - the section header's click route,
    /// mirroring Left/Right's direction-specific `actions` handlers.
    fn toggle_section(&mut self, category: Category, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&category) {
            self.collapsed.insert(category);
        }
        cx.notify();
    }

    /// Starts discovery as soon as the window's context has a client. A no-op
    /// while the state is already settled or a request is in flight.
    fn sync(&mut self, connection: &Entity<ClusterConnection>, cx: &mut Context<Self>) {
        if self.loading || matches!(self.state, ResourceState::Loaded(_)) {
            return;
        }
        let ConnectionState::Connected(client) = &connection.read(cx).state else {
            return;
        };
        self.load(client.clone(), cx);
    }

    fn load(&mut self, client: kube::Client, cx: &mut Context<Self>) {
        self.loading = true;
        self.state = ResourceState::Loading;
        let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
            let _ = tx.send(discover_kinds(client).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.loading = false;
                    this.state = match result {
                        Ok(kinds) => ResourceState::Loaded(kinds),
                        Err(error) => ResourceState::Failed(error.to_string()),
                    };
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }

    /// The row for each discovered kind: its label, the target selecting it
    /// opens, and whether that target is the one currently showing. Splitting
    /// this out of `render`'s row-building is what makes the list assertable -
    /// the rendered rows are otherwise only reachable by simulating a click.
    ///
    /// A row reads just its kind (`Deployment`); the API group lives in the row's
    /// tooltip ([`api_version_label`]). Only when two kinds in `kinds` share a name
    /// does each keep its group in the label, so the two rows stay distinct.
    fn rows(&self, kinds: &[DiscoveredKind]) -> Vec<(String, NavTarget, bool)> {
        kinds
            .iter()
            .map(|kind| {
                let target = NavTarget::Kind(kind.clone());
                let active = self.selected.as_ref() == Some(&target);
                let shared_name = kinds
                    .iter()
                    .filter(|other| other.gvk.kind == kind.gvk.kind)
                    .count()
                    > 1;
                let label = if shared_name {
                    kind.label()
                } else {
                    kind.gvk.kind.clone()
                };
                (label, target, active)
            })
            .collect()
    }

    /// Asks the window to show `target`. The single request path behind every
    /// way of opening a row: the double-click (9.1), the context menu's "Open"
    /// (9.2), and now `Enter` (section 4.1) - they differ only in how they get
    /// here, which is what makes them equivalent. Also marks `target`
    /// highlighted, so opening a row from the context menu leaves the
    /// keyboard cursor pointing at the panel it just opened.
    fn request_open(&mut self, target: NavTarget, cx: &mut Context<Self>) {
        self.highlighted = Some(target.clone());
        cx.emit(ResourceEvent::Open(target));
        cx.notify();
    }
}

impl EventEmitter<ResourceEvent> for ResourcePanel {}

/// A kind's API version for its row tooltip: `apps/v1`, or `v1 (core)` for the core
/// group, which has no name of its own.
pub(super) fn api_version_label(kind: &DiscoveredKind) -> String {
    if kind.gvk.group.is_empty() {
        format!("{} (core)", kind.gvk.version)
    } else {
        format!("{}/{}", kind.gvk.group, kind.gvk.version)
    }
}

// Not `use super::*;` in the sibling test module: `gpui_kit::*`'s huge re-export
// surface (all of `gpui`/`gpui-component`), combined with `IconName`'s ~2500 variants
// and a `#[gpui_kit::test]`-annotated item, blows this toolchain's macro-expansion
// budget - the same crash `ui/status_bar.rs` documents. Split into a sibling
// `resource/tests.rs` (rather than kept inline) once `window-context-bar`'s
// `set_active_context` tests pushed this file toward the line cap.
#[cfg(test)]
mod tests;
