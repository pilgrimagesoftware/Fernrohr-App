//! The bulk action bar: shown above a list while rows are checked, it counts
//! them and offers each action the checked set is offered as an icon button
//! with a tooltip (`icon-buttons.md`). A button dispatches the action's own
//! command, so the bar, the palette and any key a user binds run the same thing.

use super::commands::{ClearChecked, command};
use crate::k8s::resource::bulk::BulkAction;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

/// The bar's selector, and its Uncheck All button's id.
pub(crate) const BAR_SELECTOR: &str = "bulk-bar";
pub(crate) const CLEAR_ID: &str = "bulk-clear";

/// `action`'s button id: `bulk-delete`, `bulk-copy_names`.
pub(crate) fn button_id(action: BulkAction) -> SharedString {
    let (id, _, _) = command(action);
    format!("bulk-{}", id.trim_start_matches("bulk.")).into()
}

fn icon(action: BulkAction) -> IconName {
    match action {
        BulkAction::ViewLogs => IconName::ScrollText,
        BulkAction::RestartRollout => IconName::RotateCw,
        BulkAction::RollbackRollout => IconName::Undo2,
        BulkAction::Scale => IconName::ChevronsUpDown,
        BulkAction::Cordon => IconName::Ban,
        BulkAction::Uncordon => IconName::CircleCheck,
        BulkAction::Drain => IconName::ArrowDownToLine,
        BulkAction::Label => IconName::Tag,
        BulkAction::Annotate => IconName::StickyNote,
        BulkAction::CopyNames => IconName::Copy,
        BulkAction::CopyYaml => IconName::FileCode,
        BulkAction::Delete => IconName::Trash,
    }
}

/// The bar for `count` checked rows offered `offered`.
pub(crate) fn render(count: usize, offered: &[BulkAction], cx: &App) -> AnyElement {
    let space = crate::ui::space::spacing(cx);
    let buttons = offered
        .iter()
        .map(|action| action_button(*action))
        .collect::<Vec<_>>();
    div()
        .id(BAR_SELECTOR)
        .debug_selector(|| BAR_SELECTOR.into())
        .flex()
        .flex_wrap()
        .items_center()
        .gap_1()
        .mb(space.control_gap)
        .px(space.panel_inset)
        .py_1()
        .rounded_md()
        .bg(cx.theme().secondary)
        .child(
            div()
                .mr(space.control_gap)
                .text_sm()
                .child(format!("{count} checked")),
        )
        .children(buttons)
        .child(div().flex_1())
        .child(
            Button::new(CLEAR_ID)
                .icon(IconName::X)
                .xsmall()
                .ghost()
                .tooltip_with_action("Uncheck all", &ClearChecked, None)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ClearChecked), cx)),
        )
        .into_any_element()
}

fn action_button(action: BulkAction) -> Button {
    let (_, _, boxed) = command(action);
    let dispatched = boxed.boxed_clone();
    Button::new(button_id(action))
        .icon(icon(action))
        .xsmall()
        .ghost()
        .tooltip_with_action(action.label(), &*boxed, None)
        .on_click(move |_, window, cx| window.dispatch_action(dispatched.boxed_clone(), cx))
}
