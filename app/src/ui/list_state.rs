//! What a list panel shows besides its rows (`list-loading-indicator` D2-D4):
//! a loading indicator while its first list is under way, an empty state once
//! a list has come back with nothing (or a filter has hidden everything), and
//! a small refreshing spinner in its header while it relists with rows on
//! screen. The object list, Pods and events browser panels all decide and draw
//! these here, from their store's [`LoadPhase`].
//!
//! Both indicators wait out [`LIST_INDICATOR_DELAY`] first ([`Delay`]), so a
//! list that loads quickly never flickers one.

use crate::consts::LIST_INDICATOR_DELAY;
use crate::k8s::resource::load_phase::LoadPhase;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The loading indicator's debug selector.
pub(crate) const LOADING_SELECTOR: &str = "list-loading";
/// The empty state's.
pub(crate) const EMPTY_SELECTOR: &str = "list-empty";
/// The header's refreshing spinner's.
pub(crate) const REFRESHING_SELECTOR: &str = "list-refreshing";
/// The refreshing spinner's tooltip.
pub(crate) const REFRESHING_TOOLTIP: &str = "Refreshing";
/// What the empty state says when rows exist but the filter hides them all.
pub(crate) const FILTERED_EMPTY: &str = "No rows match the filter";

/// What a list panel's table area shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TableArea {
    /// The rows.
    Table,
    /// The first list is under way; `received` rows of it have arrived.
    Loading { received: usize },
    /// No rows to show, and why, in words.
    Empty(String),
}

/// Decides the table area from the store's `phase`, how many rows are in the
/// panel's scope (`scoped`), and how many of those its filters leave
/// (`visible`). Until the first list completes the table isn't drawn at all,
/// so a half-arrived list never shows as if it were the whole.
pub(crate) fn table_area(
    phase: LoadPhase,
    scoped: usize,
    visible: usize,
    kind: &str,
    namespaces: Option<&[String]>,
) -> TableArea {
    match phase {
        LoadPhase::FirstLoad { received } => TableArea::Loading { received },
        LoadPhase::Loaded | LoadPhase::Refreshing { .. } if scoped == 0 => {
            TableArea::Empty(empty_text(kind, namespaces))
        }
        LoadPhase::Loaded | LoadPhase::Refreshing { .. } if visible == 0 => {
            TableArea::Empty(FILTERED_EMPTY.to_string())
        }
        LoadPhase::Loaded | LoadPhase::Refreshing { .. } => TableArea::Table,
    }
}

/// "No Pods in team-a": the kind, and the namespace scope for a namespaced
/// kind (`None` for a cluster-scoped one).
pub(crate) fn empty_text(kind: &str, namespaces: Option<&[String]>) -> String {
    match namespaces {
        None => format!("No {kind}"),
        Some([]) => format!("No {kind} in any namespace"),
        Some([namespace]) => format!("No {kind} in {namespace}"),
        Some(namespaces) => format!("No {kind} in {} namespaces", namespaces.len()),
    }
}

/// The debug selector of a line of a list state reading `text`, so a test can
/// assert the words as well as the state.
pub(crate) fn text_selector(text: &str) -> String {
    format!("list-text {text}")
}

/// One line of a list state's text, findable by [`text_selector`].
fn line(text: String) -> Div {
    let selector = text_selector(&text);
    div().debug_selector(move || selector).child(text)
}

/// The table area for anything but [`TableArea::Table`]: a spinner and
/// "Loading <kind>…", with a running count once rows arrive - drawn only once
/// `shown` - or the empty state's text.
fn placeholder(area: &TableArea, kind: &str, shown: bool, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let centered = || {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .text_color(muted)
    };
    match area {
        TableArea::Loading { .. } if !shown => div().size_full().into_any_element(),
        TableArea::Loading { received } => centered()
            .debug_selector(|| LOADING_SELECTOR.to_string())
            .child(Spinner::new().color(muted))
            .child(line(format!("Loading {kind}…")))
            .when(*received > 0, |this| {
                this.child(line(format!("{received} received")).text_sm())
            })
            .into_any_element(),
        TableArea::Empty(text) => centered()
            .debug_selector(|| EMPTY_SELECTOR.to_string())
            .child(line(text.clone()))
            .into_any_element(),
        TableArea::Table => div().into_any_element(),
    }
}

/// The table area: `table` itself, with the loading or empty view laid over
/// it for anything but [`TableArea::Table`]. The table stays mounted under
/// the overlay so it keeps keyboard focus, and the panel's keys - a
/// namespace jump back out of an empty scope, say - still reach the panel.
pub(crate) fn table_area_element(
    table: AnyElement,
    area: &TableArea,
    kind: &str,
    shown: bool,
    cx: &App,
) -> AnyElement {
    let overlay = match area {
        TableArea::Table => None,
        area => Some(
            div()
                .absolute()
                .inset_0()
                .bg(cx.theme().background)
                .child(placeholder(area, kind, shown, cx)),
        ),
    };
    div()
        .relative()
        .size_full()
        .child(table)
        .children(overlay)
        .into_any_element()
}

/// The header's refreshing spinner, with its tooltip.
pub(crate) fn refreshing(cx: &App) -> AnyElement {
    crate::ui::icon_tooltip::with_tooltip(
        "list-refreshing-tooltip",
        REFRESHING_TOOLTIP,
        Spinner::new().xsmall().color(cx.theme().muted_foreground),
    )
    .debug_selector(|| REFRESHING_SELECTOR.to_string())
    .into_any_element()
}

/// Holds an indicator back until its phase has lasted
/// [`LIST_INDICATOR_DELAY`] (design D3). A view keeps one per indicator and
/// asks [`Delay::ready`] each render.
#[derive(Default)]
pub(crate) struct Delay {
    shown: bool,
    pending: Option<Task<()>>,
}

impl Delay {
    /// Whether the indicator may show now. While `active`, the first call
    /// starts a timer that redraws `V` once the delay has passed; once not
    /// `active`, the delay starts over.
    pub(crate) fn ready<V: 'static>(
        &mut self,
        active: bool,
        delay: fn(&mut V) -> &mut Delay,
        cx: &mut Context<V>,
    ) -> bool {
        if !active {
            self.shown = false;
            self.pending = None;
            return false;
        }
        if !self.shown && self.pending.is_none() {
            self.pending = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(LIST_INDICATOR_DELAY).await;
                let _ = this.update(cx, |view, cx| {
                    let delay = delay(view);
                    delay.shown = true;
                    delay.pending = None;
                    cx.notify();
                });
            }));
        }
        self.shown
    }
}

/// A panel's two delays: the first-load indicator's and the refreshing
/// spinner's.
#[derive(Default)]
pub(crate) struct Indicators {
    pub(crate) loading: Delay,
    pub(crate) refreshing: Delay,
}

#[cfg(test)]
mod tests;
