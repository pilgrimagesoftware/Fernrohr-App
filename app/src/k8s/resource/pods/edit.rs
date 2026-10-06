//! Edit YAML from the Pods panel (#140): `e` edits the selected pod's manifest
//! in the object panel's editor - the one editor and apply path every kind
//! uses - opened on the YAML.

use super::*;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_list::EditListedObject;
use crate::ui::nav::ObjectTarget;

impl PodsPanel {
    pub(super) fn on_action_edit_pod(
        &mut self,
        _: &EditPod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.table_selection(cx) else {
            return;
        };
        window.dispatch_action(
            Box::new(EditListedObject {
                context_name: selection.context_name,
                target: ObjectTarget {
                    kind: DiscoveredKind::pods(),
                    namespace: Some(selection.namespace),
                    name: selection.name,
                },
            }),
            cx,
        );
    }
}
