//! The Resource panel: a connected window's list of every resource kind its
//! cluster's API discovery reports, CRDs included.
//!
//! Section 8.1 of the `cluster-picker-and-navigation` change replaces the fixed
//! Pods/Logs sidebar with this. It owns no connection of its own - it reaches
//! the window's context through [`ClusterRegistry`], so the list and the panels
//! it opens all share one `ClusterSession` and switching between them never
//! reconnects.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::{DiscoveredKind, discover_kinds};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::sidebar::{Sidebar, SidebarMenuItem};
use gpui_kit::*;

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
    /// Kept, rather than `.detach()`ed, so [`Self::set_active_context`] can
    /// replace it: switching the active context means observing a *different*
    /// connection, and the old subscription must stop firing into a state that
    /// no longer describes what `header` shows.
    _connection_observation: Subscription,
}

impl ResourcePanel {
    /// `contexts` is the window's full context list (`window-context-bar` design.md
    /// decision 4), so the cluster dropdown always lists every context the window
    /// uses, not just the one `context_name` starts on.
    pub fn new(context_name: String, contexts: Vec<String>, cx: &mut Context<Self>) -> Self {
        let connection = ClusterRegistry::connection(cx, &context_name);
        Self::with_connection(context_name, contexts, connection, cx)
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
        cx: &mut Context<Self>,
    ) -> Self {
        let observation = Self::observe(&connection, cx);
        let mut this = Self {
            context_name,
            contexts,
            state: ResourceState::WaitingForConnection,
            loading: false,
            selected: None,
            _connection_observation: observation,
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
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_connection(context_name, contexts, connection, cx)
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

    /// The sidebar's rows while there is nothing to list yet - a plain
    /// non-interactive row per state, so the panel's width and placement stay
    /// the same whether or not discovery has landed.
    fn status_row(label: String) -> SidebarMenuItem {
        SidebarMenuItem::new(label).disable(true)
    }

    /// The row for each discovered kind: its label, the target selecting it
    /// opens, and whether that target is the one currently showing. Splitting
    /// this out of `render_kinds` is what makes the list assertable - the
    /// rendered rows are otherwise only reachable by simulating a click.
    fn rows(&self, kinds: &[DiscoveredKind]) -> Vec<(String, NavTarget, bool)> {
        kinds
            .iter()
            .map(|kind| {
                let target = NavTarget::Kind(kind.clone());
                let active = self.selected.as_ref() == Some(&target);
                (target.label(), target, active)
            })
            .collect()
    }

    /// Asks the window to show `target`. The single request path behind both
    /// ways of opening a row: the double-click (9.1) and the context menu's
    /// "Open" (9.2). They differ only in how they get here, which is what
    /// makes them equivalent.
    fn request_open(&mut self, target: NavTarget, cx: &mut Context<Self>) {
        cx.emit(ResourceEvent::Open(target));
        cx.notify();
    }

    fn render_kinds(&self, kinds: &[DiscoveredKind], cx: &mut Context<Self>) -> AnyElement {
        let this = cx.weak_entity();
        let items: Vec<SidebarMenuItem> = self
            .rows(kinds)
            .into_iter()
            .map(|(label, target, active)| {
                let dbl = this.clone();
                let opened = target.clone();
                let menu_target = target.clone();
                let menu_panel = this.clone();
                SidebarMenuItem::new(label)
                    .icon(target.icon())
                    .active(active)
                    // A single click only arms the row; the second click of a
                    // double-click opens the panel (9.1).
                    .on_click(move |event, _window, cx| {
                        if event.click_count() < 2 {
                            return;
                        }
                        let _ = dbl.update(cx, |this, cx| this.request_open(opened.clone(), cx));
                    })
                    .context_menu(move |menu, _window, _cx| {
                        let target = menu_target.clone();
                        let panel = menu_panel.clone();
                        menu.item(PopupMenuItem::new("Open").on_click(
                            move |_event, _window, cx| {
                                let _ = panel
                                    .update(cx, |this, cx| this.request_open(target.clone(), cx));
                            },
                        ))
                    })
            })
            .collect();
        Sidebar::new("resources")
            .w_full()
            .collapsible(false)
            .header(self.header(cx))
            .children(items)
            .into_any_element()
    }

    /// The sidebar's header: which cluster's resources these rows are, and - once
    /// the window holds more than one - a real dropdown of every context this
    /// window uses (`window-context-bar` design.md decision 4). Picking a different
    /// one only *asks*: it emits [`ResourceEvent::SwitchContext`] rather than
    /// switching locally, so `active` is always written in exactly one place,
    /// `MainWindow`, whether the request came from here or from a chip click.
    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        // One row: "Resources" on the left, the context (a dropdown once the window
        // holds several) in the right corner, sharing the label's text baseline.
        div()
            .w_full()
            .flex()
            .items_baseline()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(theme.sidebar_foreground)
                    .child("Resources"),
            )
            // `ml_auto` as well as `justify_between`: the selector stays in the right
            // corner even if the sidebar's header slot doesn't stretch this row.
            .child(
                div()
                    .ml_auto()
                    .min_w_0()
                    .child(if self.shows_cluster_dropdown() {
                        self.cluster_dropdown(cx)
                    } else {
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(self.context_name.clone())
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn cluster_dropdown(&self, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.weak_entity();
        let contexts = self.contexts.clone();
        let current = self.context_name.clone();
        Button::new("resource-cluster")
            .label(self.context_name.clone())
            .icon(gpui_kit::assets::IconName::ChevronDown)
            .xsmall()
            .ghost()
            .dropdown_menu(move |menu, _window, _cx| {
                // Built per open, not hoisted: `PopupMenuItem` is not `Clone`, and
                // this closure is `Fn` so it can run more than once - the same
                // shape as `ui/picker_tunnel.rs::selector`.
                let mut menu = menu;
                for context_name in &contexts {
                    let picked = context_name.clone();
                    let panel = this.clone();
                    menu = menu.item(
                        PopupMenuItem::new(context_name.clone())
                            .checked(*context_name == current)
                            .on_click(move |_event, _window, cx| {
                                let _ = panel.update(cx, |_panel, cx| {
                                    cx.emit(ResourceEvent::SwitchContext(picked.clone()));
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }
}

impl EventEmitter<ResourceEvent> for ResourcePanel {}

impl Render for ResourcePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.state {
            ResourceState::Loaded(kinds) if kinds.is_empty() => Sidebar::new("resources")
                .w_full()
                .collapsible(false)
                .header(self.header(cx))
                .child(Self::status_row(
                    "This cluster reported no resource kinds.".to_string(),
                ))
                .into_any_element(),
            ResourceState::Loaded(kinds) => self.render_kinds(kinds, cx),
            ResourceState::WaitingForConnection => Sidebar::new("resources")
                .w_full()
                .collapsible(false)
                .header(self.header(cx))
                .child(Self::status_row("Connecting...".to_string()))
                .into_any_element(),
            ResourceState::Loading => Sidebar::new("resources")
                .w_full()
                .collapsible(false)
                .header(self.header(cx))
                .child(Self::status_row(
                    "Discovering resource kinds...".to_string(),
                ))
                .into_any_element(),
            ResourceState::Failed(reason) => Sidebar::new("resources")
                .w_full()
                .collapsible(false)
                .header(self.header(cx))
                .child(Self::status_row(format!(
                    "Could not discover resource kinds: {reason}"
                )))
                .into_any_element(),
        }
    }
}

// Not `use super::*;` in the sibling test module: `gpui_kit::*`'s huge re-export
// surface (all of `gpui`/`gpui-component`), combined with `IconName`'s ~2500 variants
// and a `#[gpui_kit::test]`-annotated item, blows this toolchain's macro-expansion
// budget - the same crash `ui/status_bar.rs` documents. Split into a sibling
// `resource/tests.rs` (rather than kept inline) once `window-context-bar`'s
// `set_active_context` tests pushed this file toward the 700-line cap.
#[cfg(test)]
mod tests;
