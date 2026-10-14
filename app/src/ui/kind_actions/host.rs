//! What a panel provides to run its kind's actions ([`KindHost`]) - the object
//! they act on, and where a refusal is shown - and the handlers and key
//! contexts every such panel gets from it.

use super::commands::*;
use super::flow;
use crate::k8s::cluster::connection::ClusterConnection;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::delete_flow::refusal::Refusal;
use crate::k8s::resource::kind_actions::{KindAction, ObjectFacts};
use gpui_kit::*;

/// The object a kind action acts on.
#[derive(Clone)]
pub(crate) struct KindTarget {
    pub(crate) context_name: String,
    pub(crate) connection: Entity<ClusterConnection>,
    pub(crate) kind: DiscoveredKind,
    pub(crate) namespace: Option<String>,
    pub(crate) name: String,
    pub(crate) facts: ObjectFacts,
}

impl KindTarget {
    pub(crate) fn offered(&self) -> Vec<KindAction> {
        KindAction::offered(&self.kind, &self.facts)
    }

    /// `CronJob backup`.
    pub(crate) fn noun(&self) -> String {
        format!("{} {}", self.kind.gvk.kind, self.name)
    }

    pub(crate) fn namespace(&self) -> &str {
        self.namespace.as_deref().unwrap_or_default()
    }
}

/// A panel showing one object, or a selected row, that kind actions act on.
pub(crate) trait KindHost: Sized + 'static {
    /// The object, or `None` with none selected or loaded.
    fn kind_target(&self, cx: &App) -> Option<KindTarget>;

    /// Shows why an action failed, or - `None` - clears it.
    fn show_refusal(&mut self, refusal: Option<Refusal>, cx: &mut Context<Self>);
}

/// Adds the key contexts of what `this`'s object is offered.
pub(crate) fn add_key_contexts<T: KindHost>(
    this: &T,
    key_context: &mut KeyContext,
    in_list: bool,
    cx: &App,
) {
    if let Some(target) = this.kind_target(cx) {
        add_contexts(key_context, &target.offered(), in_list);
    }
}

/// `div` with every kind action's handler, each acting on `T`'s object.
pub(crate) fn on_kind_actions<T: KindHost>(div: Div, cx: &mut Context<T>) -> Div {
    macro_rules! kind {
        ($div:expr, $($action:ident),* $(,)?) => {
            $div$(.on_action(cx.listener(|this: &mut T, _: &$action, window, cx| {
                flow::start(KindAction::$action, this, window, cx)
            })))*
        };
    }
    kind!(
        div,
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
    )
}
