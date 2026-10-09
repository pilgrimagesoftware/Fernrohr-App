//! `manual-confirmation-tunnels`: a tunnel Fernrohr starts nothing for. The user
//! brings the network path up by hand - a VPN from a menu-bar client - and the
//! connections bound to it wait until they say Proceed (or Cancel).
//!
//! [`ManualTransport`] is the forward's transport: it probes the API server when
//! asked to (`skip_when_reachable`), else publishes a pending confirmation and waits
//! for its answer. [`ManualConfirmations`] is the app's list of those pending
//! confirmations, which the status bar, the picker and the Proceed and Cancel
//! commands read and resolve. [`ManualTunnel`] is the supervised forward the tunnel
//! registry shares - keyed by the tunnel alone, so every context bound to it shares
//! one confirmation.

mod confirmations;
mod handle;
mod transport;

pub use confirmations::{Decision, ManualConfirmations};
pub use handle::ManualTunnel;
pub(crate) use transport::{ManualTransport, ProbeTarget};
