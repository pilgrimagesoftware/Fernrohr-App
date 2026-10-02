//! The list dock panel every non-Pod kind opens: its state, its watch
//! subscription, its handlers and its dock integration. What it draws is
//! `render`'s; how its columns behave is `table`'s.

use super::commands::{FocusFilter, OpenListedObject, OpenSelected, WarpNamespace};
use super::store::ObjectsTable;
use super::table::{
    ColumnLayout, ListColumn, ListRow, ObjectTableDelegate, apply_layout, reselect,
};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pod_detail::DetailView;
use crate::ui::nav::ObjectTarget;
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use gpui_kit::component::button::Button;
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState,
};
use gpui_kit::component::input::InputState;
use gpui_kit::component::table::{TableEvent, TableState};
use gpui_kit::*;

mod keys;

/// A dock panel listing one discovered kind's objects in its scope's cluster,
/// live, narrowed to its namespace selection and its filter.
pub struct ObjectListPanel {
    pub(super) kind: DiscoveredKind,
    pub(super) scope: PanelScope,
    pub(super) connection: Entity<ClusterConnection>,
    /// The kind's shared table - this panel's own empty one until it subscribes.
    pub(super) objects: Entity<ObjectsTable>,
    pub(super) namespaces: Entity<NamespaceList>,
    pub(super) subscribed: bool,
    pub(super) focus_handle: FocusHandle,
    /// Built on first render, which is the first time there's a `Window`.
    pub(super) table: Option<Entity<TableState<ObjectTableDelegate>>>,
    pub(super) filter: Option<Entity<InputState>>,
    /// The column layout to start the table with, once it's built.
    pub(super) initial_layout: ColumnLayout,
}

impl ObjectListPanel {
    pub fn new(kind: DiscoveredKind, scope: PanelScope, cx: &mut Context<Self>) -> Self {
        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        let namespaces =
            crate::k8s::cluster::namespaces::NamespaceRegistry::list(cx, &scope.context_name);
        Self::with_connection(kind, scope, connection, namespaces, cx)
    }

