//! The panel a saved list comes back as when its cluster no longer serves its kind
//! (`standard-resource-panels` D4). Every discovered kind opens a real list now, so
//! this is only a restore fallback: it names the kind and says why there's no list,
//! rather than restoring a list that could never fill.

use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::panel_title::{self, PanelScope, ScopeEvent};
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, register_panel,
};
use gpui_kit::*;

/// Restores a saved `Resource` panel - what a placeholder saves - through the list
/// panel's restore: it comes back as a list once its kind is served again, and as a
/// placeholder only while its cluster no longer reports the kind
/// (`standard-resource-panels` D4).
pub fn register_restore(cx: &mut App) {
    register_panel(cx, "Resource", |context, _window, cx| {
        crate::ui::unrestored::restore_with(&context, cx, |state, cx| {
            crate::k8s::resource::object_list::restore::restore(state, cx)
        })
    });
}

/// The placeholder's key context: its one command, Pick Namespaces, acts
/// only while it is on the focus path.
pub const PANEL_KEY_CONTEXT: &str = "PlaceholderPanel";

/// Registers the placeholder's command: Pick Namespaces.
pub fn register_commands(registry: &mut crate::command::CommandRegistry) {
    registry.register(crate::ui::namespace_picker::pick_namespaces_command(
        "placeholder.pick_namespaces",
        "Placeholder: Pick Namespaces",
        PANEL_KEY_CONTEXT,
    ));
}

/// A dock panel standing in for a kind its cluster no longer serves: a saved list
/// panel restored after discovery stopped reporting its kind.
pub struct PlaceholderPanel {
    kind: DiscoveredKind,
    scope: PanelScope,
    namespaces: Entity<crate::k8s::cluster::namespaces::NamespaceList>,
    focus_handle: FocusHandle,
    /// The title bar's namespace picker, made on first render.
    namespace_picker: crate::ui::namespace_picker::NamespacePickerSlot,
}

