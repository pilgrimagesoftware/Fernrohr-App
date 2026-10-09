//! The events browser dock panel: its state, its Event watch subscription and
//! its dock integration. What it draws is `render`'s; how its columns behave is
//! `table`'s.

use super::columns::EventColumn;
use super::filters::EventFilters;
use super::row::EventRow;
use super::store::EventsTable;
use super::table::{DEFAULT_SORT, EventsTableDelegate, reselect};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery_registry::{DiscoveredKinds, DiscoveryRegistry};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::list_search::{self, ListSearch};
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState,
};
use gpui_kit::component::input::InputState;
use gpui_kit::component::table::{ColumnSort, TableEvent, TableState};
use gpui_kit::*;

/// The search box's placeholder - also what it matches on (reason, involved
/// object, message), design D3's interim substring filter.
pub(super) const SEARCH_PLACEHOLDER: &str = "Search reason, object, message...";

/// A dock panel listing one context's retained events, live, newest first,
/// narrowed to its namespace selection.
pub struct EventsPanel {
    /// Its target is the core Event kind, so the window keys it like any other
    /// kind's panel.
    pub(super) scope: PanelScope,
    pub(super) connection: Entity<ClusterConnection>,
    /// The context's shared Event table - this panel's own empty one until it
    /// subscribes.
    pub(super) events: Entity<EventsTable>,
    pub(super) namespaces: Entity<NamespaceList>,
    pub(super) subscribed: bool,
    pub(super) focus_handle: FocusHandle,
    /// The title bar's namespace picker, made on first render.
    pub(super) namespace_picker: crate::ui::namespace_picker::NamespacePickerSlot,
    /// Built on first render, which is the first time there's a `Window`.
    pub(super) table: Option<Entity<TableState<EventsTableDelegate>>>,
    /// The sort to start the table with, once it's built - the saved one, or
    /// [`DEFAULT_SORT`].
    pub(super) initial_sort: Option<(EventColumn, ColumnSort)>,
    /// The type, kind and reason filters, saved with the panel.
    pub(super) filters: EventFilters,
    /// The search box: its lazily built `InputState`, and the text to start
    /// it with once restored (`saved-panel-layouts` 1.6, `list-search` #189).
    pub(super) search: ListSearch,
    /// The context's discovery, which decides which involved objects link.
    pub(super) discovery: Entity<DiscoveredKinds>,
}

impl EventsPanel {
    /// Scopes this panel to `namespaces` (empty for all), as its own picker does -
    /// what Warp All to Namespace applies to every namespaced panel in a context.
    pub(crate) fn set_namespaces(&mut self, namespaces: Vec<String>, cx: &mut Context<Self>) {
        self.scope = self.scope.scoped_to(namespaces.clone());
        cx.emit(crate::ui::panel_title::ScopeEvent::NamespacesChanged(
            namespaces,
        ));
        cx.notify();
    }
    pub fn new(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        let namespaces =
            crate::k8s::cluster::namespaces::NamespaceRegistry::list(cx, &scope.context_name);
        let discovery = DiscoveryRegistry::kinds(cx, &scope.context_name);
        cx.observe(&discovery, |_, _, cx| cx.notify()).detach();
        cx.observe(&connection, |this: &mut Self, connection, cx| {
            this.subscribe_if_connected(&connection, cx);
            cx.notify();
        })
        .detach();
        cx.observe(&namespaces, |_, _, cx| cx.notify()).detach();
        cx.on_release(|this: &mut Self, cx| {
            if this.subscribed {
                ClusterRegistry::unsubscribe_events(cx, &this.scope.context_name);
            }
        })
        .detach();
        let mut this = Self::unsubscribed(scope, connection.clone(), namespaces, discovery, cx);
        this.subscribe_if_connected(&connection, cx);
        this
    }

