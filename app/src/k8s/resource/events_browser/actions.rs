//! The events browser's handlers: following the selected event's involved
//! object, the search box, and setting and clearing the filters.

use super::commands::{
    ClearFilters, FilterByKind, FilterByReason, FilterByType, FocusSearch, OpenInvolvedObject,
};
use super::filters::{Facet, FacetPicker, options};
use super::panel::EventsPanel;
use super::row::EventRow;
use crate::k8s::object_ref::ObjectRef;
use crate::ui::link::FollowReference;
use gpui_kit::*;

impl EventsPanel {
    /// The object `row` is about, as a reference the window can follow.
    pub(super) fn involved_ref(row: &EventRow) -> ObjectRef {
        ObjectRef::from_api_version(
            &row.involved.api_version,
            row.involved.kind.clone(),
            row.involved.namespace.clone(),
            row.involved.name.clone(),
        )
    }

    /// Enter: opens (or focuses) the selected event's involved object's detail
    /// panel - the keyboard twin of clicking its link in the detail strip.
    pub(super) fn on_action_open_involved(
        &mut self,
        _: &OpenInvolvedObject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.selected_event(cx) else {
            return;
        };
        window.dispatch_action(
            Box::new(FollowReference {
                context_name: self.scope.context_name.clone(),
                target: Self::involved_ref(&row),
                mode: crate::ui::nav::OpenMode::Foreground,
            }),
            cx,
        );
    }

    pub(super) fn on_action_focus_search(
        &mut self,
        _: &FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search = self.search_input(window, cx);
        search.read(cx).focus_handle(cx).focus(window, cx);
    }

    /// Escape in the search box: clears it and hands focus back to the table, so
    /// the keyboard is never left stranded in the text field.
    pub(super) fn on_action_clear_search(
        &mut self,
        _: &gpui_kit::component::input::Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = &self.search {
            search.update(cx, |search, cx| search.set_value("", window, cx));
        }
        self.focus_table(window, cx);
        cx.notify();
    }

    fn focus_table(&self, window: &mut Window, cx: &mut App) {
        match &self.table {
            Some(table) => table.read(cx).focus_handle(cx).focus(window, cx),
            None => self.focus_handle.focus(window, cx),
        }
    }

    pub(super) fn on_action_filter_type(
        &mut self,
        _: &FilterByType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_filter(Facet::Type, window, cx);
    }

    pub(super) fn on_action_filter_kind(
        &mut self,
        _: &FilterByKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_filter(Facet::Kind, window, cx);
    }

    pub(super) fn on_action_filter_reason(
        &mut self,
        _: &FilterByReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_filter(Facet::Reason, window, cx);
    }

    /// Opens `facet`'s picker over the values present in this panel's scope.
    /// Each toggle applies at once; the panel is notified so its layout - and
    /// so its saved filters - follow.
    pub(super) fn open_filter(
        &mut self,
        facet: Facet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.scoped_rows(cx);
        let mut offered = options(&rows, facet);
        // A chosen value no longer present stays offered, so it can be cleared.
        for value in self.filters.values(facet) {
            if !offered.contains(value) {
                offered.push(value.clone());
            }
        }
        offered.sort();
        let chosen = self.filters.values(facet).clone();
        let this = cx.weak_entity();
        FacetPicker::open(
            facet,
            offered,
            chosen,
            move |facet, value, _window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.filters.toggle(facet, &value);
                    cx.emit(gpui_kit::component::dock::PanelEvent::LayoutChanged);
                    cx.notify();
                });
            },
            window,
            cx,
        );
    }

    /// Removes one value from `facet`'s filter - an active chip's close.
    pub(super) fn remove_filter(&mut self, facet: Facet, value: &str, cx: &mut Context<Self>) {
        self.filters.remove(facet, value);
        cx.emit(gpui_kit::component::dock::PanelEvent::LayoutChanged);
        cx.notify();
    }

    pub(super) fn on_action_clear_filters(
        &mut self,
        _: &ClearFilters,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_filters(cx);
    }

    /// Clears every filter - `x`, and the Clear filters button.
    pub(super) fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.filters.clear();
        cx.emit(gpui_kit::component::dock::PanelEvent::LayoutChanged);
        cx.notify();
    }
}
