//! The Forwards column's cell (`port-forward-indicators` 2.1): for an object with
//! active port-forwards, a forward icon and their count, with a tooltip listing
//! each `127.0.0.1:<local> → <target port>`; nothing for one with none. Shared by
//! the Pods and Services lists.

use crate::k8s::cluster::port_forwards::ForwardSummary;
use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::*;

/// The debug selector of `name`'s indicator, for tests.
pub fn selector(name: &str) -> String {
    format!("forward-indicator {name}")
}

/// The tooltip's lines, one per forward.
pub fn tooltip_text(forwards: &[ForwardSummary]) -> String {
    forwards
        .iter()
        .map(|forward| format!("{} \u{2192} {}", forward.local_addr, forward.target_port))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `name`'s cell: the icon and count with their tooltip, or `None` with no forwards.
pub fn indicator(name: &str, forwards: &[ForwardSummary]) -> Option<AnyElement> {
    if forwards.is_empty() {
        return None;
    }
    let tooltip = SharedString::from(tooltip_text(forwards));
    let selector = selector(name);
    Some(
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector)
            .flex()
            .items_center()
            .gap_1()
            .child(Icon::new(IconName::ArrowLeftRight).xsmall())
            .child(forwards.len().to_string())
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .into_any_element(),
    )
}
