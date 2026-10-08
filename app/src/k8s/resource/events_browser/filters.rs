//! The events browser's facet filters (design D2): type, involved-object kind
//! and reason, each a multi-select whose options are the values present.
//! Filters apply before the search box (`panel::visible_rows`, `list-search`
//! #189), both client-side.
//!
//! [`EventFilters`] is the state the panel saves; [`FacetPicker`] is the dialog
//! that sets one facet, from the keyboard or a filter button alike.

use super::row::EventRow;
use gpui_kit::component::command::{Command as CommandList, CommandItem, CommandState};
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::rc::Rc;

/// One filterable facet of an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facet {
    Type,
    Kind,
    Reason,
}

impl Facet {
    pub const ALL: [Facet; 3] = [Facet::Type, Facet::Kind, Facet::Reason];

    pub fn title(self) -> &'static str {
        match self {
            Facet::Type => "Type",
            Facet::Kind => "Kind",
            Facet::Reason => "Reason",
        }
    }

    /// `row`'s value for this facet - what a filter on it matches.
    pub fn value(self, row: &EventRow) -> String {
        match self {
            Facet::Type => row.type_.clone().unwrap_or_default(),
            Facet::Kind => row.involved.kind.clone(),
            Facet::Reason => row.reason.clone(),
        }
    }

    fn selector_id(self) -> &'static str {
        match self {
            Facet::Type => "type",
            Facet::Kind => "kind",
            Facet::Reason => "reason",
        }
    }

    /// The debug selector of this facet's filter button.
    pub fn button_selector(self) -> String {
        format!("events-filter-{}", self.selector_id())
    }
}

/// The chosen values per facet. An empty set doesn't filter on that facet;
/// a non-empty one keeps only events with one of its values.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventFilters {
    #[serde(default)]
    pub types: BTreeSet<String>,
    #[serde(default)]
    pub kinds: BTreeSet<String>,
    #[serde(default)]
    pub reasons: BTreeSet<String>,
}

impl EventFilters {
    pub fn values(&self, facet: Facet) -> &BTreeSet<String> {
        match facet {
            Facet::Type => &self.types,
            Facet::Kind => &self.kinds,
            Facet::Reason => &self.reasons,
        }
    }

    fn values_mut(&mut self, facet: Facet) -> &mut BTreeSet<String> {
        match facet {
            Facet::Type => &mut self.types,
            Facet::Kind => &mut self.kinds,
            Facet::Reason => &mut self.reasons,
        }
    }

    /// Adds `value` to `facet`'s selection, or removes it if it's there.
    pub fn toggle(&mut self, facet: Facet, value: &str) {
        let values = self.values_mut(facet);
        if !values.remove(value) {
            values.insert(value.to_string());
        }
    }

    pub fn remove(&mut self, facet: Facet, value: &str) {
        self.values_mut(facet).remove(value);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn is_empty(&self) -> bool {
        Facet::ALL
            .iter()
            .all(|facet| self.values(*facet).is_empty())
    }

    /// Whether `row` passes every facet with a selection.
    pub fn matches(&self, row: &EventRow) -> bool {
        Facet::ALL.iter().all(|facet| {
            let values = self.values(*facet);
            values.is_empty() || values.contains(&facet.value(row))
        })
    }
}

/// The values `facet` takes among `rows`, sorted - a filter's options, built
/// from the events present (design D2). Empty values (an untyped event) are
/// left out: there is nothing to pick them by.
pub fn options(rows: &[EventRow], facet: Facet) -> Vec<String> {
    rows.iter()
        .map(|row| facet.value(row))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

type OnToggle = Rc<dyn Fn(Facet, String, &mut Window, &mut App)>;

/// The dialog that sets one facet's filter: its options, each checked when
/// chosen. Enter or a click toggles the highlighted option and keeps the
/// dialog open for the next; Escape closes it and returns focus to the panel.
pub struct FacetPicker {
    facet: Facet,
    options: Vec<String>,
    chosen: BTreeSet<String>,
    on_toggle: OnToggle,
    state: Entity<CommandState>,
    /// The one selection, moved by the keyboard - `Command` also keeps a
    /// highlight that follows the pointer, which is never this.
    selected: Option<usize>,
}

impl FacetPicker {
    /// Opens the picker for `facet` over `options`, with `chosen` checked.
    pub fn open(
        facet: Facet,
        options: Vec<String>,
        chosen: BTreeSet<String>,
        on_toggle: impl Fn(Facet, String, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !matches!(window.root::<Root>(), Some(Some(_))) {
            return;
        }
        let selected = (!options.is_empty()).then_some(0);
        let picker = cx.new(|cx| Self {
            facet,
            options,
            chosen,
            on_toggle: Rc::new(on_toggle),
            state: cx.new(|cx| CommandState::new(window, cx)),
            selected,
        });
        let state = picker.read(cx).state.clone();
        let title = format!("Filter by {}", facet.title());
        window.open_dialog(cx, move |dialog, _window, _cx| {
            let picker = picker.clone();
            dialog
                .title(title.clone())
                .content(move |content, _window, _cx| content.child(picker.clone()))
        });
        state.update(cx, |state, cx| state.focus(window, cx));
    }

    fn toggle(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(value) = self.options.get(index).cloned() else {
            return;
        };
        if !self.chosen.remove(&value) {
            self.chosen.insert(value.clone());
        }
        (self.on_toggle)(self.facet, value, window, cx);
        cx.notify();
    }
}

impl Render for FacetPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let items: Vec<CommandItem> = self
            .options
            .iter()
            .map(|option| {
                let mark = if self.chosen.contains(option) {
                    "✓ "
                } else {
                    "   "
                };
                CommandItem::new().label(format!("{mark}{option}"))
            })
            .collect();
        div().debug_selector(|| "events-facet-picker".into()).child(
            CommandList::new(&self.state)
                .items(items)
                .placeholder(format!("Filter {}…", self.facet.title().to_lowercase()))
                .on_select({
                    let this = this.clone();
                    move |index_path, window, cx| {
                        if !window.last_input_was_keyboard() {
                            return;
                        }
                        let _ = this.update(cx, |this, cx| {
                            this.selected = Some(index_path.row);
                            cx.notify();
                        });
                    }
                })
                .on_confirm(move |index_path, window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        let index = if window.last_input_was_keyboard() {
                            this.selected.unwrap_or(index_path.row)
                        } else {
                            index_path.row
                        };
                        this.toggle(index, window, cx);
                    });
                }),
        )
    }
}
