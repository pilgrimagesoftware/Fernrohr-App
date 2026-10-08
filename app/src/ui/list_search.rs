//! The search/filter box shared by every list panel (#189): `/` focuses it,
//! typing narrows the table to rows whose visible columns contain the text,
//! case-insensitively, and Escape clears it and returns focus to the table.
//!
//! [`ListSearch`] owns the mechanism: the [`InputState`] entity's lazy build,
//! focusing it, clearing it, and what a panel saves and restores with it. A
//! panel supplies what's its own: the registered action `/` dispatches, the
//! `KeyContext` that gates it, the table to refocus on Escape, and which text
//! of a row counts as "visible" - there's no one definition of that across
//! the events browser (three fixed fields, `events_browser::filters`) and an
//! object or Pods list (every column the table renders). [`matches`] is the
//! shared half of that: the actual substring test once a panel has gathered
//! those texts.
//!
//! Its text is what a panel's `DockAreaState` panel data persists as the
//! optional `filter` field (`saved-panel-layouts` 1.6): [`ListSearch::dump`]
//! reads it back out for a panel's own `dump`, and [`ListSearch::restored`]
//! starts a panel with one read in, so both automatic restore and a saved
//! layout keep it. Building the box with [`ListSearch::input`] *before* the
//! first read of a panel's rows is the caller's job: a restored filter has to
//! be in the box before anything reads it, or the first paint shows every row
//! unfiltered until some later event redraws it.

use gpui_kit::component::input::InputState;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

/// One panel's search/filter box, and the text it starts with once restored.
#[derive(Default)]
pub struct ListSearch {
    input: Option<Entity<InputState>>,
    /// The text to start the box with once it is built - a restored panel's
    /// saved search, until [`Self::input`] consumes it.
    initial: Option<String>,
}

impl ListSearch {
    /// A search starting empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// A search starting with `initial`'s text once its box is built - a
    /// restored panel's saved search (`saved-panel-layouts` 1.6), or `None`
    /// for state saved before this field existed, or saved with nothing typed.
    pub fn restored(initial: Option<String>) -> Self {
        Self {
            input: None,
            initial,
        }
    }

    /// The box, created the first time a window renders the panel it's in.
    pub fn input<T: 'static>(
        &mut self,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Entity<InputState> {
        if let Some(input) = &self.input {
            return input.clone();
        }
        let initial = self.initial.take();
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(placeholder);
            if let Some(initial) = initial {
                state = state.default_value(initial);
            }
            state
        });
        cx.subscribe(
            &input,
            |_: &mut T, _, _: &gpui_kit::component::input::InputEvent, cx| cx.notify(),
        )
        .detach();
        self.input = Some(input.clone());
        input
    }

    /// The box's text, trimmed - empty before it is built, so a panel reading
    /// it ahead of its own first render (there isn't one) never has to ask.
    pub fn query(&self, cx: &App) -> String {
        self.input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_string())
            .unwrap_or_default()
    }

    /// Focuses the box, building it with `placeholder` first if this is the
    /// first time - a panel's `/`-bound action.
    pub fn focus<T: 'static>(
        &mut self,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<T>,
    ) {
        let input = self.input(placeholder, window, cx);
        input.read(cx).focus_handle(cx).focus(window, cx);
    }

    /// Whether the box holds focus - always `false` if it was never built.
    /// Test-only: production code never needs to ask, only tests proving the
    /// keyboard route actually moved focus.
    #[cfg(test)]
    pub(crate) fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.input
            .as_ref()
            .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Clears the box and returns focus to `table_focus` - a panel's `Escape`
    /// handler (`gpui_kit::component::input::Escape`), the keyboard twin of
    /// never leaving the keyboard stranded in a text field it just emptied.
    /// Does nothing to the box if it was never built - nothing to clear.
    pub fn clear<T: 'static>(
        &self,
        table_focus: &FocusHandle,
        window: &mut Window,
        cx: &mut Context<T>,
    ) {
        if let Some(input) = &self.input {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        table_focus.focus(window, cx);
    }

    /// What a panel saves with it: the box's current text, or the text it was
    /// given to start with if it was never drawn.
    pub fn dump(&self, cx: &App) -> Option<String> {
        match &self.input {
            Some(input) => Some(input.read(cx).value().to_string()),
            None => self.initial.clone(),
        }
    }

    /// One hint-row entry for the search box: `action`'s live key within
    /// `context` (or `fallback` when the keymap has none), labeled `label`.
    pub fn hint(
        action: &dyn Action,
        context: &'static str,
        fallback: &str,
        label: &'static str,
        window: &mut Window,
    ) -> Div {
        let key = Kbd::binding_for_action(action, Some(context), window)
            .unwrap_or_else(|| Kbd::new(Keystroke::parse(fallback).expect("a valid default key")));
        div().flex().gap_1().items_center().child(key).child(label)
    }
}

/// Whether any of `texts` - a row's visible column text, a panel's own to
/// gather - contains `query`, case-insensitively. An empty query matches
/// everything.
pub fn matches<I, S>(texts: I, query: &str) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    texts
        .into_iter()
        .any(|text| text.as_ref().to_lowercase().contains(&query))
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn an_empty_query_matches_everything() {
        assert!(matches(Vec::<&str>::new(), ""));
        assert!(matches(["anything"], ""));
    }

    #[test]
    fn it_matches_any_text_case_insensitively() {
        assert!(matches(["Pod", "default", "Running"], "run"));
        assert!(matches(["Pod", "default", "Running"], "RUNNING"));
        assert!(!matches(["Pod", "default", "Running"], "pending"));
    }

    #[test]
    fn it_accepts_owned_strings_too() {
        let texts: Vec<String> = vec!["web-1".to_string(), "staging".to_string()];
        assert!(matches(texts, "STAG"));
    }
}
