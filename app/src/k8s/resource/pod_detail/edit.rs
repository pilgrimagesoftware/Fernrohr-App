//! Edit YAML from a pod's detail panel (#140): `e` edits this pod's manifest
//! in the object panel's editor - the one editor and apply path every kind
//! uses - opened beside this panel on the YAML.

use super::commands::EditPodYaml;
use super::panel::PodDetailPanel;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_list::EditListedObject;
use crate::ui::nav::ObjectTarget;
use gpui_kit::{Context, Window};

impl PodDetailPanel {
    pub(super) fn on_action_edit_pod_yaml(
        &mut self,
        _: &EditPodYaml,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.dispatch_action(
            Box::new(EditListedObject {
                context_name: self.scope.context_name.clone(),
                target: ObjectTarget {
                    kind: DiscoveredKind::pods(),
                    namespace: Some(self.pod.namespace.clone()),
                    name: self.pod.name.clone(),
                },
            }),
            cx,
        );
    }
}
