//! Subscribing panels to a context's shared Pods watch: starting and tearing it down on
//! the 0-to-1/1-to-0 transition, and restarting it after a 401 credential refresh.

use super::*;

impl ClusterRegistry {
    /// Subscribes a panel to `context_name`'s shared Pods watch, starting it on the
    /// 0-to-1 transition. Returns the shared table to render from.
    pub fn subscribe_pods(cx: &mut App, context_name: &str, client: Client) -> Entity<PodsTable> {
        Self::ensure_init(cx, context_name);
        let table = cx.global::<Self>().sessions[context_name].pods.clone();
        let should_start = cx
            .global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap()
            .watchers
            .subscribe("pods");
        if should_start {
            cx.global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .pods_client = Some(client.clone());
            let watch = Self::start_pods_watch(cx, context_name, client);
            cx.global_mut::<Self>()
                .sessions
                .get_mut(context_name)
                .unwrap()
                .pods_watch = Some(watch);
        }
        table
    }

    /// Starts the Pods watch against `client` for `context_name`, routing a 401 through
    /// [`Self::handle_pods_unauthorized`] - the one place that knows how to pause and
    /// attempt a credential refresh. Shared by the initial subscribe and every restart
    /// (health resume, successful credential refresh) so both go through one path.
    pub(super) fn start_pods_watch(
        cx: &mut App,
        context_name: &str,
        client: Client,
    ) -> gpui_kit::Task<()> {
        let table = cx.global::<Self>().sessions[context_name].pods.clone();
        let context_name = context_name.to_string();
        watch_all_namespaces(
            client,
            table,
            move |cx| Self::handle_pods_unauthorized(cx, &context_name),
            cx,
        )
    }

    /// Section 7.3: a watch stream reported a 401 for `context_name`. Pauses through the
    /// same path a forward flap uses (section 7.2), then attempts one credential refresh
    /// by re-resolving that context's config and probing again - the same sequence the
    /// original connect used, reused via [`crate::k8s::cluster::connection::connect_and_probe`] rather
    /// than duplicated. A successful probe resumes through that same path with the
    /// refreshed client; a failed one leaves the watch paused - closing every subscribed
    /// panel is still what releases it (`unsubscribe_pods`, already unconditional on
    /// health/auth state).
    fn handle_pods_unauthorized(cx: &mut App, context_name: &str) {
        if !cx.global::<Self>().sessions.contains_key(context_name) {
            return;
        }
        Self::apply_health_transition(
            cx,
            context_name,
            HealthTransition::Pause(PauseReason::CredentialRefresh),
        );

        let connection = cx.global::<Self>().sessions[context_name]
            .connection
            .clone();
        let forward_wait = connection.read(cx).forward_wait();
        let context_for_resolve = context_name.to_string();
        let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
            let config_result =
                crate::k8s::cluster::connection::resolve_config(Some(context_for_resolve.as_str()))
                    .await;
            crate::k8s::cluster::connection::connect_and_probe(config_result, forward_wait, tx)
                .await;
        });
        let context_name = context_name.to_string();
        cx.spawn(async move |cx| {
            crate::runtime::drain(rx, move |state| {
                if let ConnectionState::Connected(client) = state {
                    let context_name = context_name.clone();
                    cx.update(move |cx| Self::refresh_pods_client(cx, &context_name, client));
                }
            })
            .await;
        })
        .detach();
    }

    /// A credential refresh succeeded for `context_name`: record the new client and
    /// resume through the same pause/resume path section 7.2 established, restarting
    /// the watch from it.
    fn refresh_pods_client(cx: &mut App, context_name: &str, client: Client) {
        if !cx.global::<Self>().sessions.contains_key(context_name) {
            return;
        }
        cx.global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .unwrap()
            .pods_client = Some(client);
        Self::apply_health_transition(cx, context_name, HealthTransition::Resume);
    }

    /// Unsubscribes a panel from `context_name`'s shared Pods watch, tearing it down on
    /// the 1-to-0 transition.
    pub fn unsubscribe_pods(cx: &mut App, context_name: &str) {
        if !cx.has_global::<Self>() {
            return;
        }
        let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
            return;
        };
        if session.watchers.unsubscribe(&"pods") {
            session.pods_watch = None;
        }
    }
}

#[cfg(test)]
mod tests;
