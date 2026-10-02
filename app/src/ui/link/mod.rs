//! References in detail views: drawn as links when they can be followed, and
//! followed through one action the window handles - by click, or by keyboard
//! through the "Go to…" picker.
//!
//! `resource-links` sections 2.2, 3.2 and 4. [`reference`] owns how one
//! reference looks and what clicking it does, [`go_to`] the picker and its
//! command; which kinds are followable is `ui::viewer`'s decision, and opening
//! the destination is `MainWindow`'s.

mod go_to;
mod reference;

#[cfg(test)]
pub(crate) use go_to::{GO_TO_COMMAND_ID, GO_TO_DEFAULT_BINDING};
pub use go_to::{
    GoToEntry, GoToReference, LINKS_KEY_CONTEXT, followable, go_to_key, open as open_go_to,
    register_commands,
};
pub use reference::{FollowReference, references};
#[cfg(test)]
mod tests;
