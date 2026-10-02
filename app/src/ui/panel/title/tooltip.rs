//! The tooltip a panel's title element carries: which lines it shows, and the
//! element that shows them. The title text itself is `super`'s.

use super::PanelScope;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyView, App, InteractiveElement as _, ParentElement as _, Styled as _, Window, div,
};

/// A title's tooltip lines: the API group for a custom resource list, whose
/// title leaves the group out, then "Context: <name>".
pub(super) fn tooltip_lines(scope: &PanelScope) -> (Option<String>, String) {
    (
        scope.target.custom_group().map(str::to_string),
        format!("Context: {}", scope.context_name),
    )
}

/// The tooltip [`super::title_element`] shows, one line per fact. Each line carries a
/// debug selector, so a test that hovers the tab can see which ones it shows.
pub(super) fn title_tooltip(scope: &PanelScope, window: &mut Window, cx: &mut App) -> AnyView {
    let (group, context) = tooltip_lines(scope);
    Tooltip::element(move |_window, _cx| {
        div()
            .flex()
            .flex_col()
            .children(group.clone().map(|group| {
                div()
                    .debug_selector(|| "panel-title-tooltip-group".into())
                    .child(group)
            }))
            .child(
                div()
                    .debug_selector(|| "panel-title-tooltip-context".into())
                    .child(context.clone()),
            )
    })
    .build(window, cx)
}
