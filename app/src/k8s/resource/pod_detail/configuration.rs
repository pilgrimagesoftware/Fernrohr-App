//! The Configuration tab: every ConfigMap and Secret a pod references,
//! expanded in place, with Secret values revealed one at a time
//! (`pod-configuration-tab`).
//!
//! [`entries`] decides what the tab lists, from the references pod detail
//! already projects; [`state`] holds each card's contents and the revealed
//! values; [`fetch`] reads the cards' objects when the tab is first shown;
//! [`actions`] is the panel's side (loading, revealing, hiding); [`view`] draws
//! the cards.

mod actions;
mod entries;
mod fetch;
mod state;
mod view;

#[cfg(test)]
pub(super) use view::{expand_button_id, reveal_button_id, value_id};

pub(super) use state::ConfigurationState;
#[cfg(test)]
pub(super) use state::{CardContents, Reveal};
