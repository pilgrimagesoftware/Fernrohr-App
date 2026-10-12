//! Keeping a label-following panel's streams in step with its selector and
//! the pods it matches: one stream per started container, tagged, started as a
//! matching container appears or restarts and dropped as it goes away.

use super::*;
use crate::consts::{LABEL_LOGS_MAX_STREAMS, LABEL_LOGS_TAIL_LINES};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;

/// What a typed panel says when Enter is pressed on a blank selector, or one
/// that would pick every pod.
pub(in crate::util::logs) const NEEDS_A_REQUIREMENT: &str =
    "Enter at least one label requirement, such as app=web.";

impl LogsPanel {
    /// The namespaces a label panel's selector applies in: a workload's own,
    /// or a typed panel's scope (every namespace, when it has none).
    fn label_namespaces(&self) -> Vec<String> {
        match self.labels.as_ref().map(|following| &following.source) {
            Some(LabelLogs::Workload(workload)) => vec![workload.namespace.clone()],
            _ => self.scope.namespaces.clone(),
        }
    }

    /// Starts the streams the selector now wants and drops those it no longer
    /// does - on connecting, on a new selector, and on every change to the
    /// context's pods. A no-op until connected and given a selector.
    pub(in crate::util::logs) fn sync_labels(&mut self, cx: &mut Context<Self>) {
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        let Some(following) = &self.labels else {
            return;
        };
        let Some((_, selector)) = following.selector.clone() else {
            return;
        };
        let filter = following.container.clone();
        let table = match &following.pods {
            Some(table) => table.clone(),
            None => self.subscribe_pods(client.clone(), cx),
        };
        let namespaces = self.label_namespaces();
        let pods = table.read(cx).pods();
        let (wanted, matched) = wanted(pods, &selector, &namespaces, filter.as_deref());
        let containers = container_names(pods, &selector, &namespaces);
        let view = self.view.clone();
        let context_name = self.scope.context_name.clone();
        let Some(following) = &mut self.labels else {
            return;
        };
        following.matched = matched;
        following.containers = containers;
        let wanted: Vec<Wanted> = wanted.into_iter().take(LABEL_LOGS_MAX_STREAMS).collect();
        let now = jiff::Timestamp::now();
        let Following {
            streams, stopped, ..
        } = following;
        streams.retain(|key, _| {
            let keep = wanted.iter().any(|want| &want.key == key);
            if !keep {
                stopped.insert(key.clone(), now);
            }
            keep
        });
        for Wanted {
            key,
            source,
            follow,
        } in wanted
        {
            if streams.contains_key(&key) {
                continue;
            }
            let target = label_target(&key, &context_name, follow, stopped.remove(&key));
            #[cfg(test)]
            let recorded = target.clone();
            let client = client.clone();
            let task =
                start_tagged_stream(view.clone(), cx, LOG_CHANNEL_CAPACITY, source, move |tx| {
                    produce_container_logs(client, target, tx)
                });
            streams.insert(
                key,
                Streaming {
                    #[cfg(test)]
                    target: recorded,
                    _task: task,
                },
            );
        }
        cx.notify();
    }

    /// Test-only: each stream's request, by `pod/container`.
    #[cfg(test)]
    pub(crate) fn test_targets(&self) -> Vec<(String, LogTarget)> {
        let mut targets: Vec<(String, LogTarget)> = self
            .labels
            .iter()
            .flat_map(|following| following.streams.iter())
            .map(|(key, streaming)| {
                (
                    format!("{}/{}", key.pod, key.container),
                    streaming.target.clone(),
                )
            })
            .collect();
        targets.sort_by(|a, b| a.0.cmp(&b.0));
        targets
    }

    /// Subscribes the panel to its context's shared Pods watch, released with
    /// the panel, and re-syncs its streams on every change to it.
    fn subscribe_pods(
        &mut self,
        client: kube::Client,
        cx: &mut Context<Self>,
    ) -> Entity<crate::k8s::resource::pods::PodsTable> {
        let context_name = self.scope.context_name.clone();
        let table = ClusterRegistry::subscribe_pods(cx, &context_name, client);
        cx.on_release(move |_: &mut Self, cx| ClusterRegistry::unsubscribe_pods(cx, &context_name))
            .detach();
        cx.observe(&table, |this: &mut Self, _, cx| this.sync_labels(cx))
            .detach();
        if let Some(following) = &mut self.labels {
            following.pods = Some(table.clone());
        }
        table
    }

