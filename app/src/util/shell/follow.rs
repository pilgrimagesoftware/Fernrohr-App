//! Following a reference out of a detail view: `resource-links` section 3.
//!
//! The link itself (`ui::link`) only says *what* was followed and from which
//! cluster context. Here the window turns that into a panel, through the same
//! open/dedup/focus path every other request takes - so "following it again
//! focuses the existing panel" is `open_target_in`'s existing behaviour, not
//! new logic.

use super::MainWindow;
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::ui::link::FollowReference;
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
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests;
