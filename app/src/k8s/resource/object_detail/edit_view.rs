//! Drawing a YAML edit: the editor, the last save's refusal above it, and the
//! keys that save and cancel, read from the live keymap.

use super::ObjectDetailPanel;
use super::commands::{
    CANCEL_EDIT_KEY, CancelObjectEdit, EDIT_KEY_CONTEXT, SAVE_EDIT_KEY, SaveObjectEdit,
};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::Editor;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The refusal banner's debug selector, and the edit notice's.
pub(super) const EDIT_FAILURE: &str = "object-edit-failure";
pub(super) const EDIT_NOTICE: &str = "object-edit-notice";

impl ObjectDetailPanel {
    /// The edit in progress, or `None` when the object isn't being edited.
    pub(super) fn render_edit(&self, window: &Window, cx: &App) -> Option<AnyElement> {
        let edit = self.edit.as_ref()?;
        let space = crate::ui::space::spacing(cx);
        let bad = crate::ui::style::status(crate::ui::style::Tone::Bad, cx);
        let key = |action: &dyn Action, literal: &str| {
            Kbd::binding_for_action(action, Some(EDIT_KEY_CONTEXT), window)
                .unwrap_or_else(|| Kbd::new(Keystroke::parse(literal).expect("valid keybinding")))
        };
        let hint = |key: Kbd, label: &'static str| {
            div().flex().items_center().gap_1().child(key).child(label)
        };
        Some(
            div()
                .size_full()
                .p(space.panel_inset)
                .flex()
                .flex_col()
                .gap(space.control_gap)
                .children(edit.failure.as_ref().map(|failure| {
                    div()
                        .debug_selector(|| EDIT_FAILURE.into())
                        .flex()
                        .flex_col()
                        .px(space.panel_inset)
                        .py(space.control_gap)
                        .rounded_md()
                        .border_1()
                        .border_color(bad)
                        .child(
                            div()
                                .text_color(bad)
                                .child(format!("Not saved: {}", failure.message)),
                        )
                        .when(!failure.detail.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(failure.detail.clone()),
                            )
                        })
                }))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .child(Editor::new(&edit.editor).h_full()),
                )
                .child(
                    div()
                        .flex()
                        .gap_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(hint(key(&SaveObjectEdit, SAVE_EDIT_KEY), "Save"))
                        .child(hint(key(&CancelObjectEdit, CANCEL_EDIT_KEY), "Cancel"))
                        .when(edit.saving, |this| this.child("Saving…")),
                )
                .into_any_element(),
        )
    }

    /// Why Edit did nothing, above the content.
    pub(super) fn render_edit_notice(&self, cx: &App) -> Option<AnyElement> {
        let notice = self.edit_notice.clone()?;
        let space = crate::ui::space::spacing(cx);
        Some(
            div()
                .debug_selector(|| EDIT_NOTICE.into())
                .px(space.panel_inset)
                .py(space.control_gap)
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(notice)
                .into_any_element(),
        )
    }
}
