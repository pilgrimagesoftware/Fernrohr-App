//! Drawing the panel: the header with its shortcut hints, the tabbed
//! structured view, the Events tab and the YAML view.

use super::commands::{
    CONFIGURATION_TAB_KEY, CONTAINERS_TAB_KEY, EVENTS_TAB_KEY, MANAGED_FIELDS_TAB_KEY,
    OVERVIEW_TAB_KEY, PANEL_KEY_CONTEXT, SelectConfigurationTab, SelectContainersTab,
    SelectEventsTab, SelectManagedFieldsTab, SelectOverviewTab, SelectVolumesTab, TOGGLE_VIEW_KEY,
    ToggleDetailView, VOLUMES_TAB_KEY,
};
use super::fetch::PodDetailState;
use super::model::{DetailSection, DetailView};
use super::panel::PodDetailPanel;
use crate::ui::panel_title;
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use jiff::Timestamp;

impl PodDetailPanel {
    fn render_structured(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let fields = self.fields(Timestamp::now());
        let active_tab = self.active_tab;
        let this = cx.weak_entity();
        let tabs = TabBar::new("pod-detail-tabs")
            .selected_index(
                DetailSection::ALL
                    .iter()
                    .position(|section| *section == active_tab)
                    .unwrap_or(0),
            )
            .on_click(move |ix, _window, cx| {
                let Some(section) = DetailSection::ALL.get(*ix).copied() else {
                    return;
                };
                let _ = this.update(cx, |this: &mut Self, cx| this.set_active_tab(section, cx));
            })
            .children(
                DetailSection::ALL
                    .iter()
                    .map(|section| Tab::new().label(section.label())),
            );
        let content = if active_tab == DetailSection::Configuration {
            self.render_configuration(cx)
        } else if active_tab == DetailSection::Events {
            self.render_events(window, cx)
        } else {
            let rows = crate::ui::detail::striped(
                fields
                    .iter()
                    .filter(|field| field.section == active_tab)
                    .map(|field| self.render_field(field, cx)),
                cx,
            );
            match (active_tab == DetailSection::Overview)
                .then(|| self.render_overview_warnings(cx))
                .flatten()
            {
                Some(warnings) => div()
                    .flex()
                    .flex_col()
                    .gap(crate::ui::space::spacing(cx).section_gap)
                    .child(warnings)
                    .child(rows)
                    .into_any_element(),
                None => rows,
            }
        };
        div()
            .flex()
            .flex_col()
            .child(tabs)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .pt(crate::ui::space::spacing(cx).section_gap)
                    .child(content),
            )
            .into_any_element()
    }

    /// The Events tab: every event naming this pod, newest first. Its own
    /// render path rather than a `PodField` - events come from a separate
    /// fetch, not from `pod_fields`'s projection of the pod object itself.
    fn render_events(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        self.render_events_view(window, cx)
    }

    /// The manifest, folding and scrolling both ways (`ui::yaml_view`).
    fn render_yaml(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(yaml) = self.yaml() else {
            return div().into_any_element();
        };
        let this = cx.weak_entity();
        let on_toggle: crate::ui::yaml_view::OnToggle = std::rc::Rc::new(move |line, _, cx| {
            let _ = this.update(cx, |this: &mut Self, cx| {
                this.yaml_view.toggle(line);
                cx.notify();
            });
        });
        self.yaml_view.element(&yaml, on_toggle, cx)
    }

    fn on_action_fold_all(
        &mut self,
        _: &crate::ui::yaml_view::FoldAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(yaml) = self.yaml() {
            self.yaml_view.fold_all(&yaml);
            cx.notify();
        }
    }

    fn on_action_unfold_all(
        &mut self,
        _: &crate::ui::yaml_view::UnfoldAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.yaml_view.unfold_all();
        cx.notify();
    }
}

