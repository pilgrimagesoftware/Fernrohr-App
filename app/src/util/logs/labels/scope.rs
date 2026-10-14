//! A typed label panel's namespace scope (`selector-logs`): the namespaces its
//! selector searches, shown in its bar with the shared namespace picker and
//! changed there, by `n`, or by switching to a namespace set. A workload's
//! panel stays in its workload's namespace and has no picker.

use super::*;
use crate::k8s::cluster::namespaces::{NamespaceList, NamespaceRegistry};
use crate::ui::namespace_picker::NamespacePickerSlot;

/// A typed panel's picker and the context's namespaces it offers, made on the
/// panel's first render.
#[derive(Default)]
pub(in crate::util::logs) struct ScopePicker {
    slot: NamespacePickerSlot,
    namespaces: Option<Entity<NamespaceList>>,
}

impl LogsPanel {
    /// Scopes a typed label panel's selector to `namespaces` (empty for all):
    /// streams of pods outside it stop, those inside it start. `false` - and
    /// nothing done - for any other Logs panel, whose namespace isn't its own
    /// to change.
    pub(crate) fn set_namespaces(
        &mut self,
        namespaces: Vec<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.is_typed_labels() {
            return false;
        }
        self.scope = self.scope.scoped_to(namespaces.clone());
        cx.emit(ScopeEvent::NamespacesChanged(namespaces));
        self.sync_labels(cx);
        cx.notify();
        true
    }

    pub(super) fn is_typed_labels(&self) -> bool {
        self.labels
            .as_ref()
            .is_some_and(|following| following.source == LabelLogs::Typed)
    }

    /// The picker reading a typed panel's scope; `None` for any other.
    pub(in crate::util::logs) fn scope_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.is_typed_labels() {
            return None;
        }
        let context_name = self.scope.context_name.clone();
        let picker = &mut self.labels.as_mut()?.scope;
        let list = match &picker.namespaces {
            Some(list) => list.clone(),
            None => {
                let list = NamespaceRegistry::list(cx, &context_name);
                cx.observe(&list, |_, _, cx| cx.notify()).detach();
                picker.namespaces = Some(list.clone());
                list
            }
        };
        let namespaces = list.read(cx).names().to_vec();
        let this = cx.weak_entity();
        // `picker` borrows `self.labels`, `&self.scope` a field beside it.
        picker.slot.element(
            &self.scope,
            &namespaces,
            move |namespaces, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.set_namespaces(namespaces, cx)
                });
            },
            window,
            cx,
        )
    }

    /// `PickNamespaces` in a typed panel: opens its picker.
    pub(in crate::util::logs) fn open_scope_picker(&self, window: &mut Window, cx: &mut App) {
        if let Some(following) = &self.labels {
            following.scope.slot.open(window, cx);
        }
    }
}
