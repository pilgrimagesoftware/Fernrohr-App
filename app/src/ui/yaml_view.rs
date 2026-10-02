//! The detail panels' YAML view (`resource-detail-ui-improvements` 2): the
//! object's manifest, one line per row, scrolling both ways, with every nested
//! mapping or sequence foldable.
//!
//! [`YamlLines`] works out what folds, [`YamlFolds`] is which blocks are folded
//! (the panel's state, reset when the YAML changes), and [`render`] draws
//! them. Each foldable line has a gutter chevron that is a tab stop - Tab
//! reaches it, Enter or Space folds it, as a click does - and the panels
//! register Fold All and Unfold All as commands.

use crate::ui::typography::TypeRole as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;
use std::collections::HashSet;
use std::rc::Rc;

/// One line of the manifest, and - when it opens a nested block - the last
/// line of that block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YamlLine {
    pub text: String,
    /// The block this line opens ends at this line index, inclusive; `None`
    /// for a line that opens nothing.
    pub fold_end: Option<usize>,
}

/// A manifest split into lines, with each nested block's extent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct YamlLines {
    pub lines: Vec<YamlLine>,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

impl YamlLines {
    /// Splits `yaml` and finds every foldable block:
    /// - a `key:` with nothing after the colon opens the lines below it that
    ///   are indented further, and the `- ` items at its own indent - how
    ///   `serde_yaml` writes a sequence under its key;
    /// - a `- ` item opens the lines of that item indented past its dash.
    ///
    /// Blank lines inside a block belong to it.
    pub fn parse(yaml: &str) -> Self {
        let texts: Vec<&str> = yaml.lines().collect();
        let lines = texts
            .iter()
            .enumerate()
            .map(|(index, text)| YamlLine {
                text: text.to_string(),
                fold_end: Self::block_end(&texts, index),
            })
            .collect();
        Self { lines }
    }

    fn block_end(texts: &[&str], start: usize) -> Option<usize> {
        let line = texts[start];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        let indent = indent_of(line);
        let opens_key = trimmed.ends_with(':');
        let is_item = trimmed.starts_with("- ") || trimmed == "-";
        if !opens_key && !is_item {
            return None;
        }
        let mut end = None;
        for (index, text) in texts.iter().enumerate().skip(start + 1) {
            let next = text.trim_start();
            if next.is_empty() {
                continue;
            }
            let next_indent = indent_of(text);
            let child = next_indent > indent
                || (opens_key && !is_item && next_indent == indent && next.starts_with("- "));
            if !child {
                break;
            }
            end = Some(index);
        }
        end
    }

    /// Every line that opens a block.
    pub fn foldable(&self) -> impl Iterator<Item = usize> + '_ {
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.fold_end.is_some())
            .map(|(index, _)| index)
    }
}

/// Which blocks are folded, by the line that opens them, for the YAML it was
/// built over - a different manifest (the object changed) starts unfolded.
#[derive(Clone, Debug, Default)]
pub struct YamlFolds {
    folded: HashSet<usize>,
    yaml: String,
}

impl YamlFolds {
    /// Forgets every fold when `yaml` is not the manifest they were made on.
    pub fn sync(&mut self, yaml: &str) {
        if self.yaml != yaml {
            self.folded.clear();
            self.yaml = yaml.to_string();
        }
    }

    pub fn is_folded(&self, line: usize) -> bool {
        self.folded.contains(&line)
    }

    pub fn toggle(&mut self, line: usize) {
        if !self.folded.remove(&line) {
            self.folded.insert(line);
        }
    }

    pub fn fold_all(&mut self, lines: &YamlLines) {
        self.folded = lines.foldable().collect();
    }

    pub fn unfold_all(&mut self) {
        self.folded.clear();
    }
}

actions!(yaml_view, [FoldAll, UnfoldAll]);

/// The default keys for [`FoldAll`] and [`UnfoldAll`] in each detail panel.
pub const FOLD_ALL_KEY: &str = "z";
pub const UNFOLD_ALL_KEY: &str = "shift-z";

/// A detail panel's YAML view state: its folds and its scroll position.
#[derive(Default)]
pub struct YamlViewState {
    pub folds: YamlFolds,
    pub scroll: ScrollHandle,
}

impl YamlViewState {
    /// Draws `yaml` under this state's folds, flipping a block through
    /// `on_toggle` - the panel calls back into [`Self::toggle`].
    pub fn element(&mut self, yaml: &str, on_toggle: OnToggle, cx: &App) -> AnyElement {
        self.folds.sync(yaml);
        let lines = YamlLines::parse(yaml);
        render(&lines, &self.folds, &self.scroll, on_toggle, cx)
    }

    pub fn toggle(&mut self, line: usize) {
        self.folds.toggle(line);
    }

    /// Folds every block of `yaml`.
    pub fn fold_all(&mut self, yaml: &str) {
        self.folds.sync(yaml);
        self.folds.fold_all(&YamlLines::parse(yaml));
    }

    pub fn unfold_all(&mut self) {
        self.folds.unfold_all();
    }
}

/// The lines shown under `folds`: each with its index, whether it opens a
/// block, and whether that block is folded. A folded block's own lines are
/// left out.
pub fn visible_lines<'a>(
    lines: &'a YamlLines,
    folds: &'a YamlFolds,
) -> Vec<(usize, &'a YamlLine, bool)> {
    let mut shown = Vec::new();
    let mut index = 0;
    while index < lines.lines.len() {
        let line = &lines.lines[index];
        let folded = line.fold_end.is_some() && folds.is_folded(index);
        shown.push((index, line, folded));
        index = match (folded, line.fold_end) {
            (true, Some(end)) => end + 1,
            _ => index + 1,
        };
    }
    shown
}

/// The gutter toggle of the block opened at `line`.
pub fn fold_toggle_id(line: usize) -> ElementId {
    ElementId::NamedInteger("yaml-fold".into(), line as u64)
}

/// The debug selector of the drawn line `line`.
pub fn line_selector(line: usize) -> String {
    format!("yaml-line-{line}")
}

/// What a gutter toggle runs: the panel flips the block opened at its line.
pub type OnToggle = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// Draws `lines` under `folds` in a scroll container on `scroll`, both ways,
/// with a toggle in the gutter of every line that opens a block.
pub fn render(
    lines: &YamlLines,
    folds: &YamlFolds,
    scroll: &ScrollHandle,
    on_toggle: OnToggle,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let rows = visible_lines(lines, folds)
        .into_iter()
        .map(|(index, line, folded)| {
            let gutter = div()
                .w(px(20.))
                .flex_none()
                .children(line.fold_end.map(|_| {
                    let on_toggle = on_toggle.clone();
                    Button::new(fold_toggle_id(index))
                        .icon(if folded {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .xsmall()
                        .ghost()
                        .tooltip(if folded { "Unfold" } else { "Fold" })
                        .on_click(move |_event, window, cx| on_toggle(index, window, cx))
                }));
            let selector = line_selector(index);
            div()
                .flex()
                .items_center()
                .child(gutter)
                .child(
                    div()
                        .debug_selector(move || selector.clone())
                        .whitespace_nowrap()
                        .child(line.text.clone()),
                )
                .children(folded.then(|| div().pl_2().text_color(muted).child("…")))
        });
    div()
        .id("yaml-view")
        .size_full()
        .overflow_scroll()
        .track_scroll(scroll)
        .code_font(cx)
        .child(div().flex().flex_col().children(rows))
        .into_any_element()
}

#[cfg(test)]
mod tests;
