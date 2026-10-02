//! Subscribing consumers to a context's shared Event watch (`events-browser` D1):
//! the same refcounted start/stop as the Pods and kind watches, keyed by
//! `WatchKey::Events`, with 401s going through the session's one
//! credential-refresh path.

use super::registry::EventsWatch;
use super::*;

impl ClusterRegistry {
    /// Subscribes to `context_name`'s shared Event watch, starting it on the
    /// 0-to-1 transition. Returns the shared table to render from.
    pub fn subscribe_events(
        cx: &mut App,
        context_name: &str,
        client: Client,
    ) -> Entity<EventsTable> {
        Self::ensure_init(cx, context_name);
        if cx.global::<Self>().sessions[context_name].events.is_none() {
            let table = cx.new(|_| EventsTable::default());
            cx.global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .events = Some(EventsWatch { table, task: None });
        }
        let session = cx
            .global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap();
        let table = session
            .events
            .as_ref()
            .map(|events| events.table.clone())
            .expect("created above");
        if session.watchers.subscribe(WatchKey::Events) {
            session.client = Some(client.clone());
            let task = Self::start_events_watch(cx, context_name, client);
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
        table
    }

    /// Starts the Event watch on `context_name` against `client`. Shared by the
    /// initial subscribe and every restart (health resume, a refreshed
    /// credential).
    pub(super) fn start_events_watch(
        cx: &mut App,
        context_name: &str,
        client: Client,
    ) -> gpui_kit::Task<()> {
        let table = cx.global::<Self>().sessions[context_name]
            .events
            .as_ref()
            .map(|events| events.table.clone())
            .expect("the Event watch has a table while it runs");
        let context_name = context_name.to_string();
        watch_events(
            client,
            table,
            move |cx| Self::handle_unauthorized(cx, &context_name),
            cx,
        )
    }

    /// Unsubscribes from `context_name`'s shared Event watch. The last one out
    /// stops the watch and drops its table.
    pub fn unsubscribe_events(cx: &mut App, context_name: &str) {
        if !cx.has_global::<Self>() {
            return;
        }
        let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
            return;
        };
        if session.watchers.unsubscribe(&WatchKey::Events) {
            session.events = None;
        }
    }

    /// Test-only: whether `context_name` is consuming its Event watch right now.
    #[cfg(test)]
    pub(crate) fn events_watch_running(cx: &App, context_name: &str) -> bool {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .and_then(|session| session.events.as_ref())
            .is_some_and(|events| events.task.is_some())
    }
}

#[cfg(test)]
mod tests;
