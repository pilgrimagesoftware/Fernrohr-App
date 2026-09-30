//! Owns `TunnelEditor`'s small trait surface: emitting [`TunnelEditorEvent`] and
//! reporting its focus handle.

use super::*;

impl EventEmitter<TunnelEditorEvent> for TunnelEditor {}

impl Focusable for TunnelEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