impl PlaceholderPanel {
    /// Scopes this panel to `namespaces` (empty for all), as its own picker does -
    /// what Warp All to Namespace applies to every namespaced panel in a context.
    pub(crate) fn set_namespaces(&mut self, namespaces: Vec<String>, cx: &mut Context<Self>) {
        self.scope = self.scope.scoped_to(namespaces.clone());
        cx.emit(crate::ui::panel_title::ScopeEvent::NamespacesChanged(
            namespaces,
        ));
        cx.notify();
    }
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
    pub(crate) fn with_namespaces(
        kind: DiscoveredKind,
        scope: PanelScope,
        namespaces: Entity<crate::k8s::cluster::namespaces::NamespaceList>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&namespaces, |_, _, cx| cx.notify()).detach();
        Self {
            kind,
            scope,
            namespace_picker: Default::default(),
            namespaces,
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
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
        let raised = crate::ui::style::surface_raised(cx);
        let space = crate::ui::space::spacing(cx);
        let this = cx.weak_entity();
        let namespaces = self.namespaces.read(cx).names().to_vec();
        let namespace_bar = self
            .namespace_picker
            .element(
                &self.scope,
                &namespaces,
                move |namespaces, cx| {
                    let _ = this.update(cx, |this: &mut Self, cx| {
                        this.scope = this.scope.scoped_to(namespaces.clone());
                        cx.emit(ScopeEvent::NamespacesChanged(namespaces));
                    });
                },
                window,
                cx,
            )
            .map(|picker| {
                div()
                    .flex()
                    .justify_end()
                    .px(space.panel_inset)
                    .py(space.control_gap)
                    .bg(raised)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(picker)
            });

        div()
            .size_full()
            // Tracked so a click focuses the panel, which is what lights
            // its tab's focus underline.
            .track_focus(&self.focus_handle)
            .key_context(PANEL_KEY_CONTEXT)
            .on_action(cx.listener(
                |this, _: &crate::ui::namespace_picker::PickNamespaces, window, cx| {
                    this.namespace_picker.open(window, cx)
                },
            ))
            .flex()
            .flex_col()
            .children(namespace_bar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(space.control_gap)
                    .p(space.panel_inset)
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
                                "Cluster {} no longer serves {}.",
                                self.scope.context_name,
                                self.kind.gvk.api_version()
                            )),
                    )
                    .child(div().text_sm().text_color(theme.muted_foreground).child(
                        "This panel came back with a saved layout. It reopens as a \
                                 list once the cluster serves the kind again.",
                    )),
            )
            // Tab stays in the panel: see `ui::panel::focus`.
            .focus_trap("placeholder-panel-tab-trap", &self.focus_handle)
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
    fn title(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(
            &self.scope,
            panel_title::title(&self.scope),
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
mod tests {
    use super::PlaceholderPanel;
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use crate::k8s::cluster::namespaces::NamespaceList;
    use crate::ui::nav::NavTarget;
    use crate::ui::panel_title::PanelScope;
    use gpui_kit::AppContext as _;
    use gpui_kit::component::dock::{DockArea, DockPlacement, DockSkin, panel_handle};
    use gpui_kit::{
        Context, Entity, IntoElement, Modifiers, ParentElement as _, Render, Styled as _,
        TestAppContext, VisualTestContext, Window, div, point, px,
    };
    use kube::core::GroupVersionKind;
    use std::rc::Rc;

    fn kind(group: &str, kind: &str) -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, "v1", kind),
            plural: format!("{kind}s"),
            namespaced: true,
            verbs: Default::default(),
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

    /// A click inside the panel focuses it - the focus its tab's underline
    /// follows. Without `track_focus` on the body a click landed nowhere, and
    /// the panel could never be marked as the focused one.
    #[gpui_kit::test]
    async fn a_click_focuses_the_placeholder(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let (panel, cx) = cx.add_window_view(|_window, cx| {
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
        cx.run_until_parked();
        let focused = |cx: &mut gpui_kit::VisualTestContext| {
            cx.update(|window, cx| panel.read(cx).focus_handle.contains_focused(window, cx))
        };
        assert!(!focused(cx), "nothing has focused the panel yet");

        cx.simulate_click(point(px(200.), px(200.)), Modifiers::none());
        cx.run_until_parked();
        assert!(focused(cx), "the click focuses the panel");
    }

    /// The app's dock, hosting whatever is added to it.
    struct DockHost {
        area: Entity<DockArea>,
        _skin: Rc<DockSkin>,
    }

    impl Render for DockHost {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.area.clone())
        }
    }

    /// Whether the dock drew `title`'s tab label in the given focus state.
    fn title_drawn(cx: &mut VisualTestContext, title: &str, focused: bool) -> bool {
        let state = if focused { "focused" } else { "unfocused" };
        // `debug_bounds` wants a `'static` selector; leaking a test's few
        // short strings is harmless.
        let selector: &'static str = format!("panel-title-{title}-{state}").leak();
        cx.debug_bounds(selector).is_some()
    }

    /// The tab strip draws each panel's own title element, so the focused
    /// panel's tab - and only it - carries the focus underline, and the
    /// underline moves when a click on the other tab moves focus there.
    #[gpui_kit::test]
    async fn the_focused_panels_tab_is_the_one_underlined(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let (host, cx) = cx.add_window_view(|window, cx| {
            let (area, skin) = DockSkin::dock_area("focus-test", Some(1), window, cx);
            DockHost { area, _skin: skin }
        });
        let area = cx.update(|_window, cx| host.read(cx).area.clone());
        let placeholder = |kind_name: &'static str, cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                let panel = cx.new(|cx| {
                    PlaceholderPanel::with_namespaces(
                        kind("", kind_name),
                        PanelScope::new(NavTarget::Kind(kind("", kind_name)), "kind-dev".into()),
                        cx.new(|_| NamespaceList::empty()),
                        cx,
                    )
                });
                area.update(cx, |area, cx| {
                    area.add_panel_view(
                        panel_handle(panel.clone()),
                        DockPlacement::Center,
                        None,
                        window,
                        cx,
                    )
                });
                panel
            })
        };
        let _fern = placeholder("Fern", cx);
        let moss = placeholder("Moss", cx);
        cx.run_until_parked();

        cx.update(|window, cx| {
            let handle = moss.read(cx).focus_handle.clone();
            handle.focus(window, cx);
        });
        cx.run_until_parked();
        assert!(
            title_drawn(cx, "Mosss", true),
            "the focused panel's tab is underlined"
        );
        assert!(title_drawn(cx, "Ferns", false), "the other tab is not");

        let fern_tab = cx
            .debug_bounds("panel-title-Ferns-unfocused")
            .expect("Fern's tab is drawn");
        cx.simulate_click(fern_tab.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            title_drawn(cx, "Ferns", true),
            "the underline follows focus to Fern"
        );
        assert!(title_drawn(cx, "Mosss", false), "and leaves Moss");
    }
}
