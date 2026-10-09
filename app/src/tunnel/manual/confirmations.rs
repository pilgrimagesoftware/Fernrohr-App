//! [`ManualConfirmations`]: every manual tunnel waiting for the user's Proceed or
//! Cancel (design.md D4), as one GPUI entity the prompt surfaces observe.
//!
//! A [`ManualTransport`](super::ManualTransport) runs on the tokio runtime, so it
//! publishes and withdraws its entry by sending a [`ConfirmationEvent`]; a task on
//! the main thread applies each to the entity and notifies its observers. Resolving
//! an entry ([`ManualConfirmations::resolve`]) answers its transport and removes it.
//! There is at most one entry per tunnel: every context bound to a manual tunnel
//! shares its forward, and so its confirmation.
//!
//! Which contexts an entry is holding up is recorded as they acquire the tunnel
//! ([`ManualConfirmations::note_waiting`]), and forgotten when the entry is resolved
//! or withdrawn.

use gpui_kit::{App, AppContext as _, Entity, Global};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};

/// The user's answer to a pending confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The network path is up: let the waiting connections continue.
    Proceed,
    /// Fail the waiting connections; never connect directly instead.
    Cancel,
}

/// What a transport asks the user about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingRequest {
    pub(crate) id: u64,
    pub(crate) tunnel_id: String,
    pub(crate) name: String,
    pub(crate) message: Option<String>,
}

/// What a transport tells the main thread.
pub(crate) enum ConfirmationEvent {
    /// A confirmation is waiting; answer it on `resolve`.
    Pending {
        request: PendingRequest,
        resolve: oneshot::Sender<Decision>,
    },
    /// The confirmation `id` is no longer wanted: its forward went away.
    Withdrawn { id: u64 },
    /// `tunnel_id` was confirmed without asking (its API server answered), so no
    /// one is waiting on it any more.
    Settled { tunnel_id: String },
}

/// One manual tunnel waiting on the user.
pub struct PendingConfirmation {
    id: u64,
    /// The tunnel's id in `tunnels.toml`.
    pub tunnel_id: String,
    /// The tunnel's display name.
    pub name: String,
    /// The tunnel's instruction, if it has one.
    pub message: Option<String>,
    /// The contexts waiting on it, in name order.
    pub contexts: Vec<String>,
    /// When it started waiting.
    pub since: Instant,
    resolve: Option<oneshot::Sender<Decision>>,
}

/// Every pending manual-tunnel confirmation in the app.
#[derive(Default)]
pub struct ManualConfirmations {
    pending: Vec<PendingConfirmation>,
    /// Contexts that acquired each tunnel while it was unconfirmed.
    waiting: BTreeMap<String, BTreeSet<String>>,
}

/// The entity, and the sender transports publish through.
struct Confirmations {
    entity: Entity<ManualConfirmations>,
    events: mpsc::UnboundedSender<ConfirmationEvent>,
}

impl Global for Confirmations {}

impl ManualConfirmations {
    /// The app's confirmations, if any manual tunnel has ever been acquired - for a
    /// surface to read and observe.
    pub fn entity(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<Confirmations>()
            .map(|confirmations| confirmations.entity.clone())
    }

    /// Starts the app's confirmations at launch, so every surface can observe them
    /// from the start rather than only once a manual tunnel is first acquired.
    pub fn init(cx: &mut App) {
        let _ = Self::events(cx);
    }

    /// The sender a transport publishes through, starting the entity and its
    /// main-thread task on first use.
    pub(crate) fn events(cx: &mut App) -> mpsc::UnboundedSender<ConfirmationEvent> {
        if let Some(confirmations) = cx.try_global::<Confirmations>() {
            return confirmations.events.clone();
        }
        let entity = cx.new(|_| Self::default());
        let (events, mut rx) = mpsc::unbounded_channel();
        // Weak: the task lives as long as the channel, which the global keeps open,
        // and must not keep the entity alive past the app (or a test) on its own.
        let applied = entity.downgrade();
        cx.spawn(async move |cx| {
            while let Some(event) = rx.recv().await {
                let applied = applied.update(cx, |this, cx| {
                    this.apply(event);
                    cx.notify();
                });
                if applied.is_err() {
                    return;
                }
            }
        })
        .detach();
        cx.set_global(Confirmations {
            entity,
            events: events.clone(),
        });
        events
    }

