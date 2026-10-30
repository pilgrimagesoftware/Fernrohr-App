//! Every single-line text field's shared builder (#250): a gpui-component
//! [`Input`] with its clear button - the muted ✕ at the right inside the frame,
//! drawn only while the field holds text. A click empties the field, keeps
//! focus in it and sends the same change event typing does, so a filter
//! updates.
//!
//! Left out: a masked field (a tunnel's private key), where one stray click
//! shouldn't wipe a secret, and multi-line text areas, which `Input` never
//! gives the button. The logs panel's selector field is the logs lane's to
//! adopt.

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{App, Entity};

/// `state`'s field, with the clear button unless it is masked.
pub(crate) fn text_field(state: &Entity<InputState>, cx: &App) -> Input {
    Input::new(state).cleanable(!state.read(cx).presentation().is_masked())
}

#[cfg(test)]
mod tests;
