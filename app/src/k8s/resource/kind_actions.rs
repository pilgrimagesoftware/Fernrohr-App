//! Actions that only mean something for one kind (`resource-specific-actions`):
//! which a kind and an object are offered ([`KindAction::offered`]), what of an
//! object they read ([`facts`]), and the cluster calls they make ([`calls`]).
//!
//! The calls reuse `resource_actions` where the agent tools already make the
//! same one - Trigger Now, Suspend, Pause - and add Expand, taints and
//! TokenRequest. Each mutation of a listed object carries its UID, so it fails
//! on an object recreated under the same name since it was listed.

use crate::k8s::cluster::discovery::DiscoveredKind;

mod calls;
mod facts;
mod quantity;

pub(crate) use calls::{
    add_taint, create_token, expand, remove_taint, set_paused, set_suspended, trigger_now,
};
pub(crate) use facts::{ObjectFacts, Taint, TaintEffect, openable};
pub(crate) use quantity::bytes;

/// Every kind-specific action, in the order a menu lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum KindAction {
    ForwardService,
    TriggerNow,
    Suspend,
    Resume,
    PauseRollout,
    ResumeRollout,
    Expand,
    AddTaint,
    RemoveTaint,
    CreateToken,
    OpenHost,
}

impl KindAction {
    pub(crate) const ALL: [Self; 11] = [
        Self::ForwardService,
        Self::TriggerNow,
        Self::Suspend,
        Self::Resume,
        Self::PauseRollout,
        Self::ResumeRollout,
        Self::Expand,
        Self::AddTaint,
        Self::RemoveTaint,
        Self::CreateToken,
        Self::OpenHost,
    ];

    /// The menu item's label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ForwardService => "Port Forward…",
            Self::TriggerNow => "Trigger Now",
            Self::Suspend => "Suspend",
            Self::Resume => "Resume",
            Self::PauseRollout => "Pause Rollout",
            Self::ResumeRollout => "Resume Rollout",
            Self::Expand => "Expand…",
            Self::AddTaint => "Add Taint…",
            Self::RemoveTaint => "Remove Taint…",
            Self::CreateToken => "Create Token",
            Self::OpenHost => "Open Host in Browser",
        }
    }

    /// What `kind`'s object, as `facts` reads it, is offered. A mutation needs
    /// the kind's patch verb; the others make their own request or none.
    pub(crate) fn offered(kind: &DiscoveredKind, facts: &ObjectFacts) -> Vec<Self> {
        let (group, name) = (kind.gvk.group.as_str(), kind.gvk.kind.as_str());
        let patch = kind.verbs.patch;
        let mut offered = Vec::new();
        match (group, name) {
            ("", "Service") => offered.push(Self::ForwardService),
            ("batch", "CronJob") => {
                offered.push(Self::TriggerNow);
                if patch {
                    offered.push(Self::suspend_or_resume(facts));
                }
            }
            ("batch", "Job") if patch => offered.push(Self::suspend_or_resume(facts)),
            ("apps", "Deployment") if patch => offered.push(if facts.paused {
                Self::ResumeRollout
            } else {
                Self::PauseRollout
            }),
            ("", "PersistentVolumeClaim") if patch && facts.storage.is_some() => {
                offered.push(Self::Expand)
            }
            ("", "Node") if patch => {
                offered.push(Self::AddTaint);
                if !facts.taints.is_empty() {
                    offered.push(Self::RemoveTaint);
                }
            }
            ("", "ServiceAccount") => offered.push(Self::CreateToken),
            ("networking.k8s.io", "Ingress") if facts.host.is_some() => {
                offered.push(Self::OpenHost)
            }
            _ => {}
        }
        offered
    }

    fn suspend_or_resume(facts: &ObjectFacts) -> Self {
        if facts.suspended {
            Self::Resume
        } else {
            Self::Suspend
        }
    }
}

#[cfg(test)]
mod tests;
