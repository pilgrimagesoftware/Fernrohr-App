//! Drawing a list panel: the header, the filter, the table and its hint bar, and
//! the states that stand in for rows - connecting, failed, and a kind the server
//! won't list.

use super::commands::{
    DELETE_KEY, DESCRIBE_KEY, DeleteSelected, DescribeSelected, FILTER_KEY, FocusFilter,
    NAMESPACE_KEY, OPEN_IN_BACKGROUND_KEY, OPEN_KEY, OpenInBackground, OpenSelected,
    PANEL_KEY_CONTEXT, REFRESH_KEY, RefreshList, ShowSelectedYaml, WarpNamespace, YAML_KEY,
};
use super::panel::ObjectListPanel;
use super::store::ListMode;
use super::table::data_table;
use crate::k8s::cluster::connection::ConnectionState;
use crate::ui::panel_title::{self, ScopeEvent};
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl ObjectListPanel {
    /// One hint: `action`'s live key (or `fallback` when the keymap has none) and
    /// what it does.
    fn hint(action: &dyn Action, fallback: &str, label: &'static str, window: &mut Window) -> Div {
        let key = Kbd::binding_for_action(action, Some(PANEL_KEY_CONTEXT), window)
            .unwrap_or_else(|| Kbd::new(Keystroke::parse(fallback).expect("a valid default key")));
        div().flex().gap_1().items_center().child(key).child(label)
    }

    fn content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let space = crate::ui::space::spacing(cx);
        let status = |text: String| div().size_full().p(space.panel_inset).child(text);
        match &self.connection.read(cx).state {
            ConnectionState::Connecting => {
                return status("Connecting...".into()).into_any_element();
            }
            ConnectionState::WaitingForTunnel => {
                return status("Waiting for tunnel...".into()).into_any_element();
            }
            // Selectable, with Report… beside it (#177).
            ConnectionState::Failed(reason) => {
                return panel_title::error_content(
                    // Not the context's name: it's in the panel's title already, and a
                    // report filed from here goes to a public issue (#177).
                    "Couldn't connect to the cluster".to_string(),
                    Some(reason.clone()),
                    cx,
                )
                .into_any_element();
            }
            ConnectionState::Connected(_) => {}
        }
        // Spec: "A kind the user cannot list" - the server's refusal, not an
        // empty table that looks like there's nothing there.
        if let Some(refused) = self.objects.read(cx).refused() {
            return panel_title::error_content(
                format!("Can't list {}", self.scope.target.list_label()),
                Some(refused.to_string()),
                cx,
            )
            .into_any_element();
        }
        // `unwatchable-kinds`: a kind discovery offers no `list` for has no rows
        // to show, so say so rather than drawing an empty table.
        let polled = match self.objects.read(cx).mode() {
            ListMode::Unlistable => {
                return status("This kind can't be listed".into())
                    .debug_selector(|| "object-list-unlistable".into())
                    .into_any_element();
            }
            ListMode::Polled { .. } => true,
            ListMode::Watched => false,
        };
        let rows = self.visible_rows(cx);
        let filter = self.filter_input(window, cx);
        let table = self.sync_table(rows, window, cx);
        let mut hints = div()
            .flex()
            .gap(space.control_gap)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(Self::hint(&FocusFilter, FILTER_KEY, "Filter", window))
            .child(Self::hint(&OpenSelected, OPEN_KEY, "Open", window))
            .child(Self::hint(
                &OpenInBackground,
                OPEN_IN_BACKGROUND_KEY,
                "Background",
                window,
            ))
            // Only a namespaced kind has a picker to open.
            .when(self.scope.is_namespaced(), |hints| {
                hints.child(Self::hint(
                    &crate::ui::namespace_picker::PickNamespaces,
                    crate::ui::namespace_picker::PICK_NAMESPACES_KEY,
                    "Namespaces",
                    window,
                ))
            })
            .child(Self::hint(
                &DescribeSelected,
                DESCRIBE_KEY,
                "Describe",
                window,
            ))
            .child(Self::hint(&ShowSelectedYaml, YAML_KEY, "YAML", window));
        // A Service with a forward running can stop it (`port-forward-indicators` 4.1).
        if !self.selected_forwards(cx).is_empty() {
            hints = hints.child(Self::hint(
                &crate::k8s::resource::pods::StopPortForward,
                crate::k8s::resource::pods::STOP_PORT_FORWARD_KEY,
                "Stop forward",
                window,
            ));
        }
        if self.kind.verbs.patch {
            let key = Kbd::binding_for_action(
                &super::commands::EditSelected,
                Some(super::commands::EDITABLE_KEY_CONTEXT),
                window,
            )
            .unwrap_or_else(|| {
                Kbd::new(Keystroke::parse(super::commands::EDIT_KEY).expect("a valid default key"))
            });
            hints = hints.child(div().flex().gap_1().items_center().child(key).child("Edit"));
        }
        if polled {
            hints = hints.child(Self::hint(&RefreshList, REFRESH_KEY, "Refresh", window));
        }
        if self.deletable() {
            let key = Kbd::binding_for_action(
                &DeleteSelected,
                Some(super::delete::DELETABLE_KEY_CONTEXT),
                window,
            )
            .unwrap_or_else(|| {
                Kbd::new(Keystroke::parse(DELETE_KEY).expect("a valid default key"))
            });
            hints = hints.child(
                div()
                    .flex()
                    .gap_1()
                    .items_center()
                    .child(key)
                    .child("Delete"),
            );
        }
        if self.kind.namespaced {
            hints = hints.child(Self::hint(
                &WarpNamespace,
                NAMESPACE_KEY,
                "Namespace",
                window,
            ));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .p(space.panel_inset)
            .children(Self::notes(&self.kind, polled, cx))
            .child(
                div()
                    .on_action(cx.listener(Self::on_action_clear_filter))
                    .child(Input::new(&filter)),
            )
            .children(self.render_refusal(cx))
            .child({
                let context = self.table_key_context();
                div()
                    .flex_1()
                    .min_h_0()
                    .when(!context.is_empty(), |this| this.key_context(context))
                    .on_action(cx.listener(Self::on_action_delete_selected))
                    .child(data_table(&table, cx))
            })
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(crate::ui::style::surface_raised(cx))
                    .child(hints),
            )
            .into_any_element()
    }
}