    /// Construction from an explicit connection and namespace list, so tests can
    /// hand in stubs rather than start a real connect.
    pub(super) fn with_connection(
        kind: DiscoveredKind,
        scope: PanelScope,
        connection: Entity<ClusterConnection>,
        namespaces: Entity<NamespaceList>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&connection, |this: &mut Self, connection, cx| {
            this.subscribe_if_connected(&connection, cx);
            cx.notify();
        })
        .detach();
        cx.observe(&namespaces, |_, _, cx| cx.notify()).detach();
        cx.on_release(|this: &mut Self, cx| {
            if this.subscribed {
                ClusterRegistry::unsubscribe_kind(cx, &this.scope.context_name, &this.kind);
            }
        })
        .detach();
        let mut this = Self {
            kind,
            scope,
            connection: connection.clone(),
            objects: cx.new(|_| ObjectsTable::default()),
            namespaces,
            subscribed: false,
            focus_handle: cx.focus_handle(),
            table: None,
            filter: None,
            initial_layout: Vec::new(),
        };
        this.subscribe_if_connected(&connection, cx);
        this
    }

    /// A connected panel over a table the test fills itself: `client` only marks
    /// the connection `Connected`, and no watch is started - `subscribed` stands in
    /// for one, so `objects` is what renders.
    #[cfg(test)]
    pub(crate) fn with_table(
        kind: DiscoveredKind,
        scope: PanelScope,
        objects: Entity<ObjectsTable>,
        client: kube::Client,
        cx: &mut Context<Self>,
    ) -> Self {
        let connection =
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
        cx.observe(&objects, |_, _, cx| cx.notify()).detach();
        Self {
            kind,
            scope,
            connection,
            objects,
            namespaces: cx.new(|_| NamespaceList::empty()),
            subscribed: true,
            focus_handle: cx.focus_handle(),
            table: None,
            filter: None,
            initial_layout: Vec::new(),
        }
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
        self.objects =
            ClusterRegistry::subscribe_kind(cx, &self.scope.context_name, client, &self.kind);
        cx.observe(&self.objects, |_, _, cx| cx.notify()).detach();
        self.subscribed = true;
    }

    /// The kind this panel lists.
    #[cfg(test)]
    pub fn kind(&self) -> &DiscoveredKind {
        &self.kind
    }

    /// The rows to show now: the kind's objects in this panel's namespaces whose
    /// name contains the filter text.
    pub(super) fn visible_rows(&self, cx: &App) -> Vec<ListRow> {
        let now = jiff::Timestamp::now();
        let namespaces = &self.scope.namespaces;
        let filter = self
            .filter
            .as_ref()
            .map(|filter| filter.read(cx).value().to_string())
            .unwrap_or_default();
        self.objects
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
            .filter(|row| row.name.contains(filter.as_str()))
            .map(|row| ListRow::new(row.clone(), now))
            .collect()
    }

    /// The text field filtering rows by name, created the first time a window
    /// renders this panel.
    pub(super) fn filter_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(filter) = &self.filter {
            return filter.clone();
        }
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by name..."));
        cx.subscribe(
            &filter,
            |_, _, _event: &gpui_kit::component::input::InputEvent, cx| cx.notify(),
        )
        .detach();
        self.filter = Some(filter.clone());
        filter
    }

    /// The table, created the first time a window renders this panel and given
    /// `rows` on every render after.
    pub(super) fn sync_table(
        &mut self,
        rows: Vec<ListRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TableState<ObjectTableDelegate>> {
        if self.table.is_none() {
            let columns = apply_layout(ListColumn::for_kind(&self.kind), &self.initial_layout);
            let this = cx.weak_entity();
            let table = cx.new(|cx| {
                let mut delegate = ObjectTableDelegate::new(columns);
                delegate.set_on_open(move |row_ix, window, cx| {
                    let _ = this.update(cx, |this, cx| this.open_row(row_ix, window, cx));
                });
                TableState::new(delegate, window, cx)
                    .row_selectable(true)
                    .col_selectable(false)
                    .sortable(true)
                    .col_movable(true)
                    .col_resizable(true)
            });
            cx.subscribe_in(
                &table,
                window,
                |this, table, event, window, cx| match event {
                    TableEvent::SelectRow(row_ix) => {
                        table.update(cx, |table, _| {
                            table.delegate_mut().remember_selection(*row_ix)
                        });
                    }
                    TableEvent::DoubleClickedRow(row_ix) => this.open_row(*row_ix, window, cx),
                    TableEvent::ColumnWidthsChanged(widths) => {
                        table.update(cx, |table, _| table.delegate_mut().set_widths(widths));
                    }
                    _ => {}
                },
            )
            .detach();
            self.table = Some(table);
        }
        let table = self.table.clone().expect("built above");
        table.update(cx, |table, cx| {
            table.delegate_mut().set_rows(rows);
            reselect(table, cx);
            cx.notify();
        });
        table
    }

    /// Opens the object at displayed row `row_ix` (`standard-resource-panels` D5):
    /// its detail panel in this panel's context, or the already-open one, focused.
    pub(super) fn open_row(&mut self, row_ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.open_row_as(row_ix, None, window, cx);
    }

    /// [`Self::open_row`], showing `view` - `y` asks for the YAML.
    pub(super) fn open_row_as(
        &mut self,
        row_ix: usize,
        view: Option<DetailView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = &self.table else {
            return;
        };
        let Some(row) = table.read(cx).delegate().rows().get(row_ix).cloned() else {
            return;
        };
        table.update(cx, |table, _| {
            table.delegate_mut().remember_selection(row_ix)
        });
        let target = ObjectTarget {
            kind: self.kind.clone(),
            namespace: row.object.namespace.clone(),
            name: row.object.name.clone(),
        };
        window.dispatch_action(
            Box::new(OpenListedObject {
                context_name: self.scope.context_name.clone(),
                target,
                view,
            }),
            cx,
        );
    }

    /// The table's selected row, if any.
    pub(super) fn selected_row(&self, cx: &App) -> Option<usize> {
        self.table.as_ref()?.read(cx).selected_row()
    }

    pub(super) fn on_action_open_selected(
        &mut self,
        _: &OpenSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row_ix) = self.selected_row(cx) {
            self.open_row(row_ix, window, cx);
        }
    }

    /// Escape in the filter: clears it and hands focus back to the table, so the
    /// keyboard is never left stranded in the text field.
    pub(super) fn on_action_clear_filter(
        &mut self,
        _: &gpui_kit::component::input::Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(filter) = &self.filter {
            filter.update(cx, |filter, cx| filter.set_value("", window, cx));
        }
        if let Some(table) = &self.table {
            table.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn on_action_focus_filter(
        &mut self,
        _: &FocusFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filter = self.filter_input(window, cx);
        filter.read(cx).focus_handle(cx).focus(window, cx);
    }

    /// Whether the filter box holds focus. Test-only.
    #[cfg(test)]
    pub(crate) fn filter_focused(&self, window: &Window, cx: &App) -> bool {
        self.filter
            .as_ref()
            .is_some_and(|filter| filter.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Narrows a namespaced kind's list to the selected object's namespace - the
    /// Pods panel's `w`.
    pub(super) fn on_action_warp_namespace(
        &mut self,
        _: &WarpNamespace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(namespace) = self
            .selected_row(cx)
            .and_then(|row_ix| {
                self.table
                    .as_ref()?
                    .read(cx)
                    .delegate()
                    .rows()
                    .get(row_ix)
                    .cloned()
            })
            .and_then(|row| row.object.namespace)
        else {
            return;
        };
        self.scope = self.scope.scoped_to(vec![namespace.clone()]);
        cx.emit(ScopeEvent::NamespacesChanged(vec![namespace]));
    }
}

impl Focusable for ObjectListPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for ObjectListPanel {}
impl EventEmitter<ScopeEvent> for ObjectListPanel {}

impl BasePanel for ObjectListPanel {
    fn panel_name(&self) -> &'static str {
        "ObjectList"
    }

    fn dump(&self, cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(super::restore::dump(self, cx)),
        }
    }
}

/// The same title bar every panel gets from the dock: the kind's plural name.
impl Panel for ObjectListPanel {
    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(
            &self.scope,
            panel_title::title(&self.scope),
            &self.focus_handle,
            window,
            cx,
        )
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn toolbar_buttons(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Vec<Button>> {
        panel_title::toolbar_buttons(cx.entity())
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}

#[cfg(test)]
mod tests;
