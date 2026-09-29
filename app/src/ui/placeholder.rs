//! The panel a discovered kind opens when this build has no concrete
//! implementation for it yet.
//!
//! Section 8.2 of the `cluster-picker-and-navigation` change: every row in the
//! Resource panel is openable, so a kind nobody has written a table for still
//! gives the user a panel that names it and says what is missing - rather than
//! the row doing nothing at all, which would leave a listed kind looking broken.

use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::*;
use kube::core::GroupVersionKind;

pub fn register_restore(cx: &mut App) {
    register_panel(cx, "Resource", |context, _window, cx| {
        let PanelInfo::Panel(state) = context.info() else {
            panic!("Resource layout state must be a panel");
        };
        let context_name = state["context_name"]
            .as_str()
            .expect("Resource layout state must name its cluster")
            .to_string();
        let namespaces = serde_json::from_value(state["namespaces"].clone()).unwrap_or_default();
        let kind = DiscoveredKind {
            gvk: GroupVersionKind::gvk(
                state["group"].as_str().unwrap_or_default(),
                state["version"].as_str().unwrap_or("v1"),
                state["kind"].as_str().unwrap_or("Resource"),
            ),
            plural: state["plural"].as_str().unwrap_or("resources").to_string(),
            namespaced: state["namespaced"].as_bool().unwrap_or(false),
        };
        let scope = PanelScope::new(crate::ui::nav::NavTarget::Kind(kind.clone()), context_name)
            .scoped_to(namespaces);
        panel_handle(cx.new(|cx| PlaceholderPanel::new(kind, scope, cx)))
    });
}

/// A dock panel standing in for a kind this build has no table for.
pub struct PlaceholderPanel {
    kind: DiscoveredKind,
    scope: PanelScope,
    namespaces: Entity<crate::k8s::cluster::namespaces::NamespaceList>,
    focus_handle: FocusHandle,
}

impl PlaceholderPanel {
    pub fn new(kind: DiscoveredKind, scope: PanelScope, cx: &mut Context<Self>) -> Self {
        let namespaces =
            crate::k8s::cluster::namespaces::NamespaceRegistry::list(cx, &scope.context_name);
        Self::with_namespaces(kind, scope, namespaces, cx)
    }

    /// Construction from an explicit namespace list, so a test can hand in a
    /// list that never syncs instead of the registry's - the same seam
    /// `PodsPanel` and `PodDetailPanel` use.
    ///
    /// It exists because `NamespaceRegistry::list` reaches
    /// `ClusterRegistry::connection`, which starts a real connect on a tokio
    /// worker. gpui's test harness runs the assertion on its own thread and
    /// fails the test on any cross-thread activity it can see, so a test that
    /// only wants to know which kind a panel holds would otherwise be testing
    /// the tokio runtime as well.
    fn with_namespaces(
        kind: DiscoveredKind,
        scope: PanelScope,
        namespaces: Entity<crate::k8s::cluster::namespaces::NamespaceList>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&namespaces, |_, _, cx| cx.notify()).detach();
        Self {
            kind,
            scope,
            namespaces,
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
impl EventEmitter<ScopeEvent> for PlaceholderPanel {}

impl Render for PlaceholderPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        panel_title::focus_frame(
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
                        .child(format!(
                            "Discovered from cluster {}.",
                            self.scope.context_name
                        )),
                ),
            &self.focus_handle,
            window,
            cx,
        )
    }
}

// `Resource` rather than the kind's name: `panel_name` is a persisted-layout
// identifier and must be `&'static str`, while the kind is only known at
// runtime. The title bar below is what names a panel for the user; this only
// has to be stable and distinct from the concrete panels.
impl BasePanel for PlaceholderPanel {
    fn panel_name(&self) -> &'static str {
        "Resource"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": self.scope.context_name,
                "namespaces": self.scope.namespaces,
                "group": self.kind.gvk.group,
                "version": self.kind.gvk.version,
                "kind": self.kind.gvk.kind,
                "plural": self.kind.plural,
                "namespaced": self.kind.namespaced,
            })),
        }
    }
}

/// Section 10: the title bar. The dock draws it and lays the parts out - this
/// only supplies them, so a placeholder and a concrete panel get the same bar.
impl Panel for PlaceholderPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title(&self.scope)
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn title_suffix(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let this = cx.weak_entity();
        let namespaces = self.namespaces.read(cx).names();
        panel_title::namespace_picker(&self.scope, namespaces, move |namespaces, cx| {
            let _ = this.update(cx, |this: &mut Self, cx| {
                this.scope = this.scope.scoped_to(namespaces.clone());
                cx.emit(ScopeEvent::NamespacesChanged(namespaces));
            });
        })
    }

    fn toolbar_buttons(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Vec<Button>> {
        panel_title::toolbar_buttons()
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}

#[cfg(test)]
mod tests {
    use super::PlaceholderPanel;
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use crate::k8s::cluster::namespaces::NamespaceList;
    use crate::ui::nav::NavTarget;
    use crate::ui::panel_title::PanelScope;
    use gpui_kit::AppContext as _;
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
            PlaceholderPanel::with_namespaces(
                kind("ferns.example.com", "Fern"),
                PanelScope::new(
                    NavTarget::Kind(kind("ferns.example.com", "Fern")),
                    "kind-dev".into(),
                ),
                cx.new(|_| NamespaceList::empty()),
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