    fn unsubscribed(
        scope: PanelScope,
        connection: Entity<ClusterConnection>,
        namespaces: Entity<NamespaceList>,
        discovery: Entity<DiscoveredKinds>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            scope,
            namespace_picker: Default::default(),
            connection,
            events: cx.new(|_| EventsTable::default()),
            namespaces,
            subscribed: false,
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
            table: None,
            // Resolved when the table is built: the remembered sort, else
            // [`DEFAULT_SORT`] (`remembered-list-sort`).
            initial_sort: None,
            filters: EventFilters::default(),
            search: ListSearch::new(),
            discovery,
        }
    }

    /// A connected panel over a table the test fills itself: no watch is
    /// started, and `subscribed` stands in for one.
    #[cfg(test)]
    pub(crate) fn with_table(
        scope: PanelScope,
        events: Entity<EventsTable>,
        client: kube::Client,
        kinds: Vec<crate::k8s::cluster::discovery::DiscoveredKind>,
        cx: &mut Context<Self>,
    ) -> Self {
        let connection =
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
        cx.observe(&events, |_, _, cx| cx.notify()).detach();
        let namespaces = cx.new(|_| NamespaceList::empty());
        let discovery = cx.new(|_| DiscoveredKinds::loaded(kinds));
        let mut this = Self::unsubscribed(scope, connection, namespaces, discovery, cx);
        this.events = events;
        this.subscribed = true;
        this
    }

    fn subscribe_if_connected(
        &mut self,
        connection: &Entity<ClusterConnection>,
        cx: &mut Context<Self>,
    ) {
        if self.subscribed {
            return;
        }
        let ConnectionState::Connected(client) = &connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        self.events = ClusterRegistry::subscribe_events(cx, &self.scope.context_name, client);
        cx.observe(&self.events, |_, _, cx| cx.notify()).detach();
        self.subscribed = true;
    }

    /// The context's events in this panel's namespaces - what the filters'
    /// options are built from.
    pub(super) fn scoped_rows(&self, cx: &App) -> Vec<EventRow> {
        let namespaces = &self.scope.namespaces;
        self.events
            .read(cx)
            .rows()
            .iter()
            .filter(|row| {
                namespaces.is_empty()
                    || row
                        .namespace
                        .as_ref()
                        .is_some_and(|namespace| namespaces.contains(namespace))
            })
            .cloned()
            .collect()
    }

    /// The events to show now: [`Self::scoped_rows`] through the filters, then
    /// the search - a case-insensitive substring of its reason, involved
    /// object or message (design D3's interim search), not every column the
    /// table renders: this browser's own idea of "visible" for
    /// `crate::ui::list_search`'s shared matcher.
    pub(super) fn visible_rows(&self, cx: &App) -> Vec<EventRow> {
        let query = self.search.query(cx);
        self.scoped_rows(cx)
            .into_iter()
            .filter(|row| self.filters.matches(row))
            .filter(|row| {
                list_search::matches([&row.reason, &row.object_label(), &row.message], &query)
            })
            .collect()
    }

    /// The search box, created the first time a window renders this panel.
    pub(super) fn search_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        self.search.input(SEARCH_PLACEHOLDER, window, cx)
    }

    /// The table, created the first time a window renders this panel and given
    /// `rows` on every render after.
    pub(super) fn sync_table(
        &mut self,
        rows: Vec<EventRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TableState<EventsTableDelegate>> {
        if self.table.is_none() {
            let own = self.initial_sort.and_then(|(column, sort)| match sort {
                ColumnSort::Ascending => Some((column.id().into(), false)),
                ColumnSort::Descending => Some((column.id().into(), true)),
                ColumnSort::Default => None,
            });
            let ids: Vec<SharedString> = EventColumn::DEFAULT_ORDER
                .iter()
                .map(|column| SharedString::from(column.id()))
                .collect();
            let key = crate::ui::list_sort::EVENTS_KEY;
            let (column, descending) = crate::ui::list_sort::starting(
                own,
                crate::util::shell::SortDefaults::get(cx, key),
                &ids,
                super::table::default_sort(),
            );
            let sort = EventColumn::from_id(&column)
                .map(|column| {
                    let direction = if descending {
                        ColumnSort::Descending
                    } else {
                        ColumnSort::Ascending
                    };
                    (column, direction)
                })
                .or(Some(DEFAULT_SORT));
            let table = cx.new(|cx| {
                let mut delegate = EventsTableDelegate::new(sort);
                delegate.remember_as(key);
                TableState::new(delegate, window, cx)
                    .row_selectable(true)
                    .col_selectable(false)
                    .sortable(true)
                    .col_movable(true)
                    .col_resizable(true)
            });
            cx.subscribe_in(&table, window, |_this, table, event, _window, cx| {
                match event {
                    TableEvent::SelectRow(row_ix) => table.update(cx, |table, _| {
                        table.delegate_mut().remember_selection(*row_ix)
                    }),
                    TableEvent::ColumnWidthsChanged(widths) => {
                        table.update(cx, |table, _| table.delegate_mut().set_widths(widths))
                    }
                    _ => {}
                }
                cx.notify();
            })
            .detach();
            self.table = Some(table);
        }
        let table = self.table.clone().expect("built above");
        table.update(cx, |table, cx| {
            table.delegate_mut().set_rows(rows, jiff::Timestamp::now());
            reselect(table, cx);
            cx.notify();
        });
        table
    }

    /// The selected event, if any is selected and still listed.
    pub(super) fn selected_event(&self, cx: &App) -> Option<EventRow> {
        self.table.as_ref()?.read(cx).delegate().selected().cloned()
    }

    /// The sort to save: the table's, or the one it would open with.
    pub(super) fn sort(&self, cx: &App) -> Option<(EventColumn, ColumnSort)> {
        match &self.table {
            Some(table) => table.read(cx).delegate().sort(),
            None => self.initial_sort,
        }
    }
}

impl Focusable for EventsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for EventsPanel {}
impl EventEmitter<ScopeEvent> for EventsPanel {}

impl BasePanel for EventsPanel {
    fn panel_name(&self) -> &'static str {
        "Events"
    }

    fn dump(&self, cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(super::restore::dump(self, cx)),
        }
    }
}

impl Panel for EventsPanel {
    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(
            &self.scope,
            panel_title::title(&self.scope),
            &self.focus_handle,
            panel_title::close_button(cx.entity()),
            window,
            cx,
        )
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}
