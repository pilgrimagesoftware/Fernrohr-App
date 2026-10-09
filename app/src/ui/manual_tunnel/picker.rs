//! Which waiting manual tunnel a Proceed or Cancel command means, when several
//! are waiting: each tunnel with the contexts it holds. Up/Down or a filter, then
//! Enter, answers the chosen one only; Escape answers none. Hover moves only the
//! list's own highlight, never the row Enter acts on (`keyboard-first.md`).

use crate::tunnel::manual::{Decision, ManualConfirmations};
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::command::{Command as CommandList, CommandItem, CommandState};
use gpui_kit::*;

/// The debug selector of `tunnel_id`'s row.
pub(crate) fn row_selector(tunnel_id: &str) -> String {
    format!("manual-tunnel-row {tunnel_id}")
}

/// Opens the picker in `window`, answering the chosen tunnel with `decision`.
pub(crate) fn open(decision: Decision, window: &mut Window, cx: &mut App) {
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    // The tunnels as they wait now; one answered meanwhile is skipped on choosing.
    let tunnels: Vec<(String, String)> = ManualConfirmations::entity(cx)
        .map(|entity| {
            entity
                .read(cx)
                .pending()
                .iter()
                .map(|entry| {
                    let label = if entry.contexts.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{} ({})", entry.name, entry.contexts.join(", "))
                    };
                    (entry.tunnel_id.clone(), label)
                })
                .collect()
        })
        .unwrap_or_default();
    let return_focus = window.focused(cx);
    let picker = cx.new(|cx| TunnelPicker {
        state: cx.new(|cx| CommandState::new(window, cx)),
        selected: Some(0),
        tunnels,
        decision,
        return_focus,
    });
    let state = picker.read(cx).state.clone();
    let title = match decision {
        Decision::Proceed => "Proceed with Which Tunnel?",
        Decision::Cancel => "Cancel Which Tunnel?",
    };
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let picker = picker.clone();
        dialog
            .title(title)
            .content(move |content, _window, _cx| content.child(picker.clone()))
    });
    state.update(cx, |state, cx| state.focus(window, cx));
}

struct TunnelPicker {
    state: Entity<CommandState>,
    /// The row Enter acts on: the keyboard's, or the last clicked.
    selected: Option<usize>,
    /// Each waiting tunnel's id and label.
    tunnels: Vec<(String, String)>,
    decision: Decision,
    return_focus: Option<FocusHandle>,
}

impl TunnelPicker {
    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((tunnel_id, _)) = self.tunnels.get(index).cloned() else {
            return;
        };
        window.close_dialog(cx);
        if let Some(focus) = &self.return_focus {
            focus.focus(window, cx);
        }
        ManualConfirmations::resolve(cx, &tunnel_id, self.decision);
    }
}

impl Render for TunnelPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let items: Vec<CommandItem> = self
            .tunnels
            .iter()
            .map(|(tunnel_id, label)| {
                let selector = row_selector(tunnel_id);
                let label = label.clone();
                CommandItem::new().label(label.clone()).child(move |_, _| {
                    let selector = selector.clone();
                    div()
                        .flex_1()
                        .debug_selector(move || selector)
                        .child(label.clone())
                })
            })
            .collect();
        CommandList::new(&self.state)
            .items(items)
            .placeholder("Filter tunnels…")
            .on_select({
                let this = this.clone();
                move |index_path, window, cx| {
                    if !window.last_input_was_keyboard() {
                        return;
                    }
                    let _ = this.update(cx, |this, _| this.selected = Some(index_path.row));
                }
            })
            .on_confirm(move |index_path, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let index = if window.last_input_was_keyboard() {
                        this.selected.unwrap_or(index_path.row)
                    } else {
                        index_path.row
                    };
                    this.choose(index, window, cx);
                });
            })
    }
}
