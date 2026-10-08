//! What a saved panel comes back as when its layout state can't be read - a
//! hand-edited or corrupted workspace file - instead of the app panicking at
//! launch (`restore-no-panic`). The rest of the layout restores around it.
//!
//! [`restore_with`] wraps every panel kind's restore: the kind's own reader
//! returns `Err(reason)` for state it can't use, and this logs the reason and
//! stands an [`UnrestoredPanel`] in its place. The panel keeps the original
//! state and saves it back unchanged, as gpui-kit's own placeholder does for a
//! panel name it doesn't know, so nothing in the file is lost - a later build,
//! or a fix to the file, can still restore it.

use crate::ui::panel_title;
use gpui_kit::base::dock::PanelView;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelBuildContext, PanelEvent, PanelInfo, PanelState, panel_handle,
    register_panel,
};
use gpui_kit::*;
use serde_json::Value;
use std::sync::Arc;

/// The panel kind this placeholder registers under. `restore_with`'s own
/// fallback never writes it to a saved layout - [`UnrestoredPanel::dump`]
/// always answers with the panel it stands in for - but `saved_layouts::load`
/// (section 5.1) does write it, to wrap a panel scoped to a context a window
/// doesn't hold, which [`register_restore`] is what reads back.
pub(crate) const PANEL_NAME: &str = "Unrestored";

/// Restores the panel `context` describes with `restore`, or - when its state
/// isn't a panel's, or `restore` can't read it - an [`UnrestoredPanel`] saying
/// why, after logging the same.
pub fn restore_with(
    context: &PanelBuildContext,
    cx: &mut App,
    restore: impl FnOnce(&Value, &mut App) -> Result<Arc<dyn PanelView>, String>,
) -> Arc<dyn PanelView> {
    let state = context.state();
    let result = match context.info() {
        PanelInfo::Panel(data) => restore(data, cx),
        _ => Err("its layout state isn't a panel's".to_string()),
    };
    result.unwrap_or_else(|reason| {
        log::warn!(
            "couldn't restore a saved {} panel ({reason}); showing a placeholder",
            state.panel_name
        );
        let state = state.clone();
        panel_handle(cx.new(|cx| UnrestoredPanel::new(state, reason, cx)))
    })
}

/// `value` as a string, or an `Err` naming `field` as missing.
pub fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("its state has no `{field}`"))
}

/// Registers how a saved [`PANEL_NAME`] panel comes back: unwraps straight to
/// the [`UnrestoredPanel`] it already was (`reason` and the wrapped `original`
/// state `saved_layouts::load` saved it with, design.md D5), rather than
/// reinterpreting the wrapper through a kind-specific reader - there is no
/// kind-specific data here to misread, only this module's own shape. Still
/// goes through [`restore_with`], so a wrapper a hand-edited file corrupted
/// becomes a (less specific) placeholder instead of failing the whole load.
pub fn register_restore(cx: &mut App) {
    register_panel(cx, PANEL_NAME, |context, _window, cx| {
        restore_with(&context, cx, |data, cx| {
            let reason = required_str(data, "reason")?.to_string();
            let original: PanelState = serde_json::from_value(data["original"].clone())
                .map_err(|_| "its wrapped state isn't readable".to_string())?;
            Ok(panel_handle(
                cx.new(|cx| UnrestoredPanel::new(original, reason, cx)),
            ))
        })
    });
}

/// A saved panel whose state couldn't be restored: it says which kind of panel
/// and why, and keeps the state to save back unchanged.
pub struct UnrestoredPanel {
    state: PanelState,
    reason: String,
    focus_handle: FocusHandle,
}

impl UnrestoredPanel {
    pub fn new(state: PanelState, reason: String, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            reason,
            focus_handle: crate::ui::panel::focus::panel_focus_handle(cx),
        }
    }

    /// The saved panel kind it stands in for.
    #[cfg(test)]
    pub fn panel_kind(&self) -> &str {
        &self.state.panel_name
    }

    /// Why it couldn't be restored.
    #[cfg(test)]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl Focusable for UnrestoredPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for UnrestoredPanel {}

impl BasePanel for UnrestoredPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    /// The original state, so saving the layout doesn't lose the panel.
    fn dump(&self, _cx: &App) -> PanelState {
        self.state.clone()
    }
}

impl Panel for UnrestoredPanel {
    fn title(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(format!("{} (not restored)", self.state.panel_name))
            .child(panel_title::close_button(cx.entity()))
    }
}

impl Render for UnrestoredPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .p(space.panel_inset)
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .debug_selector(|| "unrestored-panel".into())
            .child(format!(
                "This {} panel couldn't be restored from the saved layout.",
                self.state.panel_name
            ))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Reason: {}. Close it to remove it.", self.reason)),
            )
    }
}

#[cfg(test)]
mod tests;
