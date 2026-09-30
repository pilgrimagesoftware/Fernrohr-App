//! Everything the Resource panel draws: the header/cluster dropdown, the
//! grouped, collapsible kind list, and the status views (connecting,
//! loading, empty, failed).

use super::section::Section;
use super::{ResourcePanel, ResourceState};
use crate::ui::nav::NavTarget;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
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

    /// One row's `SidebarMenuItem`: a double click - or the context menu's
    /// "Open" - opens it through [`ResourcePanel::request_open`] (9.1/9.2).
    fn kind_item(
        &self,
        label: String,
        target: NavTarget,
        active: bool,
        cx: &Context<Self>,
    ) -> SidebarMenuItem {
        let this = cx.weak_entity();
        let dbl = this.clone();
        let opened = target.clone();
        let menu_target = target.clone();
        let menu_panel = this.clone();
        SidebarMenuItem::new(label)
            .icon(target.icon())
            .active(active)
            // A single click only arms the row; the second click of a
            // double-click opens the panel (9.1).
            .on_click(move |event, _window, cx| {
                if event.click_count() < 2 {
                    return;
                }
                let _ = dbl.update(cx, |this, cx| this.request_open(opened.clone(), cx));
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

    /// One category's header - name, running count, and a click that
    /// collapses or expands its rows (section 2.2) - over its rows, when
    /// expanded.
    fn render_section(
        &self,
        section: &Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let this = cx.weak_entity();
        let category = section.category;
        let expanded = !self.collapsed.contains(&category);

        let header = div()
            .id(format!("resource-section-{}", category.title()))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .h_7()
            .rounded(theme.radius)
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
            .child(div().child(section.kinds.len().to_string()));

        let mut rows = div().flex().flex_col().w_full();
        if expanded {
            for (index, (label, target, active)) in
                self.rows(&section.kinds).into_iter().enumerate()
            {
                let item = self.kind_item(label, target, active, cx);
                let id = format!("resource-row-{}-{index}", category.title());
                rows = rows.child(item.render(id, window, cx).into_any_element());
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

    /// The grouped, collapsible list: a stack of section headers over their
    /// rows. Rows are rendered directly rather than through `Sidebar`'s own
    /// virtualized list, since that list takes one homogeneous item type and
    /// cannot interleave a section header between two rows - not a cost worth
    /// paying at the couple-hundred-row scale a cluster's discovery reports.
    fn render_kinds(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let sections = self.sections();
        let mut list = div().flex().flex_col().w_full();
        for section in &sections {
            list = list.child(self.render_section(section, window, cx));
        }
        self.with_header(list.into_any_element(), cx)
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
        match &self.state {
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
        }
    }
}
