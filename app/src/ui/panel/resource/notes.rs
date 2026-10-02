//! What the panel draws about discovery itself (`discovery-resilience`): the
//! failure state when the cluster's API groups couldn't be listed at all, the
//! warning row when some groups couldn't be read, and the header's refresh
//! button. Every message wraps and is selectable, so the whole of it can be read
//! and copied - never a clipped one-line row.

use super::ResourcePanel;
use super::discovery::RefreshResources;
use crate::ui::panel_title;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, IconName};
use gpui_kit::*;

impl ResourcePanel {
    /// Listing the API groups failed: the readable message, then the full detail,
    /// both wrapping and selectable, under the header with its refresh button.
    pub(super) fn render_failure(
        &self,
        message: &str,
        detail: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let space = crate::ui::space::spacing(cx);
        let body = div()
            .id("resource-discovery-failure")
            .debug_selector(|| "resource-discovery-failure".into())
            .size_full()
            .overflow_y_scroll()
            .p(space.panel_inset)
            .child(panel_title::error_content(
                format!("Could not discover resource kinds. {message}"),
                Some(detail.to_string()),
                cx,
            ))
            .into_any_element();
        self.with_header(body, cx)
    }

    /// "N API groups unavailable", when the last discovery couldn't read some; a
    /// click (or Space, as a button) expands it to each group and why.
    pub(super) fn unavailable_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let unavailable = &self.notes.unavailable;
        if unavailable.is_empty() {
            return None;
        }
        let theme = cx.theme().clone();
        let count = unavailable.len();
        let summary = if count == 1 {
            "1 API group unavailable".to_string()
        } else {
            format!("{count} API groups unavailable")
        };
        let this = cx.weak_entity();
        let toggle = Button::new("resource-unavailable-toggle")
            .ghost()
            .xsmall()
            .icon(if self.notes.expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .label(summary)
            .on_click(move |_event, _window, cx| {
                let _ = this.update(cx, |panel, cx| panel.toggle_unavailable(cx));
            });
        let list = self.notes.expanded.then(|| {
            div()
                .debug_selector(|| "resource-unavailable-list".into())
                .flex()
                .flex_col()
                .gap_1()
                .pl_4()
                .children(unavailable.iter().map(|group| {
                    let name = if group.group.is_empty() {
                        "core".to_string()
                    } else {
                        group.group.clone()
                    };
                    panel_title::error_content(name, Some(group.reason.clone()), cx)
                }))
        });
        Some(
            div()
                .debug_selector(|| "resource-unavailable".into())
                .flex()
                .flex_col()
                .gap_1()
                .py_1()
                .text_xs()
                .text_color(theme.warning)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(Icon::new(IconName::TriangleAlert).xsmall())
                        .child(toggle),
                )
                .children(list)
                .into_any_element(),
        )
    }
}

/// The header's refresh control: Resources: Refresh's mouse route.
pub(super) fn refresh_button(discovering: bool) -> Button {
    Button::new("resource-panel-refresh")
        .icon(IconName::RefreshCw)
        .xsmall()
        .ghost()
        .loading(discovering)
        .tooltip("Refresh resources")
        .on_click(|_event, window, cx| window.dispatch_action(Box::new(RefreshResources), cx))
}
