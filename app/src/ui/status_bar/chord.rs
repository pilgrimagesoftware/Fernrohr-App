//! The pending-chord indicator (`pending-chord-indicator`): while the window
//! holds the start of a multi-step key binding, the status bar shows the keys
//! so far (`⌘K …`) and, above it, the keys that would complete a binding live
//! in the focused context, each with its command's title.
//!
//! It reads GPUI's own pending input rather than tracking keystrokes itself, so
//! it can't disagree with dispatch: [`StatusBarView::watch_pending_input`]
//! observes the window's pending input, and each change re-reads
//! `Window::pending_input`, which already drops input left over from an
//! earlier focus. Nothing here is focusable or handles keys, so the next key
//! reaches its binding exactly as it would without the indicator.

use super::StatusBarView;
use crate::command::CommandRegistry;
use crate::keymap::{self, Completion, LiveKeymap};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

/// How many completions the popover lists before "… and N more".
const MAX_ROWS: usize = 8;

/// The keys the window is holding, and what would complete them.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PendingChord {
    pub(super) keys: Vec<Keystroke>,
    pub(super) completions: Vec<Completion>,
}

/// The debug selector of the indicator, and of each completion's row.
pub(crate) const INDICATOR_SELECTOR: &str = "chord-indicator";
pub(crate) const MORE_SELECTOR: &str = "chord-more";
pub(crate) fn completion_selector(id: &str) -> String {
    format!("chord-completion {id}")
}

impl StatusBarView {
    /// Follows `window`'s pending input for as long as this bar lives.
    pub(crate) fn watch_pending_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._pending_input = Some(cx.observe_pending_input(window, |this, window, cx| {
            this.read_pending_input(window, cx)
        }));
        self.read_pending_input(window, cx);
    }

    fn read_pending_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = window
            .pending_input()
            .map(|pending| pending.keystrokes().to_vec());
        self.chord = keys.map(|keys| {
            let completions = match (
                cx.try_global::<CommandRegistry>(),
                cx.try_global::<LiveKeymap>(),
            ) {
                (Some(registry), Some(live)) => keymap::completions(
                    registry,
                    live.config(),
                    &keys,
                    &window.context_stack(),
                    cx.keyboard_mapper().as_ref(),
                ),
                _ => Vec::new(),
            };
            PendingChord { keys, completions }
        });
        cx.notify();
    }

    /// The keys the bar shows as pending (`unparse`d) and the completions'
    /// command ids, or `None` while nothing is pending.
    #[cfg(test)]
    pub(crate) fn pending_chord(&self) -> Option<(Vec<String>, Vec<&'static str>)> {
        self.chord.as_ref().map(|chord| {
            (
                chord.keys.iter().map(Keystroke::unparse).collect(),
                chord.completions.iter().map(|c| c.id).collect(),
            )
        })
    }
}

/// The indicator, with the completions in a popover above it. Plain elements
/// with no focus handle or key listener: it can't take focus or a key.
pub(super) fn render(chord: &PendingChord, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let rows = chord.completions.iter().take(MAX_ROWS).map(|completion| {
        let selector = completion_selector(completion.id);
        div()
            .debug_selector(move || selector)
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_1()
                    .children(completion.remaining.iter().cloned().map(Kbd::new)),
            )
            .child(completion.title)
    });
    let more = chord.completions.len().saturating_sub(MAX_ROWS);
    let popover = (!chord.completions.is_empty()).then(|| {
        deferred(
            div()
                .absolute()
                .bottom_full()
                .left_0()
                .mb_1()
                .p_2()
                .min_w(px(240.))
                .flex()
                .flex_col()
                .gap_1()
                .text_sm()
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .border_1()
                .border_color(theme.border)
                .rounded_md()
                .shadow_md()
                .children(rows)
                .children((more > 0).then(|| {
                    div()
                        .debug_selector(|| MORE_SELECTOR.into())
                        .text_color(muted)
                        .child(format!("… and {more} more"))
                })),
        )
    });
    div()
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap_1()
        .debug_selector(|| INDICATOR_SELECTOR.into())
        .children(chord.keys.iter().cloned().map(Kbd::new))
        .child(div().text_color(muted).child("…"))
        .children(popover)
        .into_any_element()
}
