//! The release detail panel's tab keys as registry commands, scoped to the
//! panel's key context (helm-releases D5).

use crate::command::{Command, CommandRegistry};
use gpui_kit::{Action, actions};

actions!(
    helm_release,
    [
        ShowOverview,
        ShowHistory,
        ShowValues,
        ShowManifest,
        ShowNotes,
        ShowResources,
        ToggleReveal,
        ViewValues,
        ViewManifest,
        CopyValues,
        CopyManifest,
        OpenResource
    ]
);

/// The panel's key context.
pub(crate) const PANEL_KEY_CONTEXT: &str = "HelmReleasePanel";

/// The tabs' default keys.
pub(super) const OVERVIEW_KEY: &str = "1";
pub(super) const HISTORY_KEY: &str = "2";
pub(super) const VALUES_KEY: &str = "3";
pub(super) const MANIFEST_KEY: &str = "4";
pub(super) const NOTES_KEY: &str = "5";
/// Reveal or hide the Values or Manifest tab's text.
pub(super) const REVEAL_KEY: &str = "r";
/// From History: the selected revision's values, or its manifest.
pub(super) const VIEW_VALUES_KEY: &str = "v";
pub(super) const VIEW_MANIFEST_KEY: &str = "m";
pub(super) const RESOURCES_KEY: &str = "6";
/// Opens the Resources tab's selected object (section 6).
pub(super) const OPEN_RESOURCE_KEY: &str = "enter";

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, action: Box<dyn Action>| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(PANEL_KEY_CONTEXT),
            action,
            menu: None,
        });
    };
    register(
        "helm_release.overview",
        "Helm Release: Overview",
        OVERVIEW_KEY,
        Box::new(ShowOverview),
    );
    register(
        "helm_release.history",
        "Helm Release: History",
        HISTORY_KEY,
        Box::new(ShowHistory),
    );
    register(
        "helm_release.values",
        "Helm Release: Values",
        VALUES_KEY,
        Box::new(ShowValues),
    );
    register(
        "helm_release.manifest",
        "Helm Release: Manifest",
        MANIFEST_KEY,
        Box::new(ShowManifest),
    );
    register(
        "helm_release.notes",
        "Helm Release: Notes",
        NOTES_KEY,
        Box::new(ShowNotes),
    );
    register(
        "helm_release.reveal",
        "Helm Release: Reveal or Hide",
        REVEAL_KEY,
        Box::new(ToggleReveal),
    );
    register(
        "helm_release.view_values",
        "Helm Release: View Revision's Values",
        VIEW_VALUES_KEY,
        Box::new(ViewValues),
    );
    register(
        "helm_release.view_manifest",
        "Helm Release: View Revision's Manifest",
        VIEW_MANIFEST_KEY,
        Box::new(ViewManifest),
    );
    // Explicit, and only while revealed (helm-releases D6); no default key.
    register(
        "helm_release.copy_values",
        "Helm Release: Copy Values",
        "",
        Box::new(CopyValues),
    );
    register(
        "helm_release.copy_manifest",
        "Helm Release: Copy Manifest",
        "",
        Box::new(CopyManifest),
    );
    register(
        "helm_release.resources",
        "Helm Release: Resources",
        RESOURCES_KEY,
        Box::new(ShowResources),
    );
    register(
        "helm_release.open_resource",
        "Helm Release: Open Selected Resource",
        OPEN_RESOURCE_KEY,
        Box::new(OpenResource),
    );
}
