//! Retrying a failed context's connection (#416). A context a window holds keeps
//! its session and the very [`ClusterConnection`] entity its panels observe, so
//! the retry runs inside that entity (`ClusterConnection::redial`) rather than
//! swapping in a new one, which nothing would be watching. The picker's Connect
//! retries in place too (`ClusterRegistry::connection_retrying`): with agents
//! holding connections, "no window holds it" no longer means "nobody observes
//! it" (`mcp-connections` design decision 6).

use super::ClusterRegistry;
use crate::k8s::cluster::connection::ClusterConnection;
use gpui_kit::{App, Context};

impl ClusterRegistry {
    /// Tries `context_name`'s failed connection again. Returns whether an
    /// attempt started: not for a context with no open session, nor for one
    /// that isn't failed.
    pub fn reconnect(cx: &mut App, context_name: &str) -> bool {
        Self::reconnect_with(cx, context_name, |connection, cx| connection.redial(cx))
    }

    /// [`Self::reconnect`] with the redial injected - the seam tests use
    /// instead of a real kubeconfig and cluster. A redial may acquire a
    /// different tunnel forward (a manual tunnel's is let go on failure), so
    /// pause/resume is driven off whichever forward the connection holds now.
    pub(super) fn reconnect_with(
        cx: &mut App,
        context_name: &str,
        redial: impl FnOnce(&mut ClusterConnection, &mut Context<ClusterConnection>) -> bool,
    ) -> bool {
        let Some(connection) = Self::existing_connection(cx, context_name) else {
            return false;
        };
        if !connection.update(cx, redial) {
            return false;
        }
        let health = Self::drive_health(cx, context_name, &connection);
        if let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) {
            session._health = health;
        }
        true
    }
}

#[cfg(test)]
mod tests;
