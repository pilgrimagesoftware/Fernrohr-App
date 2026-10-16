//! `key-hint-rows`: a panel's row of key hints, which wraps like any flex row
//! until it would need more than [`MAX_HINT_LINES`](crate::consts::MAX_HINT_LINES)
//! lines, then collapses to the hints its panel marked essential plus an
//! overflow control (D1-D4).
//!
//! Collapsing hides a label, never a command: every hidden hint's key still
//! works, the overflow control's tooltip lists it, and the control opens the
//! Key Hints overlay (`?`), which lists every key available where focus is.
//!
//! Not for keys shown beside individual controls (dialogs, the Logs control
//! bar, the picker), nor the shell panel's row, which always lists its
//! reserved keys (`exec-terminal`).

mod fit;
mod overflow;

// Selectors for tests to find the overflow control and what it lists.
#[cfg(test)]
pub(crate) use overflow::{TOOLTIP, control_selector, hidden_selector};

pub use overflow::init;

use fit::{Fit, ItemKind, Measured};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

/// One hint: a key (or a group of keys) and what it does, drawn as the keys
/// then the label. The overflow control's tooltip lists a hidden hint by the
/// same keys and label.
pub struct KeyHint {
    keys: Vec<Kbd>,
    label: SharedString,
    role: Role,
    element: AnyElement,
}

/// When a hint is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Always, until the row collapses.
    Hint,
    /// Always, collapsed or not.
    Essential,
    /// Only while the row is collapsed: a compact stand-in for hints it hides.
    Summary,
}

impl KeyHint {
    /// The plain hint: `key` and `label`.
    pub fn new(key: Kbd, label: impl Into<SharedString>) -> Self {
        Self::styled(key, label, |body| body)
    }

    /// A hint whose drawn element is [`KeyHint::body`] passed through `style`,
    /// for a hint that also carries an id, a debug selector or a click.
    pub fn styled<E: IntoElement>(
        key: Kbd,
        label: impl Into<SharedString>,
        style: impl FnOnce(Div) -> E,
    ) -> Self {
        Self::styled_group(vec![key], label, style)
    }

    /// One hint for several keys, such as a panel's tab keys - every key,
    /// then `label` - its element passed through `style`.
    pub fn styled_group<E: IntoElement>(
        keys: Vec<Kbd>,
        label: impl Into<SharedString>,
        style: impl FnOnce(Div) -> E,
    ) -> Self {
        let label = label.into();
        let element = style(Self::body(keys.clone(), label.clone())).into_any_element();
        Self {
            keys,
            label,
            role: Role::Hint,
            element,
        }
    }

    /// The hint for `action`: its live key within `context`, or `fallback`
    /// when the keymap has none.
    pub fn for_action(
        action: &dyn Action,
        context: Option<&str>,
        fallback: &str,
        label: impl Into<SharedString>,
        window: &Window,
    ) -> Self {
        Self::new(live_key(action, context, fallback, window), label)
    }

    /// Kept when the row collapses (D2). Each row decides which of its hints
    /// matter most; a row that marks none keeps its first.
    pub fn essential(mut self) -> Self {
        self.role = Role::Essential;
        self
    }

    /// Drawn only while the row is collapsed, standing in for hints it hides,
    /// such as one group of tab keys for a tab hint each, so a collapsed row
    /// stays short. It isn't part of the fit: the row expands once the hints
    /// it stands in for fit.
    pub fn summary(mut self) -> Self {
        self.role = Role::Summary;
        self
    }

    /// A hint's standard element: the keys, then the label, on one line.
    pub fn body(keys: Vec<Kbd>, label: SharedString) -> Div {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .whitespace_nowrap()
            .children(keys)
            .child(label)
    }
}

/// `action`'s key within `context` from the live keymap, else `fallback`.
pub fn live_key(
    action: &dyn Action,
    context: Option<&str>,
    fallback: &str,
    window: &Window,
) -> Kbd {
    Kbd::binding_for_action(action, context, window)
        .unwrap_or_else(|| Kbd::new(Keystroke::parse(fallback).expect("a valid default key")))
}

enum Item {
    Control(AnyElement),
    // Boxed: a hint carries its keys and element, far larger than a control.
    Hint(Box<KeyHint>),
}

