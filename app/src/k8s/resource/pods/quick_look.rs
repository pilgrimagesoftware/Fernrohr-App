//! The quick look (`pod-quick-look`): a popover over the Pods table showing the
//! selected pod at a glance, opened with Space and closed with Space or Escape.
//!
//! The popover follows its pod in the panel's shared Pods table (D1), so it is
//! live and costs no API call. A pod being deleted reads as Terminating, and
//! once gone the popover keeps its last state under the detail panels' "deleted
//! at" banner (`ui::detail::lifecycle`) rather than closing. Its latest Warning comes from a per-pod
//! event watch it starts when opened and restarts, debounced, as Up/Down move it
//! to another pod (D2); dropping the popover drops the watch.
//!
//! It never takes focus (D3): the table keeps it, so Up/Down still move the
//! selection the popover follows. The panel owns opening, closing and the keys
//! - see the `impl PodsPanel` below; [`view`] draws.

use super::*;
use crate::k8s::cluster::discovery_registry::{DiscoveredKinds, DiscoveryRegistry};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::events::{self, InvolvedObject};
use crate::k8s::resource::events_browser::EventsTable;
use crate::ui::detail::lifecycle::Lifecycle;

mod view;

/// The popover: which pod it is over, and that pod's event watch.
pub(in crate::k8s::resource) struct QuickLookPopover {
    /// The panel's shared Pods table the pod is read from.
    pods: Entity<PodsTable>,
    pub(super) target: PodSelection,
    /// The context's discovered kinds, which decide whether the owner is a link.
    discovery: Entity<DiscoveredKinds>,
    client: Option<kube::Client>,
    /// The pod as last listed - kept once it is gone, shown as stale.
    last: Option<Box<Pod>>,
    /// When the pod was seen gone from the table.
    deleted_at: Option<Timestamp>,
    /// Re-renders every second while the pod is Terminating, so its grace
    /// period counts down.
    countdown: Option<Task<()>>,
    events: Option<QuickLookEvents>,
    /// The pending restart of the event watch after a move to another pod.
    retarget: Option<Task<()>>,
}

/// The event watch on the pod the popover shows - dropping it ends the watch.
struct QuickLookEvents {
    table: Entity<EventsTable>,
    _task: Task<()>,
}

/// The latest Warning event about the pod.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Warning {
    pub(super) reason: String,
    pub(super) message: String,
    pub(super) last_seen: Option<Timestamp>,
}

impl QuickLookPopover {
    pub(super) fn new(
        pods: Entity<PodsTable>,
        target: PodSelection,
        client: Option<kube::Client>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&pods, |this, _, cx| this.follow(cx)).detach();
        let discovery = DiscoveryRegistry::kinds(cx, &target.context_name);
        cx.observe(&discovery, |_, _, cx| cx.notify()).detach();
        let mut this = Self {
            pods,
            target,
            discovery,
            client,
            last: None,
            deleted_at: None,
            countdown: None,
            events: None,
            retarget: None,
        };
        this.follow(cx);
        this.watch_events(cx);
        this
    }

    /// Moves the popover to `target`. The fields follow at once; the event
    /// watch restarts once the selection has rested, so scanning down a list
    /// doesn't start and drop a watch per row.
    pub(super) fn retarget(&mut self, target: PodSelection, cx: &mut Context<Self>) {
        if self.target == target {
            return;
        }
        self.target = target;
        self.last = None;
        self.deleted_at = None;
        self.events = None;
        self.follow(cx);
        self.retarget = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(crate::consts::QUICK_LOOK_EVENTS_DEBOUNCE)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.retarget = None;
                this.watch_events(cx);
            });
        }));
        cx.notify();
    }

    /// Starts watching the target pod's events, if there is a client to watch with.
    fn watch_events(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let uid = self
            .pod()
            .and_then(|pod| pod.metadata.uid.clone())
            .filter(|uid| !uid.is_empty());
        let table = cx.new(|_| EventsTable::default());
        cx.observe(&table, |_, _, cx| cx.notify()).detach();
        let context_name = self.target.context_name.clone();
        let task = events::watch(
            client,
            &InvolvedObject {
                kind: "Pod",
                namespace: Some(&self.target.namespace),
                name: &self.target.name,
                uid: uid.as_deref(),
            },
            table.clone(),
            move |cx| ClusterRegistry::handle_unauthorized(cx, &context_name),
            cx,
        );
        self.events = Some(QuickLookEvents { table, _task: task });
    }

    /// Catches up with the shared table: the pod's latest write, or the moment
    /// it went.
    fn follow(&mut self, cx: &mut Context<Self>) {
        let listed = self
            .pods
            .read(cx)
            .find(&self.target.namespace, &self.target.name, None);
        match listed {
            Some(pod) => {
                let current = self.last.as_ref().is_some_and(|last| {
                    last.metadata.uid == pod.metadata.uid
                        && last.metadata.resource_version == pod.metadata.resource_version
                });
                if !current {
                    self.last = Some(Box::new(pod.clone()));
                    self.deleted_at = None;
                }
            }
            None if self.last.is_some() && self.deleted_at.is_none() => {
                self.deleted_at = Some(Timestamp::now());
            }
            None => {}
        }
        self.count_down_while_terminating(cx);
        cx.notify();
    }

    fn count_down_while_terminating(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.lifecycle(), Some(Lifecycle::Terminating { .. })) {
            self.countdown = None;
            return;
        }
        if self.countdown.is_none() {
            self.countdown = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(crate::consts::TERMINATING_COUNTDOWN_TICK)
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        return;
                    }
                }
            }));
        }
    }

    /// The pod as last listed - still there once it is gone, then stale.
    pub(super) fn pod(&self) -> Option<&Pod> {
        self.last.as_deref()
    }

    /// The banner above the pod, in the detail panels' words: deleted, or
    /// Terminating while it has a deletion timestamp.
    pub(super) fn lifecycle(&self) -> Option<Lifecycle> {
        if let Some(at) = self.deleted_at {
            return Some(Lifecycle::Deleted { at });
        }
        self.pod()?
            .metadata
            .deletion_timestamp
            .as_ref()
            .map(|deadline| Lifecycle::Terminating {
                deadline: deadline.0,
            })
    }

    /// The most recently seen Warning event about the pod, once its events are in.
    pub(super) fn latest_warning(&self, cx: &App) -> Option<Warning> {
        let table = self.events.as_ref()?.table.read(cx);
        table
            .rows()
            .iter()
            .filter(|row| row.type_.as_deref() == Some("Warning"))
            .max_by_key(|row| row.last_seen)
            .map(|row| Warning {
                reason: row.reason.clone(),
                message: row.message.clone(),
                last_seen: row.last_seen,
            })
    }

    /// The event table the popover watches, for tests to check it is dropped.
    #[cfg(test)]
    pub(super) fn events_table(&self) -> Option<WeakEntity<EventsTable>> {
        self.events.as_ref().map(|events| events.table.downgrade())
    }
}

