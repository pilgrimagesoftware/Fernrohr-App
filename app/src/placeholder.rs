//! The panel a discovered kind opens when this build has no concrete
//! implementation for it yet.
//!
//! Section 8.2 of the `cluster-picker-and-navigation` change: every row in the
//! Resource panel is openable, so a kind nobody has written a table for still
//! gives the user a panel that names it and says what is missing - rather than
//! the row doing nothing at all, which would leave a listed kind looking broken.

use crate::cluster::discovery::DiscoveredKind;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent};
use gpui_kit::*;

/// A dock panel standing in for a kind this build has no table for.
pub struct PlaceholderPanel {
    kind: DiscoveredKind,
    context_name: String,
    focus_handle: FocusHandle,
}

impl PlaceholderPanel {
    pub fn new(kind: DiscoveredKind, context_name: String, cx: &mut Context<Self>) -> Self {
        Self {
            kind,
            context_name,
            focus_handle: cx.focus_handle(),
        }
    }

    /// The kind this panel stands in for. Read by the test, which cannot read
    /// rendered text back out of the harness.
    #[cfg(test)]
    pub fn kind(&self) -> &DiscoveredKind {
        &self.kind
    }
}

impl Focusable for PlaceholderPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PlaceholderPanel {}

impl Render for PlaceholderPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .p_6()
            .child(
                div()
                    .text_lg()
                    .text_color(theme.foreground)
                    .child(self.kind.label()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{} has no panel implementation yet.",
                        self.kind.gvk.api_version()
                    )),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(format!("Discovered from cluster {}.", self.context_name)),
            )
    }
}

// `Resource` rather than the kind's name: `panel_name` is a persisted-layout
// identifier and must be `&'static str`, while the kind is only known at
// runtime. Section 10's per-kind title bar is what names a panel for the user;
// this only has to be stable and distinct from the concrete panels.
impl BasePanel for PlaceholderPanel {
    fn panel_name(&self) -> &'static str {
        "Resource"
    }
}

impl Panel for PlaceholderPanel {}

#[cfg(test)]
mod tests {
    use super::PlaceholderPanel;
    use crate::cluster::discovery::DiscoveredKind;
    use gpui_kit::TestAppContext;
    use kube::core::GroupVersionKind;

    fn kind(group: &str, kind: &str) -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, "v1", kind),
            plural: format!("{kind}s"),
            namespaced: true,
        }
    }

    /// A placeholder keeps the kind it was opened for, so the panel it builds
    /// can say which kind it is standing in for rather than being a generic
    /// "not implemented" screen.
    #[gpui_kit::test]
    async fn placeholder_remembers_the_kind_it_was_opened_for(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = cx.add_window(|_window, cx| {
            PlaceholderPanel::new(
                kind("ferns.example.com", "Fern"),
                "kind-dev".to_string(),
                cx,
            )
        });

        window
            .update(cx, |panel, _window, _cx| {
                assert_eq!(panel.kind().gvk.kind, "Fern");
                assert_eq!(panel.kind().gvk.group, "ferns.example.com");
                assert_eq!(panel.kind().plural, "Ferns");
            })
            .unwrap();
    }
}