impl ObjectListPanel {
    /// The contexts around the table that offer a command only when it
    /// applies: Edit for a kind that can be patched, Delete for one that can
    /// be deleted, port-forward in a Services list.
    fn table_key_context(&self) -> KeyContext {
        let mut context = KeyContext::default();
        if self.kind.verbs.patch {
            context.add(super::commands::EDITABLE_KEY_CONTEXT);
        }
        if self.deletable() {
            context.add(super::delete::DELETABLE_KEY_CONTEXT);
        }
        if self.lists_services() {
            context.add(super::port_forward::SERVICES_KEY_CONTEXT);
        }
        context
    }

    /// The notes above a list's table: that a polled kind's rows are a periodic
    /// list, with a control to re-list now, and ComponentStatus's deprecation.
    fn notes(
        kind: &crate::k8s::cluster::discovery::DiscoveredKind,
        polled: bool,
        cx: &App,
    ) -> Option<AnyElement> {
        use gpui_kit::component::Sizable as _;
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        let deprecated = kind.gvk.group.is_empty() && kind.gvk.kind == "ComponentStatus";
        if !polled && !deprecated {
            return None;
        }
        let space = crate::ui::space::spacing(cx);
        let seconds = crate::consts::LIST_POLL_INTERVAL.as_secs();
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .children(polled.then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap(space.control_gap)
                        .debug_selector(|| "object-list-polled".into())
                        .child(format!("Polled every {seconds}s"))
                        .child(
                            Button::new("object-list-refresh")
                                .label("Refresh")
                                .debug_selector(|| "object-list-refresh".into())
                                .xsmall()
                                .ghost()
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(RefreshList), cx)
                                }),
                        )
                }))
                .children(deprecated.then(|| {
                    div()
                        .debug_selector(|| "object-list-deprecated".into())
                        .child(
                            "ComponentStatus is deprecated (Kubernetes v1.19+) and may be \
                             empty on managed control planes.",
                        )
                }))
                .into_any_element(),
        )
    }
}

impl Render for ObjectListPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        let content = self.content(window, cx);
        let this = cx.weak_entity();
        let namespaces = self.namespaces.read(cx).names().to_vec();
        // `None` for a cluster-scoped kind: no namespace to pick.
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
        let panel = div()
            .size_full()
            .key_context(PANEL_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_focus_filter))
            .on_action(cx.listener(Self::on_action_open_selected))
            .on_action(cx.listener(Self::on_action_open_in_background))
            .capture_action(cx.listener(Self::capture_select_down))
            .capture_action(cx.listener(Self::capture_select_up))
            .on_action(cx.listener(Self::on_action_describe_selected))
            .on_action(cx.listener(Self::on_action_show_selected_yaml))
            .on_action(cx.listener(Self::on_action_edit_selected))
            .on_action(cx.listener(Self::on_action_warp_namespace))
            .on_action(cx.listener(
                |this, _: &crate::ui::namespace_picker::PickNamespaces, window, cx| {
                    this.namespace_picker.open(window, cx)
                },
            ))
            .on_action(cx.listener(Self::on_action_fit_columns))
            .on_action(cx.listener(Self::on_action_refresh))
            .on_action(cx.listener(Self::on_action_port_forward_service))
            .on_action(cx.listener(Self::on_action_stop_port_forward))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(div().flex_1().min_h_0().child(content)),
            )
            // Tab stays in the panel: see `ui::panel::focus`.
            .focus_trap("object-list-panel-tab-trap", &self.focus_handle);
        // Namespace quick-jump's context, for a namespaced kind's list - its own
        // frame around the panel, so it's on the focus path with the panel or
        // its table focused.
        div()
            .size_full()
            .when(self.kind.namespaced, |this| {
                this.key_context(crate::ui::namespace_jump::KEY_CONTEXT)
            })
            .on_action(cx.listener(Self::on_action_jump_namespace))
            .child(panel)
    }
}