impl PodsPanel {
    /// `QuickLook` (Space): opens the popover over the selected pod, or closes
    /// it if open.
    pub(super) fn on_action_quick_look(
        &mut self,
        _: &QuickLook,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quick_look.is_some() {
            self.close_quick_look(cx);
            return;
        }
        let Some(target) = self.table_selection(cx) else {
            return;
        };
        let client = match &self.connection.read(cx).state {
            crate::k8s::cluster::connection::ConnectionState::Connected(client) => {
                Some(client.clone())
            }
            _ => None,
        };
        let pods = self.table.clone();
        let popover = cx.new(|cx| QuickLookPopover::new(pods, target, client, cx));
        self.quick_look = Some(popover.clone());
        self.show_quick_look_in_table(Some(popover), cx);
        cx.notify();
    }

    pub(super) fn on_action_close_quick_look(
        &mut self,
        _: &CloseQuickLook,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_quick_look(cx);
    }

    /// Escape reaches the table first, whose `Cancel` would clear the selection
    /// the popover is over; while a quick look is open it closes that instead.
    pub(super) fn capture_cancel(
        &mut self,
        _: &gpui_kit::base::actions::Cancel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quick_look.is_some() {
            self.close_quick_look(cx);
            cx.stop_propagation();
        }
    }

    /// `OpenQuickLookDetails` (Enter, or the popover's button): opens the pod's
    /// detail panel - focusing it if open - and closes the popover.
    pub(super) fn on_action_open_quick_look_details(
        &mut self,
        _: &OpenQuickLookDetails,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popover) = self.quick_look.clone() else {
            return;
        };
        let target = popover.read(cx).target.clone();
        self.close_quick_look(cx);
        cx.set_global(SelectedPod(Some(target)));
        window.dispatch_action(Box::new(crate::ui::nav::ShowPodDetail), cx);
    }

    /// Follows the table's selection with an open popover.
    pub(super) fn retarget_quick_look(&mut self, target: &PodSelection, cx: &mut Context<Self>) {
        if let Some(popover) = &self.quick_look {
            popover.update(cx, |popover, cx| popover.retarget(target.clone(), cx));
        }
    }

    pub(super) fn close_quick_look(&mut self, cx: &mut Context<Self>) {
        if self.quick_look.take().is_some() {
            self.show_quick_look_in_table(None, cx);
            cx.notify();
        }
    }

    /// Hands the popover to the table, which draws it beside the selected row.
    fn show_quick_look_in_table(
        &mut self,
        popover: Option<Entity<QuickLookPopover>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(table) = &self.pod_table {
            table.update(cx, |table, cx| {
                table.delegate_mut().set_quick_look(popover);
                cx.notify();
            });
        }
    }

    /// The pod this panel's table has selected, by identity.
    pub(super) fn table_selection(&self, cx: &App) -> Option<PodSelection> {
        let table = self.pod_table.as_ref()?.read(cx);
        let row = table.selected_row()?;
        table
            .delegate()
            .rows()
            .get(row)
            .map(|row| row.selection.clone())
    }

    /// The open quick look, for tests.
    #[cfg(test)]
    pub(super) fn quick_look(&self) -> Option<&Entity<QuickLookPopover>> {
        self.quick_look.as_ref()
    }
}

#[cfg(test)]
mod tests;
