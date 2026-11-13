//! The Helm Releases dock panel: its state, its release watch subscriptions -
//! one per namespace in its scope, or one over every namespace - and its dock
//! integration. What it draws is `render`'s.

use super::columns::HelmColumn;
use super::row::HelmRow;
use super::table::{DEFAULT_SORT, HelmTableDelegate, reselect};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::helm::{HelmReleasesTable, HelmScope};
use crate::k8s::resource::load_phase::LoadPhase;
use crate::ui::list_search::{self, ListSearch};
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState,
};
use gpui_kit::component::input::InputState;
use gpui_kit::component::table::{ColumnSort, TableEvent, TableState};
use gpui_kit::*;

/// The search box's placeholder - what it matches on.
pub(super) const SEARCH_PLACEHOLDER: &str = "Search name, namespace, chart...";

/// One watch scope this panel subscribes to: its shared table, and this
/// panel's observation of it - dropped with the subscription, since another
/// panel can keep the table alive, and a kept observer would pile up with each
/// re-scope.
pub(super) struct Watched {
    pub(super) scope: HelmScope,
    pub(super) table: Entity<HelmReleasesTable>,
    _observer: Subscription,
}

/// A dock panel listing one context's Helm releases, live, in its namespace
/// scope.
pub struct HelmReleasesPanel {
    pub(super) scope: PanelScope,
    pub(super) connection: Entity<ClusterConnection>,
    /// The release tables this panel subscribes to, one per watch scope.
    pub(super) tables: Vec<Watched>,
    pub(super) namespaces: Entity<NamespaceList>,
    pub(super) focus_handle: FocusHandle,
    pub(super) namespace_picker: crate::ui::namespace_picker::NamespacePickerSlot,
    /// Built on first render, the first time there's a `Window`.
    pub(super) table: Option<Entity<TableState<HelmTableDelegate>>>,
    /// The sort to start the table with: the saved one, or the remembered or
    /// default one.
    pub(super) initial_sort: Option<(HelmColumn, ColumnSort)>,
    pub(super) indicators: crate::ui::list_state::Indicators,
    pub(super) search: ListSearch,
    /// Whether releases Helm uninstalled but kept the history of are listed
    /// (`helm list --uninstalled`); saved with the panel.
    pub(super) show_uninstalled: bool,
}

impl HelmReleasesPanel {
    pub fn new(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        let namespaces =
            crate::k8s::cluster::namespaces::NamespaceRegistry::list(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| {
            this.subscribe(cx);
            cx.notify();
        })
        .detach();
        cx.observe(&namespaces, |_, _, cx| cx.notify()).detach();
        cx.on_release(|this: &mut Self, cx| this.unsubscribe(cx))
            .detach();
        let mut this = Self {
            scope,
            connection,
            tables: Vec::new(),
            namespaces,
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
            namespace_picker: Default::default(),
            table: None,
            initial_sort: None,
            indicators: Default::default(),
            search: ListSearch::new(),
            show_uninstalled: false,
        };
        this.subscribe(cx);
        this
    }

    /// The watch scopes the panel's namespaces need: every namespace when it
    /// has none, else one per namespace, so a user allowed only some
    /// namespaces' Secrets still sees their releases.
    pub(super) fn watch_scopes(&self) -> Vec<HelmScope> {
        if self.scope.namespaces.is_empty() {
            vec![HelmScope::All]
        } else {
            self.scope
                .namespaces
                .iter()
                .map(|namespace| HelmScope::Namespace(namespace.clone()))
                .collect()
        }
    }

    /// Subscribes to the watches [`Self::watch_scopes`] names, once connected.
    fn subscribe(&mut self, cx: &mut Context<Self>) {
        if !self.tables.is_empty() {
            return;
        }
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        for scope in self.watch_scopes() {
            let table = ClusterRegistry::subscribe_helm_releases(
                cx,
                &self.scope.context_name,
                client.clone(),
                scope.clone(),
            );
            let observer = cx.observe(&table, |_, _, cx| cx.notify());
            self.tables.push(Watched {
                scope,
                table,
                _observer: observer,
            });
        }
    }

    fn unsubscribe(&mut self, cx: &mut App) {
        for watched in self.tables.drain(..) {
            ClusterRegistry::unsubscribe_helm_releases(
                cx,
                &self.scope.context_name,
                &watched.scope,
            );
        }
    }

    /// Scopes the panel to `namespaces` (empty for all), as its own picker
    /// does, and moves its subscriptions to match.
    pub(crate) fn set_namespaces(&mut self, namespaces: Vec<String>, cx: &mut Context<Self>) {
        self.scope = self.scope.scoped_to(namespaces.clone());
        self.unsubscribe(cx);
        self.subscribe(cx);
        cx.emit(ScopeEvent::NamespacesChanged(namespaces));
        cx.notify();
    }

    /// Every listed release in the panel's scope, uninstalled ones only when
    /// asked for.
    pub(super) fn scoped_rows(&self, cx: &App) -> Vec<HelmRow> {
        self.tables
            .iter()
            .flat_map(|watched| watched.table.read(cx).rows())
            .map(|release| HelmRow::new(&release))
            .filter(|row| self.show_uninstalled || !row.uninstalled())
            .collect()
    }

