//! The keyboard-first selection rule for a filterable list (`Command`) whose host keeps
//! its own selected row (keyboard-first.md: one selection, moved by keys and clicks, never
//! by hover). `Command` reports a hover as a selection too, so only keyboard input moves
//! the host's row; Enter confirms that row, and a click confirms the row clicked.

use gpui_kit::component::command::Command;
use gpui_kit::{Context, WeakEntity, Window};

/// Wires `command`'s select and confirm to `host`: `selected` is the host's row, moved
/// only by the keyboard, and `choose` acts on the confirmed row.
pub(crate) fn follow_keyboard<V: 'static>(
    command: Command,
    host: WeakEntity<V>,
    selected: fn(&mut V) -> &mut Option<usize>,
    choose: fn(&mut V, usize, &mut Window, &mut Context<V>),
) -> Command {
    command
        .on_select({
            let host = host.clone();
            move |index_path, window, cx| {
                if !window.last_input_was_keyboard() {
                    return;
                }
                let _ = host.update(cx, |host, _| *selected(host) = Some(index_path.row));
            }
        })
        .on_confirm(move |index_path, window, cx| {
            let _ = host.update(cx, |host, cx| {
                let index = if window.last_input_was_keyboard() {
                    selected(host).unwrap_or(index_path.row)
                } else {
                    index_path.row
                };
                choose(host, index, window, cx);
            });
        })
}
