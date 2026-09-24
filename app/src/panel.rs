use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent};
use gpui_kit::*;

/// A minimal named panel with no content beyond its title. Stands in for the
/// real panel kinds (Pods, logs) added in later changes, proving the dock's
/// split/resize/close mechanics without depending on them.
pub struct PlaceholderPanel {
    title: SharedString,
    focus_handle: FocusHandle,
}

impl PlaceholderPanel {
    pub fn new(title: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            title: title.into(),
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Focusable for PlaceholderPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PlaceholderPanel {}

impl Render for PlaceholderPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.title.clone())
    }
}

impl BasePanel for PlaceholderPanel {
    fn panel_name(&self) -> &'static str {
        "Placeholder"
    }
}

impl Panel for PlaceholderPanel {}
