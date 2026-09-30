//! The app-wide "tunnels.toml changed" signal. Views that cache tunnels or bindings
//! (every window's cluster picker, the Tunnels window) observe [`TunnelsRevision`] and
//! re-read the file when it moves, so a tunnel created in the Tunnels window shows up in
//! an already-open picker without any view reading the file on render. Every UI write
//! through `TunnelStore` calls [`notify_tunnels_changed`] once it succeeds.

use gpui_kit::{App, Global};

/// Bumped once per successful write to `tunnels.toml`. The value itself means nothing;
/// observers only care that it changed.
#[derive(Default)]
pub struct TunnelsRevision(u64);

impl Global for TunnelsRevision {}

/// Tells every observer that tunnels or bindings changed on disk.
pub fn notify_tunnels_changed(cx: &mut App) {
    let next = cx
        .try_global::<TunnelsRevision>()
        .map_or(1, |rev| rev.0 + 1);
    cx.set_global(TunnelsRevision(next));
}
