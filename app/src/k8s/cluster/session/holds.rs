//! Holds on a context's session (`mcp-connections` design decision 2): `hold`,
//! `release`, releasing everything one holder has, and reading who holds a
//! context. A holder is a window or an agent ([`Holder`]); the session, and with
//! it the connection and tunnel forward, lives while any holder remains.

use super::*;

impl ClusterRegistry {
    /// Adds `holder` to `context_name`'s holders, connecting (or reusing) its
    /// session first if none exists yet. Design decision 1: a window's hold on a
    /// context is independent of whether any panel has subscribed to a watch - the
    /// Resource panel can list kinds for a context with no open panels. The first
    /// holder a session ever has is remembered as the one that connected it.
    pub fn hold(cx: &mut App, context_name: &str, holder: impl Into<Holder>) {
        let holder = holder.into();
        Self::ensure_init(cx, context_name);
        let session = cx
            .global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .expect("ensure_init just inserted this session");
        session.connected_by.get_or_insert(holder);
        session.holders.insert(holder);
    }

    /// Removes `holder` from `context_name`'s holders. On the last release, drops
    /// the session entirely - its connection (and so, via `ClusterConnection`'s own
    /// `Drop`, its tunnel forward) and its watches. A no-op if the context has no
    /// session, or `holder` wasn't holding it.
    ///
    /// `window-context-bar` section 3.3's Disconnect calls this directly for the one
    /// context being disconnected; closing a window releases every context it holds
    /// at once, through [`Self::release_window`] instead.
    pub fn release(cx: &mut App, context_name: &str, holder: impl Into<Holder>) {
        let holder = holder.into();
        if !cx.has_global::<Self>() {
            return;
        }
        let should_remove = {
            let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
                return;
            };
            session.holders.remove(&holder);
            session.holders.is_empty()
        };
        if should_remove {
            cx.global_mut::<Self>().sessions.remove(context_name);
        }
    }

    /// Releases every hold `window_id` has, across every context - what closing a
    /// window does (design.md decision 2's "closing a window releases all its
    /// holds"), without the caller needing to know which contexts that window used.
    /// A context an agent also holds stays connected.
    pub fn release_window(cx: &mut App, window_id: WindowId) {
        Self::release_everywhere(cx, Holder::Window(window_id));
    }

    /// Releases every hold `agent` has, across every context - what an agent's
    /// MCP session ending does. A context a window also holds stays connected.
    // UNWIRED(#367): the MCP session layer (section 2) calls this when a session
    // ends; until then only tests release agent holds.
    #[allow(dead_code)]
    pub fn release_agent(cx: &mut App, agent: AgentId) {
        Self::release_everywhere(cx, Holder::Agent(agent));
    }

    /// What `agent`'s MCP session ending does (`mcp-connections` design decision
    /// 3): each of its holds becomes a [`Holder::Lingering`] that keeps the
    /// connection open for `grace`, then is released. No session can claim a
    /// lingering hold, so a restarted client asks for approval again; the grace
    /// period only spares the cluster a redial if the user approves it in time.
    pub fn linger_agent(cx: &mut App, agent: AgentId, grace: std::time::Duration) {
        if !cx.has_global::<Self>() {
            return;
        }
        let lingering: Vec<Holder> = cx
            .global_mut::<Self>()
            .sessions
            .values_mut()
            .filter_map(|session| {
                if !session.holders.remove(&Holder::Agent(agent)) {
                    return None;
                }
                let hold = Holder::lingering();
                session.holders.insert(hold);
                Some(hold)
            })
            .collect();
        if lingering.is_empty() {
            return;
        }
        // Detached on purpose: the timer owns only the ids, and releasing one
        // that something else already dropped (the session's last window
        // disconnecting it, say) is a no-op.
        cx.spawn(async move |cx| {
            cx.background_executor().timer(grace).await;
            cx.update(|cx| {
                for hold in lingering {
                    Self::release_everywhere(cx, hold);
                }
            });
        })
        .detach();
    }

    /// Removes `holder` from every context's holders, dropping each session it
    /// leaves with none.
    fn release_everywhere(cx: &mut App, holder: Holder) {
        if !cx.has_global::<Self>() {
            return;
        }
        let emptied: Vec<String> = cx
            .global_mut::<Self>()
            .sessions
            .iter_mut()
            .filter_map(|(context_name, session)| {
                session.holders.remove(&holder);
                session.holders.is_empty().then(|| context_name.clone())
            })
            .collect();
        for context_name in emptied {
            cx.global_mut::<Self>().sessions.remove(&context_name);
        }
    }

    /// How many *windows* currently hold `context_name` - `0` for a context with
    /// no session. Agents aren't counted: the disconnect confirmation
    /// (`window-context-bar` section 3.3) reads this minus one (itself) for "stays
    /// connected in N other windows", and the close-window warnings read it for
    /// "held by no other window".
    pub fn window_holder_count(cx: &App, context_name: &str) -> usize {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .map(|session| session.holders.iter().filter(|h| h.is_window()).count())
            .unwrap_or(0)
    }

    /// Everyone holding `context_name`, windows first, then agents - empty for a
    /// context with no session.
    // UNWIRED(#367): read by the Connections window and the status bar's agent
    // marker (sections 4 and 5).
    #[allow(dead_code)]
    pub fn holders(cx: &App, context_name: &str) -> Vec<Holder> {
        let mut holders: Vec<Holder> = cx
            .try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .map(|session| session.holders.iter().copied().collect())
            .unwrap_or_default();
        holders.sort_by_key(|holder| !holder.is_window());
        holders
    }

    /// The holder that first held `context_name`'s current session: who connected
    /// it, for the Connections window's "connected by" - `None` for a context with
    /// no session, or one created by a read and never held.
    // UNWIRED(#367): read by the Connections window (section 4).
    #[allow(dead_code)]
    pub fn connected_by(cx: &App, context_name: &str) -> Option<Holder> {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .and_then(|session| session.connected_by)
    }
}

#[cfg(test)]
mod tests;