impl Item {
    fn kind(&self) -> ItemKind {
        match self {
            Item::Control(_) => ItemKind::Control,
            Item::Hint(hint) => match hint.role {
                Role::Hint => ItemKind::Hint,
                Role::Essential => ItemKind::EssentialHint,
                Role::Summary => ItemKind::Summary,
            },
        }
    }
}

/// A wrapping row of key hints that collapses past `MAX_HINT_LINES` lines.
/// Style it like a `div`; set the gap between hints with
/// [`KeyHintRow::hint_gap`], which the fit measurement uses too.
#[derive(IntoElement)]
pub struct KeyHintRow {
    id: SharedString,
    base: Div,
    gap: Option<Pixels>,
    items: Vec<Item>,
}

impl KeyHintRow {
    /// An empty row. `id` keeps its collapsed state between frames, so it
    /// must be unique among the rows drawn in one view.
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            base: div().flex().flex_wrap().items_center(),
            gap: None,
            items: Vec::new(),
        }
    }

    /// The space between hints, across and between lines' items. Defaults to
    /// the theme's control gap.
    pub fn hint_gap(mut self, gap: Pixels) -> Self {
        self.gap = Some(gap);
        self
    }

    pub fn hint(mut self, hint: KeyHint) -> Self {
        self.items.push(Item::Hint(Box::new(hint)));
        self
    }

    pub fn hints(mut self, hints: impl IntoIterator<Item = KeyHint>) -> Self {
        self.items
            .extend(hints.into_iter().map(|hint| Item::Hint(Box::new(hint))));
        self
    }

    /// Elements always drawn, collapsed or not, such as icon buttons that are
    /// commands' mouse routes.
    pub fn controls(mut self, controls: impl IntoIterator<Item = AnyElement>) -> Self {
        self.items.extend(controls.into_iter().map(Item::Control));
        self
    }
}

impl Styled for KeyHintRow {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for KeyHintRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let fit =
            window.use_keyed_state(ElementId::Name(self.id.clone()), cx, |_, _| Fit::default());
        let collapsed = fit.read(cx).collapsed;
        let gap = self
            .gap
            .unwrap_or_else(|| crate::ui::space::spacing(cx).control_gap);
        let kinds: Vec<ItemKind> = self.items.iter().map(Item::kind).collect();
        let shown = fit::shown(&kinds, collapsed);

        let mut all = Vec::with_capacity(self.items.len());
        let mut drawn = Vec::new();
        let mut elements = Vec::new();
        let mut hidden = Vec::new();
        for (index, (item, shown)) in self.items.into_iter().zip(shown).enumerate() {
            let key = match &item {
                Item::Control(_) => SharedString::from(format!("\u{0}control {index}")),
                Item::Hint(hint) => hint.label.clone(),
            };
            // A summary isn't drawn expanded, so it isn't packed to decide
            // whether the row fits.
            let summary = item.kind() == ItemKind::Summary;
            if !summary {
                all.push(key.clone());
            }
            match (item, shown) {
                (Item::Control(element), _) => {
                    drawn.push(key);
                    elements.push(element);
                }
                (Item::Hint(hint), true) => {
                    drawn.push(key);
                    elements.push(hint.element);
                }
                (Item::Hint(hint), false) if !summary => hidden.push((hint.keys, hint.label)),
                (Item::Hint(_), false) => {}
            }
        }
        let overflow = (!hidden.is_empty()).then(|| overflow::control(&self.id, hidden, window));

        self.base
            .relative()
            .gap_x(gap)
            .gap_y_1()
            .debug_selector({
                let id = self.id.clone();
                move || row_selector(&id, collapsed)
            })
            .children(elements)
            .children(overflow)
            // Last, and out of the flow: it spans the row, so its bounds are
            // the row's own width for the fit.
            .child(div().absolute().inset_0())
            .on_children_prepainted(move |bounds, _, cx| {
                fit.update(cx, |fit, cx| {
                    let changed = fit.measure(Measured {
                        all: &all,
                        drawn: &drawn,
                        bounds: &bounds,
                        gap,
                    });
                    if changed {
                        cx.notify();
                    }
                });
            })
    }
}

/// The debug selector of the row `id`, which says whether it is collapsed, so
/// a test can wait for the state it expects.
pub fn row_selector(id: &str, collapsed: bool) -> String {
    let state = if collapsed { "collapsed" } else { "expanded" };
    format!("key-hint-row {id} {state}")
}

#[cfg(test)]
mod tests;
