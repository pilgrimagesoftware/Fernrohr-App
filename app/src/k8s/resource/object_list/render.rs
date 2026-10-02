//! Drawing a list panel: the header, the filter, the table and its hint bar, and
//! the states that stand in for rows - connecting, failed, and a kind the server
//! won't list.

use super::commands::{
    DESCRIBE_KEY, DescribeSelected, FILTER_KEY, FocusFilter, NAMESPACE_KEY, OPEN_KEY, OpenSelected,
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
            ConnectionState::Failed(reason) => {
                return status(format!("Connection failed: {reason}")).into_any_element();
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
                &DescribeSelected,
                DESCRIBE_KEY,
                "Describe",
                window,
            ))
            .child(Self::hint(&ShowSelectedYaml, YAML_KEY, "YAML", window));
        if polled {
            hints = hints.child(Self::hint(&RefreshList, REFRESH_KEY, "Refresh", window));
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
            .child(div().flex_1().min_h_0().child(data_table(&table, cx)))
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
        let namespaces = self.namespaces.read(cx).names();
        // `None` for a cluster-scoped kind: no namespace to pick.
        let namespace_bar =
            panel_title::namespace_picker(&self.scope, namespaces, move |namespaces, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.scope = this.scope.scoped_to(namespaces.clone());
                    cx.emit(ScopeEvent::NamespacesChanged(namespaces));
                });
            });
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
        div()
            .size_full()
            .key_context(PANEL_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_focus_filter))
            .on_action(cx.listener(Self::on_action_open_selected))
            .capture_action(cx.listener(Self::capture_select_down))
            .capture_action(cx.listener(Self::capture_select_up))
            .on_action(cx.listener(Self::on_action_describe_selected))
            .on_action(cx.listener(Self::on_action_show_selected_yaml))
            .on_action(cx.listener(Self::on_action_warp_namespace))
            .on_action(cx.listener(Self::on_action_fit_columns))
            .on_action(cx.listener(Self::on_action_refresh))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(div().flex_1().min_h_0().child(content)),
            )
            // Tab stays in the panel: see `ui::panel::focus`.
            .focus_trap("object-list-panel-tab-trap", &self.focus_handle)
    }
}
