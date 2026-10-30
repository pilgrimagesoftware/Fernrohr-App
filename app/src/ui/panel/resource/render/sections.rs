//! A section's and a Custom Resources subgroup's collapsible header, over its
//! rows while expanded. The header shows how many kinds it holds only while
//! collapsed (#246); open, the rows say so themselves. The rows are
//! `super`'s.

use super::super::ResourcePanel;
use super::super::section::{VisibleSection, VisibleSubgroup};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::{Icon, IconName};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl ResourcePanel {
    /// One API group inside Custom Resources: an indented header - chevron,
    /// group name (`core` for the core group), and while collapsed its count -
    /// whose click toggles it, over its rows while expanded (`custom-resource-grouping` 2.1/2.2).
    pub(super) fn render_subgroup(
        &self,
        subgroup: &VisibleSubgroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let this = cx.weak_entity();
        let group = subgroup.group.clone();
        let name = if group.is_empty() {
            "core".to_string()
        } else {
            group.clone()
        };
        let selector = format!("resource-subgroup-{name}");
        let highlighted = self.highlighted.as_ref()
            == Some(&super::super::keyboard::Cursor::Subgroup(group.clone()));
        let header = div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .test_support()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pl_4()
            .pr_2()
            .h_6()
            .rounded(theme.radius)
            .text_xs()
            .text_color(theme.sidebar_foreground.opacity(0.7))
            .when(highlighted, |el| {
                el.bg(theme.sidebar_accent)
                    .text_color(theme.sidebar_accent_foreground)
            })
            .hover(|el| el.bg(theme.sidebar_accent.opacity(0.5)))
            .on_click(move |_event, _window, cx| {
                let _ = this.update(cx, |this, cx| this.toggle_subgroup(&group, cx));
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Icon::new(if subgroup.expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size_3(),
                    )
                    .child(name.clone()),
            )
            // #246: the count only while collapsed - open, the rows show what's
            // in it. Left out, not hidden, so it leaves no gap.
            .when(!subgroup.expanded, |el| {
                let count = subgroup_count_selector(&name);
                el.child(
                    div()
                        .debug_selector(move || count.clone())
                        .child(subgroup.matches.len().to_string()),
                )
            });
        let rows = if subgroup.expanded {
            self.render_rows(&subgroup.matches, &format!("subgroup-{name}"), window, cx)
        } else {
            div()
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .child(rows)
            .into_any_element()
    }

    /// One category's header - name, its count while collapsed, and a click that
    /// toggles its collapse the same way Left/Right do - over its rows, when
    /// expanded.
    pub(in crate::ui::panel::resource) fn render_section(
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
            .test_support()
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
            // #246: the count only while collapsed - open, the rows show what's
            // in it. Left out, not hidden, so it leaves no gap.
            .when(!expanded, |el| {
                let count = section_count_selector(category.title());
                el.child(
                    div()
                        .debug_selector(move || count.clone())
                        .child(section.total.to_string()),
                )
            });

        let mut rows = div().flex().flex_col().w_full();
        if expanded && section.subgroups.is_empty() {
            rows = rows.child(self.render_rows(&section.matches, category.title(), window, cx));
        } else if expanded {
            for subgroup in &section.subgroups {
                rows = rows.child(self.render_subgroup(subgroup, window, cx));
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
}

/// The debug selector of a collapsed section's count.
pub(in crate::ui::panel::resource) fn section_count_selector(title: &str) -> String {
    format!("resource-section-count-{title}")
}

/// The debug selector of a collapsed subgroup's count.
pub(in crate::ui::panel::resource) fn subgroup_count_selector(name: &str) -> String {
    format!("resource-subgroup-count-{name}")
}