impl Render for PodDetailPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        let content = match &self.state {
            PodDetailState::Loading => div()
                .size_full()
                .p(space.panel_inset)
                .child("Loading pod...")
                .into_any_element(),
            PodDetailState::NotFound => div()
                .size_full()
                .p(space.panel_inset)
                .child("This pod no longer exists.")
                .into_any_element(),
            PodDetailState::Failed { message, detail } => panel_title::error_content(
                format!("Could not read pod: {message}"),
                Some(detail.clone()),
                cx,
            )
            .into_any_element(),
            PodDetailState::Loaded(_) => match self.viewing {
                // Field values wrap to the panel's width rather than
                // overflowing it - vertical-only scroll, so nothing pushes
                // the layout wider than the panel actually is.
                DetailView::Structured => div()
                    .size_full()
                    .p(space.panel_inset)
                    .overflow_y_scrollbar()
                    .child(self.render_structured(window, cx))
                    .into_any_element(),
                // YAML is monospace and line-oriented like the Logs panel -
                // it keeps both-axis scroll rather than wrapping lines. The view
                // scrolls itself: a scroll container around a full-height
                // child had nothing to scroll.
                DetailView::Yaml => div()
                    .size_full()
                    .p(space.panel_inset)
                    .child(self.render_yaml(cx))
                    .into_any_element(),
            },
        };

        // The structured/YAML toggle, with the same visible-shortcut-hint
        // convention `PodsPanel` uses - inside the panel's own body, not the
        // dock's shared per-tab-group toolbar, which only reflects whichever
        // tab happens to be active.
        let window_contexts = crate::util::shell::window_context_count(window, cx);
        let has_links = !self.followable(cx).is_empty();
        let yaml = self.viewing == DetailView::Yaml;
        let toggle_key =
            Kbd::binding_for_action(&ToggleDetailView, Some(PANEL_KEY_CONTEXT), window)
                .unwrap_or_else(|| {
                    Kbd::new(Keystroke::parse(TOGGLE_VIEW_KEY).expect("valid keybinding"))
                });
        let toggle_hint = div()
            .flex()
            .items_center()
            .gap_1()
            .child(toggle_key)
            .child(if yaml { "Show fields" } else { "Show YAML" });
        // Built before `tab_key`, which holds `window` for the rest of the
        // header: drawing the icon needs it mutably.
        let heading = panel_title::with_header_icon(
            crate::ui::icon::for_kind("", "Pod"),
            panel_title::item_heading(
                self.pod.name.clone(),
                panel_title::heading_context(&self.scope, window_contexts),
                cx.theme().muted_foreground,
            ),
            window,
            cx,
        );
        let tab_key = |section: DetailSection| -> Kbd {
            let (action, literal): (&dyn Action, &str) = match section {
                DetailSection::Overview => (&SelectOverviewTab as &dyn Action, OVERVIEW_TAB_KEY),
                DetailSection::Containers => {
                    (&SelectContainersTab as &dyn Action, CONTAINERS_TAB_KEY)
                }
                DetailSection::Configuration => (
                    &SelectConfigurationTab as &dyn Action,
                    CONFIGURATION_TAB_KEY,
                ),
                DetailSection::Volumes => (&SelectVolumesTab as &dyn Action, VOLUMES_TAB_KEY),
                DetailSection::Events => (&SelectEventsTab as &dyn Action, EVENTS_TAB_KEY),
                DetailSection::ManagedFields => (
                    &SelectManagedFieldsTab as &dyn Action,
                    MANAGED_FIELDS_TAB_KEY,
                ),
            };
            Kbd::binding_for_action(action, Some(PANEL_KEY_CONTEXT), window)
                .unwrap_or_else(|| Kbd::new(Keystroke::parse(literal).expect("valid keybinding")))
        };
        // The pod's name (and, in a multi-context window, its context) on the left;
        // the tab and view-toggle hints on the right.
        //
        // As the panel narrows, the name gives way first: it takes only the
        // space the hints leave, ellipsizing, down to a floor that keeps a few
        // characters readable. Past that floor the hints shrink instead and
        // wrap onto further rows, each hint kept whole.
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(space.control_gap)
            .px(space.panel_inset)
            .py(space.control_gap)
            .bg(crate::ui::style::surface_raised(cx))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().flex_1().min_w(rems(8.)).child(heading))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .min_w_0()
                    .gap_x(space.control_gap)
                    .gap_y_1()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .when(!yaml, |this| {
                        this.children(DetailSection::ALL.iter().map(|section| {
                            div()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap_1()
                                .whitespace_nowrap()
                                .child(tab_key(*section))
                                .child(section.label())
                        }))
                    })
                    .when(has_links, |this| {
                        this.child(
                            div()
                                .id("go-to-hint")
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap_1()
                                .whitespace_nowrap()
                                .child(crate::ui::link::go_to_key(window))
                                .child("Go to…")
                                .test_support(),
                        )
                    })
                    .child(toggle_hint.flex_shrink_0().whitespace_nowrap()),
            );

        Self::with_window_actions(div(), cx)
            .size_full()
            .key_context(key_context())
            .on_action(cx.listener(Self::on_action_go_to))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_toggle_view))
            .on_action(cx.listener(Self::on_action_fold_all))
            .on_action(cx.listener(Self::on_action_unfold_all))
            .on_action(cx.listener(Self::on_action_select_overview_tab))
            .on_action(cx.listener(Self::on_action_select_containers_tab))
            .on_action(cx.listener(Self::on_action_select_configuration_tab))
            .on_action(cx.listener(Self::on_action_hide_secret_values))
            .on_action(cx.listener(Self::on_action_select_volumes_tab))
            .on_action(cx.listener(Self::on_action_select_events_tab))
            .on_action(cx.listener(Self::on_action_select_managed_fields_tab))
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().child(content))
            // Tab stays in the panel: see `ui::panel::focus`.
            .focus_trap("pod-detail-panel-tab-trap", &self.focus_handle)
    }
}

/// The panel's own key context plus the shared one `links.go_to` is gated to,
/// so `g` reaches this panel without the link module knowing it exists.
fn key_context() -> KeyContext {
    let mut context = KeyContext::default();
    context.add(PANEL_KEY_CONTEXT);
    context.add(crate::ui::link::LINKS_KEY_CONTEXT);
    context
}