    /// [`Self::scoped_rows`] through the search: a case-insensitive substring
    /// of the name, namespace, chart or status.
    pub(super) fn visible_rows(&self, cx: &App) -> Vec<HelmRow> {
        let query = self.search.query(cx);
        self.scoped_rows(cx)
            .into_iter()
            .filter(|row| {
                list_search::matches(
                    [&row.name, &row.namespace, &row.chart_text(), &row.status],
                    &query,
                )
            })
            .collect()
    }

    /// The panel's load phase across its watches: loading until every one has
    /// listed, refreshing while any relists.
    pub(super) fn phase(&self, cx: &App) -> LoadPhase {
        let phases: Vec<LoadPhase> = self
            .tables
            .iter()
            .map(|watched| watched.table.read(cx).phase())
            .collect();
        let received = |phase: &LoadPhase| match phase {
            LoadPhase::FirstLoad { received } | LoadPhase::Refreshing { received } => *received,
            LoadPhase::Loaded => 0,
        };
        if phases.is_empty() || phases.iter().any(|phase| !phase.has_loaded()) {
            LoadPhase::FirstLoad {
                received: phases.iter().map(received).sum(),
            }
        } else if phases
            .iter()
            .any(|phase| matches!(phase, LoadPhase::Refreshing { .. }))
        {
            LoadPhase::Refreshing {
                received: phases.iter().map(received).sum(),
            }
        } else {
            LoadPhase::Loaded
        }
    }

    /// The first refusal any of the panel's watches got, with its scope.
    pub(super) fn refusal(&self, cx: &App) -> Option<(HelmScope, String)> {
        self.tables.iter().find_map(|watched| {
            watched
                .table
                .read(cx)
                .refused()
                .map(|message| (watched.scope.clone(), message.to_string()))
        })
    }

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
        rows: Vec<HelmRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TableState<HelmTableDelegate>> {
        if self.table.is_none() {
            let own = self.initial_sort.and_then(|(column, sort)| match sort {
                ColumnSort::Ascending => Some((column.id().into(), false)),
                ColumnSort::Descending => Some((column.id().into(), true)),
                ColumnSort::Default => None,
            });
            let ids: Vec<SharedString> = HelmColumn::DEFAULT_ORDER
                .iter()
                .map(|column| SharedString::from(column.id()))
                .collect();
            let key = crate::ui::list_sort::HELM_RELEASES_KEY;
            let (column, descending) = crate::ui::list_sort::starting(
                own,
                crate::util::shell::SortDefaults::get(cx, key),
                &ids,
                super::table::default_sort(),
            );
            let sort = HelmColumn::from_id(&column)
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
                let mut delegate = HelmTableDelegate::new(sort);
                delegate.remember_as(key);
                TableState::new(delegate, window, cx)
                    .row_selectable(true)
                    .col_selectable(false)
                    .sortable(true)
                    .col_movable(true)
                    .col_resizable(true)
            });
            cx.subscribe_in(&table, window, |_this, table, event, window, cx| {
                match event {
                    TableEvent::SelectRow(row_ix) => table.update(cx, |table, _| {
                        table.delegate_mut().remember_selection(*row_ix)
                    }),
                    TableEvent::DoubleClickedRow(row_ix) => {
                        table.update(cx, |table, _| {
                            table.delegate_mut().remember_selection(*row_ix)
                        });
                        _this.open_selected(window, cx);
                    }
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

    /// Asks the window to open the selected release's detail panel.
    pub(super) fn open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self
            .table
            .as_ref()
            .and_then(|table| table.read(cx).delegate().selected().cloned())
        else {
            return;
        };
        window.dispatch_action(
            Box::new(super::commands::OpenHelmRelease {
                context_name: self.scope.context_name.clone(),
                key: crate::k8s::resource::helm::ReleaseKey {
                    namespace: row.namespace,
                    name: row.name,
                },
            }),
            cx,
        );
    }

    /// The sort to save: the table's, or the one it would open with.
    pub(super) fn sort(&self, cx: &App) -> Option<(HelmColumn, ColumnSort)> {
        match &self.table {
            Some(table) => table.read(cx).delegate().sort(),
            None => self.initial_sort,
        }
    }

    /// The table's rows, in displayed order. Test-only.
    #[cfg(test)]
    pub(crate) fn displayed(&self, cx: &App) -> Vec<HelmRow> {
        self.table
            .as_ref()
            .map(|table| table.read(cx).delegate().rows().to_vec())
            .unwrap_or_default()
    }
}

impl Focusable for HelmReleasesPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for HelmReleasesPanel {}
impl EventEmitter<ScopeEvent> for HelmReleasesPanel {}

impl BasePanel for HelmReleasesPanel {
    fn panel_name(&self) -> &'static str {
        "HelmReleases"
    }

    fn dump(&self, cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(super::restore::dump(self, cx)),
        }
    }
}

impl Panel for HelmReleasesPanel {
    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(
            &self.scope,
            panel_title::title(&self.scope),
            &self.focus_handle,
            panel_title::close_button(cx.entity()),
            cx.entity_id(),
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

crate::scoped_panel!(HelmReleasesPanel);