    /// Every pending confirmation, oldest first.
    pub fn pending(&self) -> &[PendingConfirmation] {
        &self.pending
    }

    /// The pending confirmation for `tunnel_id`, if it is waiting.
    pub fn pending_for(&self, tunnel_id: &str) -> Option<&PendingConfirmation> {
        self.pending
            .iter()
            .find(|entry| entry.tunnel_id == tunnel_id)
    }

    /// The pending confirmation `context` is waiting on, if any.
    pub fn for_context(&self, context: &str) -> Option<&PendingConfirmation> {
        self.pending
            .iter()
            .find(|entry| entry.contexts.iter().any(|name| name == context))
    }

    /// Answers `tunnel_id`'s pending confirmation with `decision`, releasing (or
    /// failing) every connection waiting on it. `false` when nothing was waiting.
    pub fn resolve(cx: &mut App, tunnel_id: &str, decision: Decision) -> bool {
        let Some(entity) = Self::entity(cx) else {
            return false;
        };
        entity.update(cx, |this, cx| {
            let Some(index) = this
                .pending
                .iter()
                .position(|entry| entry.tunnel_id == tunnel_id)
            else {
                return false;
            };
            let mut entry = this.pending.remove(index);
            this.waiting.remove(tunnel_id);
            if let Some(resolve) = entry.resolve.take() {
                let _ = resolve.send(decision);
            }
            cx.notify();
            true
        })
    }

    /// Records that `context` is waiting on `tunnel_id`, joining its pending entry
    /// if one is already up.
    pub(crate) fn note_waiting(cx: &mut App, tunnel_id: &str, context: &str) {
        let Some(entity) = Self::entity(cx) else {
            return;
        };
        entity.update(cx, |this, cx| {
            this.waiting
                .entry(tunnel_id.to_string())
                .or_default()
                .insert(context.to_string());
            if let Some(entry) = this
                .pending
                .iter_mut()
                .find(|entry| entry.tunnel_id == tunnel_id)
                && !entry.contexts.iter().any(|name| name == context)
            {
                entry.contexts.push(context.to_string());
                entry.contexts.sort();
            }
            cx.notify();
        });
    }

    /// Puts up a pending confirmation for `tunnel_id` with `contexts` waiting, as a
    /// transport would, and returns where its answer arrives. Test-only.
    #[cfg(test)]
    pub(crate) fn insert_test_pending(
        cx: &mut App,
        tunnel_id: &str,
        name: &str,
        message: Option<&str>,
        contexts: &[&str],
    ) -> oneshot::Receiver<Decision> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1 << 32);
        Self::init(cx);
        let (resolve, answer) = oneshot::channel();
        let entity = Self::entity(cx).expect("just started");
        entity.update(cx, |this, cx| {
            for context in contexts {
                this.waiting
                    .entry(tunnel_id.to_string())
                    .or_default()
                    .insert(context.to_string());
            }
            this.apply(ConfirmationEvent::Pending {
                request: PendingRequest {
                    id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    tunnel_id: tunnel_id.to_string(),
                    name: name.to_string(),
                    message: message.map(str::to_string),
                },
                resolve,
            });
            cx.notify();
        });
        answer
    }

    fn apply(&mut self, event: ConfirmationEvent) {
        match event {
            ConfirmationEvent::Pending { request, resolve } => {
                let contexts = self
                    .waiting
                    .get(&request.tunnel_id)
                    .map(|contexts| contexts.iter().cloned().collect())
                    .unwrap_or_default();
                self.pending.push(PendingConfirmation {
                    id: request.id,
                    tunnel_id: request.tunnel_id,
                    name: request.name,
                    message: request.message,
                    contexts,
                    since: Instant::now(),
                    resolve: Some(resolve),
                });
            }
            ConfirmationEvent::Settled { tunnel_id } => {
                if self.pending_for(&tunnel_id).is_none() {
                    self.waiting.remove(&tunnel_id);
                }
            }
            ConfirmationEvent::Withdrawn { id } => {
                if let Some(index) = self.pending.iter().position(|entry| entry.id == id) {
                    let entry = self.pending.remove(index);
                    self.waiting.remove(&entry.tunnel_id);
                }
            }
        }
    }
}
