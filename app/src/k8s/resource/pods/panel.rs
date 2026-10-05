//! The Pods dock panel: its state, lifecycle, selection wiring and dock integration. What it draws is `render`'s.

use super::*;

pub fn register_restore(cx: &mut App) {
    register_panel(cx, "Pods", |context, _window, cx| {
        crate::ui::unrestored::restore_with(&context, cx, |state, cx| {
            let context_name =
                crate::ui::unrestored::required_str(state, "context_name")?.to_string();
            let namespaces =
                serde_json::from_value(state["namespaces"].clone()).unwrap_or_default();
            let scope = PanelScope::new(NavTarget::pods(), context_name).scoped_to(namespaces);
            Ok(panel_handle(cx.new(|cx| PodsPanel::new(scope, cx))))
        })
    });
}

/// A dock panel connecting to its scope's cluster and rendering its live,
/// all-namespaces Pods table.
pub struct PodsPanel {
    pub(super) scope: PanelScope,
    pub(super) connection: Entity<crate::k8s::cluster::connection::ClusterConnection>,
    pub(super) table: Entity<PodsTable>,
    pub(super) namespaces: Entity<crate::k8s::cluster::namespaces::NamespaceList>,
    pub(super) subscribed: bool,
    pub(super) focus_handle: FocusHandle,
    /// The title bar's namespace picker, made on first render.
    pub(super) namespace_picker: crate::ui::namespace_picker::NamespacePickerSlot,
    pub(super) pod_table: Option<Entity<TableState<PodTableDelegate>>>,
    /// The open quick look over the selected pod, if any (`pod-quick-look`).
    pub(super) quick_look: Option<Entity<super::quick_look::QuickLookPopover>>,
    /// The last row action the cluster refused, shown above the table until
    /// dismissed or the next action.
    pub(super) action_failure: Option<super::actions::PodActionFailure>,
}

impl PodsPanel {
    pub fn new(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let context_name = scope.context_name.clone();
        let connection = ClusterRegistry::connection(cx, &context_name);
        let namespaces =
            crate::k8s::cluster::namespaces::NamespaceRegistry::list(cx, &context_name);
        Self::with_connection(scope, connection, namespaces, cx)
    }

    /// Construction from an explicit connection and namespace list, so tests
    /// can hand in stubs. [`Self::new`] has to source both from their
    /// registries, which starts a *real* connect - a tokio task gpui's test
    /// scheduler reports as cross-thread nondeterminism if it is still in
    /// flight when the test ends. The handler tests never need that, so they
    /// pass a connection that stays in a non-`Connected` state.
    pub(super) fn with_connection(
        scope: PanelScope,
        connection: Entity<crate::k8s::cluster::connection::ClusterConnection>,
        namespaces: Entity<crate::k8s::cluster::namespaces::NamespaceList>,
        cx: &mut Context<Self>,
    ) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let context_name = scope.context_name.clone();
        cx.observe(&connection, |this: &mut Self, connection, cx| {
            this.start_watch_if_connected(&connection, cx);
            cx.notify();
        })
        .detach();
        cx.observe(&namespaces, |_, _, cx| cx.notify()).detach();
        cx.on_release({
            let context_name = context_name.clone();
            move |this: &mut Self, cx| {
                if this.subscribed {
                    ClusterRegistry::unsubscribe_pods(cx, &context_name);
                }
            }
        })
        .detach();

