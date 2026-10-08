//! Drawing the Pods panel: its table, hint bar and states.

use super::*;
use crate::ui::list_keys::{self, Step};
use crate::ui::list_search;
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;

/// Up/Down with no pod selected, or with focus on the panel rather than its table
/// (`standard-resource-panels` 5.2, [`list_keys`]).
impl PodsPanel {
    fn capture_select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::Down, window, cx);
    }

    fn capture_select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::Up, window, cx);
    }

    /// Fits every column to its contents (`ui::table_fit`).
    fn on_action_fit_columns(
        &mut self,
        _: &crate::ui::table_fit::FitAllColumns,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(table) = self.pod_table.clone() {
            let size = crate::ui::table_fit::table_size(cx);
            crate::ui::table_fit::fit_all_columns(&table, size, window, cx);
        }
    }

    fn step(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(table) = self.pod_table.clone()
            && list_keys::step(&table, &self.focus_handle, step, window, cx)
        {
            cx.stop_propagation();
        }
    }
}

impl Render for PodsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::k8s::cluster::connection::ConnectionState;

        let space = crate::ui::space::spacing(cx);
        let content = match &self.connection.read(cx).state {
            ConnectionState::Connecting => div()
                .size_full()
                .p(space.panel_inset)
                .child("Connecting..."),
            ConnectionState::WaitingForTunnel => div()
                .size_full()
                .p(space.panel_inset)
                .child("Waiting for tunnel..."),
            // Selectable, with Report… beside it (#177).
            ConnectionState::Failed(reason) => div().size_full().child(panel_title::error_content(
                // Not the context's name: it's in the panel's title already, and a
                // report filed from here goes to a public issue (#177).
                "Couldn't connect to the cluster".to_string(),
                Some(reason.clone()),
                cx,
            )),
            // `connection-status-bar`: a paused watch used to print its own "Paused
            // (...)" line here. That moved to the window's status bar
            // (`ui/status_bar.rs`), which shows it once per window rather than once per
            // affected panel - this branch no longer reads pause state at all, so a
            // paused panel just keeps rendering its last rows undisturbed.
            ConnectionState::Connected(_) => {
                // Built (and a restored filter's text loaded) before the rows
                // below are read, so the first paint is already narrowed
                // rather than showing every row until some later redraw
                // (`list-search` #189).
                let filter_input = self.filter.input(filter::FILTER_PLACEHOLDER, window, cx);
                let query = self.filter.query(cx);
                let now = Timestamp::now();
                let namespaces = &self.scope.namespaces;
                let forwards = crate::k8s::cluster::port_forwards::PortForwards::entity(cx);
                let forwards = forwards.read(cx);
                let items: Vec<PodTableRow> = self
                    .table
                    .read(cx)
                    .pods()
                    .iter()
                    .filter(|pod| matches_namespaces(pod, namespaces))
                    .map(|pod| {
                        let containers = pod
                            .spec
                            .as_ref()
                            .map(|spec| spec.containers.iter().map(|c| c.name.clone()).collect())
                            .unwrap_or_default();
                        let selection = PodSelection {
                            namespace: pod.metadata.namespace.clone().unwrap_or_default(),
                            name: pod.metadata.name.clone().unwrap_or_default(),
                            containers,
                            context_name: self.scope.context_name.clone(),
                        };
                        let forwards = forwards.for_object(
                            &crate::k8s::cluster::port_forwards::ForwardObject::pod(
                                &selection.context_name,
                                &selection.namespace,
                                &selection.name,
                            ),
                        );
                        let mut row = pod_row(pod, now);
                        row.forwards = forwards.len();
                        PodTableRow {
                            row,
                            selection,
                            forwards,
                        }
                    })
                    // Every visible column, not name alone - the table's own
                    // idea of "visible" (`pods_table::visible_texts`) for the
                    // shared matcher.
                    .filter(|item| {
                        list_search::matches(pods_table::visible_texts(&item.row), &query)
                    })
                    .collect();
                let table = self.sync_table(items, window, cx);
                // The shell command's context, only while the selected pod has a
                // running container (`shell`).
                let shellable = self.shell_candidates(cx).is_some();
                // A quick look whose pod has no row - deleted while open - can't
                // hang below it, so it sits at the top of the table instead.
                let unanchored = self.quick_look.clone().filter(|popover| {
                    let target = &popover.read(cx).target;
                    table.read(cx).delegate().index_of(target).is_none()
                });
                let shortcuts = self.render_hints(shellable, window, cx);
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .p(space.panel_inset)
                    .children(self.render_action_failure(cx))
                    .child(
                        div()
                            .on_action(cx.listener(Self::on_action_clear_filter))
                            .child(Input::new(&filter_input)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .when(shellable, |this| this.key_context(SHELL_KEY_CONTEXT))
                            .children(unanchored.map(|popover| {
                                deferred(
                                    anchored().snap_to_window_with_margin(px(8.)).child(popover),
                                )
                                .with_priority(1)
                            }))
                            .child(pods_table::data_table(&table, cx)),
                    )
                    .child(
                        div()
                            .mt(space.control_gap)
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(crate::ui::style::surface_raised(cx))
                            .child(shortcuts),
                    )
            }
        };

        let this = cx.weak_entity();
        let namespaces = self.namespaces.read(cx).names().to_vec();
        let namespace_bar = self.namespace_picker.element(
            &self.scope,
            &namespaces,
            move |namespaces, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.scope = this.scope.scoped_to(namespaces.clone());
                    cx.emit(ScopeEvent::NamespacesChanged(namespaces));
                });
            },
            window,
            cx,
        );
        // "Context: <name>" on the left, the namespace picker (when there is one)
        // on the right, in one header row.
        let header = div()
            .flex()
            .items_center()
            .gap(space.control_gap)
            .px(space.panel_inset)
            .py(space.control_gap)
            .bg(crate::ui::style::surface_raised(cx))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(panel_title::context_label(
                &self.scope,
                cx.theme().muted_foreground,
            ))
            .children(namespace_bar);

        // While a quick look is open its own keys (Escape, Enter) apply too.
        let mut key_context = KeyContext::default();
        key_context.add(PANEL_KEY_CONTEXT);
        if self.quick_look.is_some() {
            key_context.add(QUICK_LOOK_KEY_CONTEXT);
        }
        let panel = div()
            .size_full()
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .capture_action(cx.listener(Self::capture_cancel))
            .capture_action(cx.listener(Self::capture_select_down))
            .capture_action(cx.listener(Self::capture_select_up))
            .on_action(cx.listener(Self::on_action_focus_filter))
            .on_action(cx.listener(Self::on_action_warp_namespace))
            .on_action(cx.listener(Self::on_action_warp_all_to_namespace))
            .on_action(cx.listener(
                |this, _: &crate::ui::namespace_picker::PickNamespaces, window, cx| {
                    this.namespace_picker.open(window, cx)
                },
            ))
            .on_action(cx.listener(Self::on_action_describe_pod))
            .on_action(cx.listener(Self::on_action_open_in_background))
            .on_action(cx.listener(Self::on_action_show_pod_logs))
            .on_action(cx.listener(Self::on_action_show_pod_logs_flipped))
            .on_action(cx.listener(Self::on_action_show_pod_yaml))
            .on_action(cx.listener(Self::on_action_fit_columns))
            .on_action(cx.listener(Self::on_action_quick_look))
            .on_action(cx.listener(Self::on_action_delete_pod))
            .on_action(cx.listener(Self::on_action_kill_pod))
            .on_action(cx.listener(Self::on_action_shell_pod))
            .on_action(cx.listener(Self::on_action_port_forward_pod))
            .on_action(cx.listener(Self::on_action_stop_port_forward))
            .on_action(cx.listener(Self::on_action_edit_pod))
            .on_action(cx.listener(Self::on_action_close_quick_look))
            .on_action(cx.listener(Self::on_action_open_quick_look_details))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(div().flex_1().min_h_0().child(content)),
            )
            // Tab stays in the panel: see `ui::panel::focus`.
            .focus_trap("pods-panel-tab-trap", &self.focus_handle);
        // Namespace quick-jump's context, as its own frame around the panel so
        // it's on the focus path with the panel or its table focused.
        div()
            .size_full()
            .key_context(crate::ui::namespace_jump::KEY_CONTEXT)
            .on_action(cx.listener(Self::on_action_jump_namespace))
            .child(panel)
    }
}
