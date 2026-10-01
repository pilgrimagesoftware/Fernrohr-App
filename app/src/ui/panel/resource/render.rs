//! Everything the Resource panel draws: the header/cluster dropdown, the
//! grouped, filterable kind list, and the status views (connecting, loading,
//! empty, failed). [`super::actions`] answers the keyboard; this file only
//! draws what `super::ResourcePanel`'s state says is true right now.

use super::section::VisibleSection;
use super::{ResourcePanel, ResourceState};
use crate::ui::nav::NavTarget;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::sidebar::{Sidebar, SidebarItem as _, SidebarMenuItem};
use gpui_kit::component::{Icon, IconName};
use gpui_kit::*;

impl ResourcePanel {
    /// The sidebar's rows while there is nothing to list yet - a plain
    /// non-interactive row per state, so the panel's width and placement stay
    /// the same whether or not discovery has landed.
    fn status_row(label: String) -> SidebarMenuItem {
        SidebarMenuItem::new(label).disable(true)
    }

    /// One row's `SidebarMenuItem`: a click sets it highlighted (section
    /// 4.1's "a click selects the same way"), and a second click - or the
    /// context menu's "Open" - opens it through the same [`Self::request_open`]
    /// path `Enter` uses.
    fn kind_item(
        &self,
        label: String,
        target: NavTarget,
        active: bool,
        cx: &Context<Self>,
    ) -> SidebarMenuItem {
        let this = cx.weak_entity();
        let click_target = target.clone();
        let menu_target = target.clone();
        let menu_panel = this.clone();
        let highlighted = self.highlighted.as_ref() == Some(&target);
        SidebarMenuItem::new(label)
            .icon(target.icon())
            .active(active || highlighted)
            .on_click(move |event, _window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.set_highlighted(Some(click_target.clone()), cx);
                    if event.click_count() >= 2 {
                        this.request_open(click_target.clone(), cx);
                    }
                });
            })
            .context_menu(move |menu, _window, _cx| {
                let target = menu_target.clone();
                let panel = menu_panel.clone();
                menu.item(
                    PopupMenuItem::new("Open").on_click(move |_event, _window, cx| {
                        let _ = panel.update(cx, |this, cx| this.request_open(target.clone(), cx));
                    }),
                )
            })
    }

    /// One category's header - name, running count, and a click that toggles
    /// its collapse the same way Left/Right do - over its rows, when expanded.
    fn render_section(
        &self,
        section: &VisibleSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let this = cx.weak_entity();
        let category = section.category;
        let expanded = section.expanded;

        let header = div()
            .id(format!("resource-section-{}", category.title()))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .h_7()
            .rounded(theme.radius)
            .bg(crate::ui::style::surface_raised(cx))
            .text_xs()
            .text_color(theme.sidebar_foreground.opacity(0.7))
            .hover(|el| el.bg(theme.sidebar_accent.opacity(0.5)))
            .on_click(move |_event, _window, cx| {
                let _ = this.update(cx, |this, cx| this.toggle_section(category, cx));
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size_4(),
                    )
                    .child(category.title()),
            )
            .child(div().child(section.total.to_string()));

        let mut rows = div().flex().flex_col().w_full();
        if expanded {
            for (index, (label, target, active)) in
                self.rows(&section.matches).into_iter().enumerate()
            {
                let tooltip = match &target {
                    NavTarget::Kind(kind) => super::api_version_label(kind),
                    _ => String::new(),
                };
                let item = self.kind_item(label, target, active, cx);
                let id = format!("resource-row-{}-{index}", category.title());
                rows = rows.child(
                    div()
                        .id(SharedString::from(format!("{id}-tip")))
                        .child(item.render(id, window, cx))
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                                .build(window, cx)
                        }),
                );
            }
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .child(rows)
            .into_any_element()
    }

    /// The grouped, filterable list: a scrollable stack of sections over a
    /// bottom-pinned filter box and keyboard hint row. Rows are rendered
    /// directly rather than through `Sidebar`'s own virtualized list, since
    /// that list takes one homogeneous item type and cannot interleave a
    /// section header between two rows - not a cost worth paying at the
    /// couple-hundred-row scale a cluster's discovery reports.
    fn render_kinds(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let filter = self.filter_text(cx);
        let sections = self.visible_sections(cx);

        let list: AnyElement = if sections.is_empty() {
            div()
                .p_3()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(format!("No resource kinds match \u{201c}{filter}\u{201d}."))
                .into_any_element()
        } else {
            let mut list = div().flex().flex_col().w_full();
            for section in &sections {
                list = list.child(self.render_section(section, window, cx));
            }
            list.into_any_element()
        };

        let body = div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_1().min_h_0().overflow_y_scrollbar().child(list))
            .child(
                div()
                    .w_full()
                    .px_2()
                    .py_2()
                    .bg(crate::ui::style::surface_raised(cx))
                    .border_t_1()
                    .border_color(theme.sidebar_border)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Input::new(&self.filter_input))
                    .child(super::keyboard::hint_row(window, cx)),
            );

        self.with_header(body.into_any_element(), cx)
    }

    /// The panel's frame: [`Self::header`] drawn here, above header-less content,
    /// rather than in `Sidebar`'s own header slot. That slot is a padded row this panel
    /// can't size, which clipped the selector at the right edge and let it collapse to
    /// nothing when the panel was narrowed. The frame carries the sidebar's background
    /// and right border (the sidebar's own is turned off) so the two read as one panel.
    fn with_header(&self, content: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.tokens.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .child(div().w_full().px_3().pt_3().child(self.header(cx)))
            .child(div().flex_1().min_h_0().child(content))
            .into_any_element()
    }

    /// The header row: which cluster's resources these rows are, and - once the window
    /// holds more than one - a dropdown of every context it uses (`window-context-bar`
    /// design.md decision 4). Picking one only *asks*: it emits
    /// [`super::ResourceEvent::SwitchContext`], so `active` is written in one place,
    /// `MainWindow`. "Resources" gives way (ellipsized) before the selector does, and
    /// the selector sits in the right corner on the label's text baseline.
    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let selector = if self.shows_cluster_dropdown() {
            self.cluster_dropdown(cx)
        } else {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(self.context_name.clone())
                .into_any_element()
        };
        div()
            .w_full()
            .flex()
            .items_baseline()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_sm()
                    .text_color(theme.sidebar_foreground)
                    .child("Resources"),
            )
            .child(div().flex_shrink_0().child(selector))
            .into_any_element()
    }

    fn cluster_dropdown(&self, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.weak_entity();
        let contexts = self.contexts.clone();
        let current = self.context_name.clone();
        Button::new("resource-cluster")
            .label(self.context_name.clone())
            .icon(gpui_kit::assets::IconName::ChevronDown)
            .xsmall()
            .ghost()
            .dropdown_menu(move |menu, _window, _cx| {
                // Built per open, not hoisted: `PopupMenuItem` is not `Clone`, and
                // this closure is `Fn` so it can run more than once - the same
                // shape as `ui/picker_tunnel.rs::selector`.
                let mut menu = menu;
                for context_name in &contexts {
                    let picked = context_name.clone();
                    let panel = this.clone();
                    menu = menu.item(
                        PopupMenuItem::new(context_name.clone())
                            .checked(*context_name == current)
                            .on_click(move |_event, _window, cx| {
                                let _ = panel.update(cx, |_panel, cx| {
                                    cx.emit(super::ResourceEvent::SwitchContext(picked.clone()));
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// A status-only sidebar (no kinds yet, or none to show), framed like the list.
    fn render_status(&self, message: String, cx: &mut Context<Self>) -> AnyElement {
        let sidebar = Sidebar::new("resources")
            .w_full()
            .border_r_0()
            .collapsible(false)
            .child(Self::status_row(message))
            .into_any_element();
        self.with_header(sidebar, cx)
    }
}

impl Render for ResourcePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.state {
            ResourceState::Loaded(kinds) if kinds.is_empty() => {
                self.render_status("This cluster reported no resource kinds.".to_string(), cx)
            }
            ResourceState::Loaded(_) => self.render_kinds(window, cx),
            ResourceState::WaitingForConnection => {
                self.render_status("Connecting...".to_string(), cx)
            }
            ResourceState::Loading => {
                self.render_status("Discovering resource kinds...".to_string(), cx)
            }
            ResourceState::Failed(reason) => {
                self.render_status(format!("Could not discover resource kinds: {reason}"), cx)
            }
        };

        div()
            .size_full()
            .key_context(super::keyboard::PANEL_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_select_next))
            .on_action(cx.listener(Self::on_action_select_previous))
            .on_action(cx.listener(Self::on_action_open_selected))
            .on_action(cx.listener(Self::on_action_collapse_section))
            .on_action(cx.listener(Self::on_action_expand_section))
            .on_action(cx.listener(Self::on_action_focus_filter))
            .on_action(cx.listener(Self::on_action_clear_filter))
            .child(content)
    }
}
