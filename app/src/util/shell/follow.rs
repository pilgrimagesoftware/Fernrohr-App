//! Following a reference out of a detail view: `resource-links` section 3.
//!
//! The link itself (`ui::link`) only says *what* was followed and from which
//! cluster context. Here the window turns that into a panel, through the same
//! open/dedup/focus path every other request takes - so "following it again
//! focuses the existing panel" is `open_target_in`'s existing behaviour, not
//! new logic.

use super::{MainWindow, WindowMode};
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::resource::object_list::OpenListedObject;
use crate::ui::link::FollowReference;
use crate::ui::nav::{NavTarget, OpenMode, OpenPodInBackground};
use crate::ui::viewer::viewer_for;
use gpui_kit::*;

impl MainWindow {
    /// Opens (or focuses) the panel `viewer_for` says shows the followed
    /// object, in the context the reference was shown in. A reference with no
    /// viewer is never drawn as a link, so `None` here only means a stale
    /// dispatch - nothing to open.
    pub(super) fn on_action_follow_reference(
        &mut self,
        action: &FollowReference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let discovery = DiscoveryRegistry::kinds(cx, &action.context_name);
        let destination = viewer_for(&action.target, discovery.read(cx).kinds());
        let Some(destination) = destination else {
            log::debug!("no viewer for followed reference {:?}", action.target);
            return;
        };
        self.open_target_in(
            destination.target,
            None,
            Some(action.context_name.clone()),
            destination.namespaces,
            action.mode,
            window,
            cx,
        );
    }
}

impl MainWindow {
    /// Opens (or focuses) the detail panel of an object a list panel's row was
    /// activated on, in the list's own context (`standard-resource-panels` D5). The
    /// object's kind came from discovery already, so unlike a followed reference
    /// there's nothing to look up; and a Namespace row opens that Namespace's detail,
    /// where `viewer_for` would send a reference to it to the Pods list.
    pub(super) fn on_action_open_listed_object(
        &mut self,
        action: &OpenListedObject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_target_in(
            NavTarget::Object(action.target.clone()),
            action.view,
            Some(action.context_name.clone()),
            Vec::new(),
            action.mode,
            window,
            cx,
        );
    }
}

impl MainWindow {
    /// Opens a pod's detail panel in the background, in the pod's own context
    /// (`open-in-background`): a Pods row's modified or middle click, or its Open
    /// in Background.
    pub(super) fn on_action_open_pod_in_background(
        &mut self,
        action: &OpenPodInBackground,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_target_in(
            NavTarget::pod(action.namespace.clone(), action.name.clone()),
            None,
            Some(action.context_name.clone()),
            Vec::new(),
            OpenMode::Background,
            window,
            cx,
        );
    }
}

impl MainWindow {
    /// Edits one object's YAML (`EditListedObject`): opens or focuses its
    /// detail panel in the requester's context, on the YAML, and starts the
    /// edit there once the object is loaded. One editor and apply path, the
    /// object panel's, whichever list or panel asked.
    pub(super) fn on_action_edit_listed_object(
        &mut self,
        action: &crate::k8s::resource::object_list::EditListedObject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = NavTarget::Object(action.target.clone());
        self.open_target_in(
            target.clone(),
            Some(crate::k8s::resource::pod_detail::DetailView::Yaml),
            Some(action.context_name.clone()),
            Vec::new(),
            OpenMode::Foreground,
            window,
            cx,
        );
        let WindowMode::Workspace {
            open_panels,
            dock_area,
            ..
        } = &self.mode
        else {
            return;
        };
        let Some(open) = open_panels
            .iter()
            .find(|open| open.key.target == target && open.key.context_name == action.context_name)
        else {
            return;
        };
        let panel = open
            .panel
            .clone()
            .or_else(|| crate::ui::nav::opened_panel_for(dock_area.read(cx), open.id, cx));
        if let Some(crate::ui::nav::OpenedPanel::ObjectDetail(panel)) = panel {
            panel.update(cx, |panel, cx| panel.request_edit(window, cx));
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod background_tests;

#[cfg(test)]
mod list_keys_tests;
