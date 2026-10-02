//! Pausing and resuming a context's watches in response to a health edge, and reading
//! that context's combined health for the status bar.

use super::*;

impl ClusterRegistry {
    /// Applies one health edge to every watch on `context_name` - the Pods watch and each
    /// kind's. Pausing drops each watch task (stops consuming without unsubscribing);
    /// resuming restarts each from the last client used, and only while a panel is still
    /// subscribed - a health edge arriving after every panel unsubscribed has nothing to
    /// pause or resume.
    ///
    /// `pub(crate)` rather than private: this is the same real pause/resume entry point
    /// production code drives from a tunnel flap or a 401 (see [`Self::ensure_init`] and
    /// [`Self::handle_pods_unauthorized`]), and `connection-status-bar`'s integration
    /// tests (`ui/status_bar.rs`, `k8s/resource/pods.rs`) need to drive it directly to
    /// prove the status bar and the Pods panel each react correctly to a real pause -
    /// the same reasoning this module's own tests already use it for.
    pub(crate) fn apply_health_transition(
        cx: &mut App,
        context_name: &str,
        edge: HealthTransition,
    ) {
        if !cx.global::<Self>().sessions.contains_key(context_name) {
            return;
        }
        match edge {
            HealthTransition::Pause(reason) => {
                let session = cx
                    .global_mut::<Self>()
                    .sessions
                    .get_mut(context_name)
                    .unwrap();
                if session.watchers.pause(&WatchKey::Pods, reason) {
                    session.pods_watch = None;
                }
                for (kind, watch) in &mut session.kinds {
                    if session
                        .watchers
                        .pause(&WatchKey::Kind(kind.clone()), reason)
                    {
                        watch.task = None;
                    }
                }
                if let Some(events) = &mut session.events
                    && session.watchers.pause(&WatchKey::Events, reason)
                {
                    events.task = None;
                }
            }
            HealthTransition::Resume => {
                let session = cx
                    .global_mut::<Self>()
                    .sessions
                    .get_mut(context_name)
                    .unwrap();
                let should_restart = session.watchers.resume(&WatchKey::Pods)
                    && session.watchers.refcount(&WatchKey::Pods) > 0;
                let restart_events = session.events.is_some()
                    && session.watchers.resume(&WatchKey::Events)
                    && session.watchers.refcount(&WatchKey::Events) > 0;
                let client = session.client.clone();
                let kinds: Vec<DiscoveredKind> = session
                    .kinds
                    .keys()
                    .filter(|kind| {
                        let key = WatchKey::Kind((*kind).clone());
                        session.watchers.resume(&key) && session.watchers.refcount(&key) > 0
                    })
                    .cloned()
                    .collect();
                let Some(client) = client else {
                    return;
                };
                if should_restart {
                    let watch = Self::start_pods_watch(cx, context_name, client.clone());
                    cx.global_mut::<Self>()
                        .sessions
                        .get_mut(context_name)
                        .unwrap()
                        .pods_watch = Some(watch);
                }
                if restart_events {
                    let task = Self::start_events_watch(cx, context_name, client.clone());
                    if let Some(events) = &mut cx
                        .global_mut::<Self>()
                        .sessions
                        .get_mut(context_name)
                        .unwrap()
                        .events
                    {
                        events.task = Some(task);
                    }
                }
                for kind in kinds {
                    let task = Self::start_kind_watch(cx, context_name, client.clone(), &kind);
                    if let Some(watch) = cx
                        .global_mut::<Self>()
                        .sessions
                        .get_mut(context_name)
                        .unwrap()
                        .kinds
                        .get_mut(&kind)
                    {
                        watch.task = Some(task);
                    }
                }
            }
        }
    }

    /// `connection-status-bar` design.md decision 1: `context_name`'s health for the
    /// window status bar, combining its connection state with its watch registry's first
    /// paused key at precedence failed > paused > waiting for tunnel > connected. A
    /// context with no session yet (no panel has ever subscribed to it) reads as
    /// `Connected` - the bar only ever asks about contexts a window is actually using,
    /// which always has a session by the time it asks.
    pub fn health(cx: &App, context_name: &str) -> ContextHealth {
        let Some(session) = cx
            .try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
        else {
            return ContextHealth::Connected;
        };
        let connection = session.connection.read(cx);
        if let ConnectionState::Failed(reason) = &connection.state {
            return ContextHealth::Failed {
                reason: reason.clone(),
                since: connection.since(),
            };
        }
        if let Some((reason, since)) = session.watchers.first_paused() {
            return ContextHealth::Paused { reason, since };
        }
        if let ConnectionState::WaitingForTunnel = &connection.state {
            return ContextHealth::WaitingForTunnel {
                since: connection.since(),
            };
        }
        ContextHealth::Connected
    }
}

#[cfg(test)]
mod tests;
