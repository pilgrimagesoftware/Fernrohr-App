//! The panel's dock identity: its saved-layout [`BasePanel::dump`], and its
//! [`Panel`] title/tab/toolbar/zoom surface, including the streaming title
//! that names the selected pod and container once one is picked.

use super::*;

impl BasePanel for LogsPanel {
    fn panel_name(&self) -> &'static str {
        "Logs"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": self.scope.context_name,
                "namespaces": self.scope.namespaces,
            })),
        }
    }
}

/// Section 10: the title bar, supplied to the dock rather than drawn here.
/// What a Logs panel's title/tab reads for `current` (namespace, pod,
/// container), once a pod has been selected. `fallback` is whatever the
/// panel shows before that - the generic scope-derived label.
fn streaming_title(
    current: Option<&(String, String, String)>,
    fallback: impl Fn() -> String,
) -> String {
    match current {
        Some((_, pod, container)) => format!("Logs: {pod} · {container}"),
        None => fallback(),
    }
}

impl LogsPanel {
    fn streaming_title(&self) -> String {
        let title = streaming_title(self.current.as_ref(), || panel_title::title(&self.scope));
        if self.previous && self.current.is_some() {
            format!("{title} (previous)")
        } else {
            title
        }
    }
}

impl Panel for LogsPanel {
    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(
            &self.scope,
            self.streaming_title(),
            &self.focus_handle,
            panel_title::close_button(cx.entity()),
            window,
            cx,
        )
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}

#[cfg(test)]
mod tests;
