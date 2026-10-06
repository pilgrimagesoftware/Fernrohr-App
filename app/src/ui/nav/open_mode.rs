//! Opening a panel in the background (`open-in-background`): the mode every open
//! request carries, and the action a Pods row's background open dispatches.

use gpui_kit::*;

/// How a request for a panel treats focus (`open-in-background`): a foreground
/// open shows the panel and focuses it; a background open adds it as an inactive
/// tab - or, when it's already open, leaves it be - and moves nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpenMode {
    #[default]
    Foreground,
    Background,
}

impl OpenMode {
    /// A click with the platform modifier (`cmd` on macOS, `ctrl` elsewhere) or a
    /// middle-click opens in the background; any other click in the foreground.
    pub fn of_click(event: &ClickEvent) -> Self {
        if event.modifiers().secondary() || event.is_middle_click() {
            Self::Background
        } else {
            Self::Foreground
        }
    }
}

/// Opens a pod's detail panel in the background (`open-in-background`): what a
/// Pods row's modified or middle click and the Pods panel's Open in Background
/// dispatch. Carries its pod, like `OpenListedObject` - unlike `ShowPodDetail`,
/// which reads `SelectedPod` - since a background open must not move the selection.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = nav, no_json)]
pub struct OpenPodInBackground {
    pub context_name: String,
    pub namespace: String,
    pub name: String,
}