        let mut this = Self {
            scope,
            namespace_picker: Default::default(),
            connection: connection.clone(),
            table: cx.new(|_| PodsTable::default()),
            namespaces,
            subscribed: false,
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
            pod_table: None,
            quick_look: None,
            action_failure: None,
        };
        this.start_watch_if_connected(&connection, cx);
        this
    }

    /// A panel over stub cluster state: the connection never leaves
    /// `Connecting`, so no watch, discovery, or namespace list is started.
    /// For tests that exercise the panel's own behaviour rather than a live
    /// cluster.
    #[cfg(test)]
    pub(super) fn with_stubs(scope: PanelScope, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
        use crate::k8s::cluster::namespaces::NamespaceList;

        let connection =
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting));
        let namespaces = cx.new(|_| NamespaceList::empty());
        Self::with_connection(scope, connection, namespaces, cx)
    }

    pub(super) fn start_watch_if_connected(
        &mut self,
        connection: &Entity<crate::k8s::cluster::connection::ClusterConnection>,
        cx: &mut Context<Self>,
    ) {
        use crate::k8s::cluster::session::ClusterRegistry;

        if self.subscribed {
            return;
        }
        let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
            &connection.read(cx).state
        else {
            return;
        };
        let client = client.clone();
        self.table = ClusterRegistry::subscribe_pods(cx, &self.scope.context_name, client);
        cx.observe(&self.table, |_, _, cx| cx.notify()).detach();
        self.subscribed = true;
    }

    /// The pod the table's last row-selection chose, from the app-scoped
    /// global `ShowLogs` also reads. The panel keeps no copy of its own: the
    /// selection is a window-wide fact, and the rows it indexes come and go
    /// with the watch.
    pub(super) fn selected(cx: &App) -> Option<PodSelection> {
        cx.try_global::<SelectedPod>()
            .and_then(|selected| selected.0.clone())
    }

    pub(super) fn on_action_warp_namespace(
        &mut self,
        _: &WarpNamespace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(namespace) = Self::selected(cx).map(|selection| selection.namespace) else {
            return;
        };
        self.set_namespaces(vec![namespace], cx);
    }

    /// `WarpAllToNamespace` (`shift-w`): asks the window to move every namespaced
    /// list in this panel's context to the selected pod's namespace, and to make it
    /// the context's default. `w` stays this panel alone.
    pub(super) fn on_action_warp_all_to_namespace(
        &mut self,
        _: &WarpAllToNamespace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(namespace) = Self::selected(cx).map(|selection| selection.namespace) else {
            return;
        };
        window.dispatch_action(
            Box::new(crate::util::shell::WarpContextToNamespace {
                context_name: self.scope.context_name.clone(),
                namespace,
            }),
            cx,
        );
    }

    /// Scopes this panel to `namespaces` (empty for all), as its own picker does -
    /// what Warp All to Namespace applies to every namespaced panel in a context.
    pub(crate) fn set_namespaces(&mut self, namespaces: Vec<String>, cx: &mut Context<Self>) {
        self.scope = self.scope.scoped_to(namespaces.clone());
        cx.emit(crate::ui::panel_title::ScopeEvent::NamespacesChanged(
            namespaces,
        ));
        cx.notify();
    }

    /// `DescribePod` (`d`) and the row context menu's "Open" both ask for the
    /// pod's detail panel on the structured field view.
    pub(super) fn on_action_describe_pod(
        &mut self,
        _: &DescribePod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Self::selected(cx).is_some() {
            window.dispatch_action(Box::new(crate::ui::nav::ShowPodDetail), cx);
        }
    }

    pub(super) fn on_action_show_pod_logs(
        &mut self,
        _: &ShowPodLogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Self::selected(cx).is_some() {
            window.dispatch_action(Box::new(crate::ui::nav::ShowLogs), cx);
        }
    }

    /// `ShowPodYaml` (`y`) asks for the same panel as `d`, on the other view.
    /// It is a distinct app-level action rather than the same one, because `y`
    /// means "the YAML, now": routing it through `ShowPodDetail` would open
    /// the field list and leave the shortcut's second half to the user.
    pub(super) fn on_action_show_pod_yaml(
        &mut self,
        _: &ShowPodYaml,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Self::selected(cx).is_some() {
            window.dispatch_action(Box::new(crate::ui::nav::ShowPodDetailYaml), cx);
        }
    }

    pub(super) fn sync_table(
        &mut self,
        rows: Vec<PodTableRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TableState<PodTableDelegate>> {
        if self.pod_table.is_none() {
            let table = cx.new(|cx| {
                TableState::new(PodTableDelegate::default(), window, cx)
                    .row_selectable(true)
                    .col_selectable(false)
                    .sortable(true)
                    .col_movable(true)
                    .col_resizable(true)
            });
            // The row context menu dispatches its commands from the table, so
            // they reach this panel's handlers as their keys do.
            table.update(cx, |table, cx| {
                let focus = table.focus_handle(cx);
                table.delegate_mut().set_action_context(focus);
            });
            cx.subscribe_in(&table, window, |this, table, event, window, cx| {
                if let TableEvent::ColumnWidthsChanged(widths) = event {
                    table.update(cx, |table, _| table.delegate_mut().set_widths(widths));
                    return;
                }
                let row_ix = match event {
                    TableEvent::SelectRow(row_ix) => *row_ix,
                    // A single click only selects (drives WarpNamespace/
                    // ShowPodLogs, which read SelectedPod); opening the
                    // detail panel is the double-click, matching the
                    // resource panel's own row convention (section 9.1).
                    TableEvent::DoubleClickedRow(row_ix) => {
                        let Some(selection) = table
                            .read(cx)
                            .delegate()
                            .rows()
                            .get(*row_ix)
                            .map(|row| row.selection.clone())
                        else {
                            return;
                        };
                        remember_selection(table, &selection, cx);
                        cx.set_global(SelectedPod(Some(selection)));
                        window.dispatch_action(Box::new(crate::ui::nav::ShowPodDetail), cx);
                        return;
                    }
                    _ => return,
                };
                let Some(selection) = table
                    .read(cx)
                    .delegate()
                    .rows()
                    .get(row_ix)
                    .map(|row| row.selection.clone())
                else {
                    return;
                };
                remember_selection(table, &selection, cx);
                this.retarget_quick_look(&selection, cx);
                cx.set_global(SelectedPod(Some(selection)));
                cx.notify();
            })
            .detach();
            self.pod_table = Some(table);
        }
        let table = self.pod_table.as_ref().unwrap().clone();
        table.update(cx, |table, cx| {
            table.delegate_mut().set_rows(rows);
            // A row update can move or drop the pod at the previously
            // selected index (see `pods_table::reselect`'s doc comment) -
            // called directly, not deferred, since `table` here is already
            // the full `&mut TableState`, unlike inside `perform_sort`.
            pods_table::reselect(table, cx);
            cx.notify();
        });
        table
    }
}

impl Focusable for PodsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PodsPanel {}
impl EventEmitter<ScopeEvent> for PodsPanel {}

impl BasePanel for PodsPanel {
    fn panel_name(&self) -> &'static str {
        "Pods"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": self.scope.context_name,
                "namespaces": self.scope.namespaces,
            })),
        }
    }
}

/// Section 10: the title bar, supplied to the dock rather than drawn here, so
/// this panel and a placeholder get an identical bar.
impl Panel for PodsPanel {
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

#[cfg(test)]
mod list_keys_tests;
#[cfg(test)]
mod tests;
