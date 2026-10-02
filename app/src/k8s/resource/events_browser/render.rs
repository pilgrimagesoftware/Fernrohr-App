//! Drawing the events browser: the header with its namespace scope, the filter
//! bar and search, the table, the selected event's detail strip, the hint row,
//! and the states that stand in for rows.

use super::commands::{
    CLEAR_KEY, ClearFilters, FilterByKind, FilterByReason, FilterByType, FocusSearch, KIND_KEY,
    OPEN_KEY, OpenInvolvedObject, PANEL_KEY_CONTEXT, REASON_KEY, SEARCH_KEY, TYPE_KEY,
};
use super::filters::Facet;
use super::panel::EventsPanel;
use super::row::EventRow;
use super::table::data_table;
use crate::k8s::cluster::connection::ConnectionState;
use crate::ui::panel_title::{self, ScopeEvent};
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

impl EventsPanel {
    /// One hint: `action`'s live key (or `fallback`) and what it does.
    fn hint(action: &dyn Action, fallback: &str, label: &'static str, window: &mut Window) -> Div {
        let key = Kbd::binding_for_action(action, Some(PANEL_KEY_CONTEXT), window)
            .unwrap_or_else(|| Kbd::new(Keystroke::parse(fallback).expect("a valid default key")));
        div().flex().gap_1().items_center().child(key).child(label)
    }

    /// The filter buttons, each opening its facet's picker, then a chip per
    /// active value - its close removes it - and Clear when any is active.
    fn filter_bar(&self, cx: &mut Context<Self>) -> Div {
        let space = crate::ui::space::spacing(cx);
        let mut bar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(space.control_gap);
        for facet in Facet::ALL {
            let count = self.filters.values(facet).len();
            let label = match count {
                0 => format!("{} ▾", facet.title()),
                n => format!("{} ({n}) ▾", facet.title()),
            };
            let selector = facet.button_selector();
            // Straight to the panel, not a dispatched action: a click needn't
            // leave focus on the panel for its handler to be on the path.
            let this = cx.weak_entity();
            bar = bar.child(
                Button::new(SharedString::from(selector.clone()))
                    .label(label)
                    .xsmall()
                    .outline()
                    .debug_selector(move || selector.clone())
                    .on_click(move |_, window, cx| {
                        let _ = this.update(cx, |this, cx| this.open_filter(facet, window, cx));
                    }),
            );
        }
        for facet in Facet::ALL {
            for value in self.filters.values(facet).clone() {
                let id = format!("events-chip-{}-{value}", facet.title().to_lowercase());
                let this = cx.weak_entity();
                let chip_value = value.clone();
                bar = bar.child(
                    Button::new(SharedString::from(id.clone()))
                        .label(format!("{}: {value} ×", facet.title()))
                        .xsmall()
                        .ghost()
                        .debug_selector(move || id.clone())
                        .on_click(move |_, _window, cx| {
                            let _ = this
                                .update(cx, |this, cx| this.remove_filter(facet, &chip_value, cx));
                        }),
                );
            }
        }
        if !self.filters.is_empty() {
            let this = cx.weak_entity();
            bar = bar.child(
                Button::new("events-clear-filters")
                    .label("Clear filters")
                    .xsmall()
                    .ghost()
                    .debug_selector(|| "events-clear-filters".into())
                    .on_click(move |_, _window, cx| {
                        let _ = this.update(cx, |this, cx| this.clear_filters(cx));
                    }),
            );
        }
        bar
    }

    /// The selected event in full: its involved object as a link, and its
    /// whole message, selectable and copyable (`events-browser`: "Reading a
    /// long message").
    fn detail_strip(&self, row: &EventRow, cx: &App) -> AnyElement {
        let space = crate::ui::space::spacing(cx);
        let target = Self::involved_ref(row);
        let kinds = self.discovery.read(cx).kinds();
        let link = crate::ui::link::references(
            "events-involved",
            std::slice::from_ref(&target),
            |target| target.qualified_name(),
            &self.scope.context_name,
            kinds,
            cx,
        );
        let heading = format!(
            "{} · {}",
            row.type_.as_deref().unwrap_or("Event"),
            row.reason
        );
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(crate::ui::style::surface_raised(cx))
            .debug_selector(|| "events-detail-strip".into())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space.control_gap)
                    .text_sm()
                    .child(heading)
                    .child(link),
            )
            .child(
                div()
                    .debug_selector(|| "events-detail-message".into())
                    .child(
                        gpui_kit::component::text::markdown(panel_title::escape_markdown(
                            &row.message,
                        ))
                        .selectable(true),
                    ),
            )
            .into_any_element()
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
        if let Some(refused) = self.events.read(cx).refused() {
            return panel_title::error_content(
                "Can't list events".to_string(),
                Some(refused.to_string()),
                cx,
            )
            .into_any_element();
        }
        let search = self.search_input(window, cx);
        let rows = self.visible_rows(cx);
        let total = self.scoped_rows(cx).len();
        let count = match (
            self.search_query(cx).is_empty() && self.filters.is_empty(),
            rows.len(),
        ) {
            (true, shown) => format!("{shown} events"),
            (false, shown) => format!("{shown} of {total} events"),
        };
        let table = self.sync_table(rows, window, cx);
        let strip = self
            .selected_event(cx)
            .map(|row| self.detail_strip(&row, cx));
        let hints = div()
            .flex()
            .flex_wrap()
            .gap(space.control_gap)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(Self::hint(
                &OpenInvolvedObject,
                OPEN_KEY,
                "Open object",
                window,
            ))
            .child(Self::hint(&FocusSearch, SEARCH_KEY, "Search", window))
            .child(Self::hint(&FilterByType, TYPE_KEY, "Type", window))
            .child(Self::hint(&FilterByKind, KIND_KEY, "Kind", window))
            .child(Self::hint(&FilterByReason, REASON_KEY, "Reason", window))
            .child(Self::hint(
                &ClearFilters,
                CLEAR_KEY,
                "Clear filters",
                window,
            ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .p(space.panel_inset)
            .child(self.filter_bar(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space.control_gap)
                    .child(
                        div()
                            .flex_1()
                            .on_action(cx.listener(Self::on_action_clear_search))
                            .child(Input::new(&search)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .debug_selector(|| "events-count".into())
                            .child(count),
                    ),
            )
            .child(div().flex_1().min_h_0().child(data_table(&table, cx)))
            .children(strip)
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

impl Render for EventsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        let content = self.content(window, cx);
        let this = cx.weak_entity();
        let namespaces = self.namespaces.read(cx).names();
        let namespace_bar =
            panel_title::namespace_picker(&self.scope, namespaces, move |namespaces, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.scope = this.scope.scoped_to(namespaces.clone());
                    cx.emit(ScopeEvent::NamespacesChanged(namespaces));
                    cx.notify();
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
            .on_action(cx.listener(Self::on_action_open_involved))
            .on_action(cx.listener(Self::on_action_focus_search))
            .on_action(cx.listener(Self::on_action_filter_type))
            .on_action(cx.listener(Self::on_action_filter_kind))
            .on_action(cx.listener(Self::on_action_filter_reason))
            .on_action(cx.listener(Self::on_action_clear_filters))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(div().flex_1().min_h_0().child(content)),
            )
            .focus_trap("events-panel-tab-trap", &self.focus_handle)
    }
}