    /// A typed panel's Enter: streams `text`'s selector in place of the last
    /// one, onto a fresh view - or, when it doesn't parse or selects every
    /// pod, says why and keeps streaming the last one.
    pub(in crate::util::logs) fn apply_selector_text(
        &mut self,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(following) = &mut self.labels else {
            return;
        };
        let selector = match label_selector::parse(text) {
            Ok(selector) if selector.selects_all() => {
                following.error = Some(NEEDS_A_REQUIREMENT.to_string());
                cx.notify();
                return;
            }
            Ok(selector) => selector,
            Err(error) => {
                following.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        following.error = None;
        let canonical = selector.to_string();
        if following.selector.as_ref().map(|(text, _)| text) == Some(&canonical) {
            cx.notify();
            return;
        }
        following.selector = Some((canonical, selector));
        following.streams.clear();
        following.stopped.clear();
        following.matched = Matched::default();
        let view = cx.new(|_| LogsView::new(Vec::new()));
        self.follow(&view, cx);
        self.view = view;
        self.sync_labels(cx);
        cx.notify();
    }

    /// Streams only the containers named `container` from each matching pod,
    /// or - `None` - all of them. Streams the filter excludes are dropped, not
    /// hidden, so they don't count toward the cap.
    pub(in crate::util::logs) fn set_container_filter(
        &mut self,
        container: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(following) = &mut self.labels else {
            return;
        };
        if following.container == container {
            return;
        }
        following.container = container;
        self.sync_labels(cx);
        cx.notify();
    }

    /// `NextLabelContainer`: the filter's next choice - all containers, then
    /// each name the matched pods have, then all again.
    pub(in crate::util::logs) fn next_container_filter(&mut self, cx: &mut Context<Self>) {
        let Some(following) = &self.labels else {
            return;
        };
        let choices: Vec<Option<String>> = std::iter::once(None)
            .chain(following.containers.iter().cloned().map(Some))
            .collect();
        let at = choices
            .iter()
            .position(|choice| *choice == following.container)
            .unwrap_or(0);
        let next = choices[(at + 1) % choices.len()].clone();
        self.set_container_filter(next, cx);
    }

    /// Test-only: the selector streamed now, as canonical text.
    #[cfg(test)]
    pub(crate) fn test_selector(&self) -> Option<String> {
        Some(self.labels.as_ref()?.selector.as_ref()?.0.clone())
    }

    /// Test-only: why the selector last typed wasn't applied.
    #[cfg(test)]
    pub(crate) fn test_selector_error(&self) -> Option<String> {
        self.labels.as_ref()?.error.clone()
    }

    /// Test-only: how many pods and containers the selector matched.
    #[cfg(test)]
    pub(crate) fn test_matched(&self) -> (usize, usize) {
        self.labels.as_ref().map_or((0, 0), |following| {
            (following.matched.pods, following.matched.containers)
        })
    }

    /// Test-only: the containers streaming now, as `pod/container#restarts@uid`.
    #[cfg(test)]
    pub(crate) fn test_streams(&self) -> Vec<String> {
        let mut streams: Vec<String> = self
            .labels
            .iter()
            .flat_map(|following| following.streams.keys())
            .map(|key| {
                format!(
                    "{}/{}#{}@{}",
                    key.pod, key.container, key.restart_count, key.uid
                )
            })
            .collect();
        streams.sort();
        streams
    }
}

/// The request for `key`'s log: its last [`LABEL_LOGS_TAIL_LINES`] lines, then
/// what follows - or, for a stream the view showed before and dropped at
/// `stopped`, only what was written since, so lines already shown aren't
/// added again. `stopped` is rounded up to the next whole second, as the API
/// reads it: lines in the same second as the drop were shown already, or
/// are lost rather than repeated.
pub(super) fn label_target(
    key: &StreamKey,
    context_name: &str,
    follow: bool,
    stopped: Option<jiff::Timestamp>,
) -> LogTarget {
    let since = stopped.map(|stopped| {
        stopped
            .round(
                jiff::TimestampRound::new()
                    .smallest(jiff::Unit::Second)
                    .mode(jiff::RoundMode::Ceil),
            )
            .unwrap_or(stopped)
    });
    LogTarget {
        namespace: key.namespace.clone(),
        pod_name: key.pod.clone(),
        container: key.container.clone(),
        context_name: context_name.to_string(),
        previous: false,
        tail_lines: since.is_none().then_some(LABEL_LOGS_TAIL_LINES),
        follow,
        since,
    }
}
