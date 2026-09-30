//! References in detail views: drawn as links when they can be followed, and
//! followed through one action the window handles.
//!
//! `resource-links` sections 2.2 and 3.2. [`reference`] owns how one
//! reference looks and what clicking it does; which kinds are followable is
//! `ui::viewer`'s decision, and opening the destination is `MainWindow`'s.

mod reference;

pub use reference::{FollowReference, references};
