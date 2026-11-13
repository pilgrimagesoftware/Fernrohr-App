//! What a panel shows of its actions: a banner while one runs or after one
//! failed, and the icon buttons that start them - disabled, with the reason in
//! the tooltip, when `helm` or the connection isn't available
//! (`.claude/rules/icon-buttons.md`).

use super::flow::{ActionHost, ActionState};
use crate::k8s::resource::delete_flow::refusal;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;

/// The running line's and the refusal banner's ids, and the banner's
/// Dismiss button's.
pub(crate) struct BannerIds {
    pub(crate) running: &'static str,
    pub(crate) refusal: &'static str,
    pub(crate) dismiss: &'static str,
}

/// The banner for `state`, if any: what is running, else the last refusal
/// with a Dismiss button that clears it on `panel`.
pub(crate) fn status_banner<T: ActionHost>(
    state: &ActionState,
    panel: WeakEntity<T>,
    ids: &BannerIds,
    cx: &App,
) -> Option<AnyElement> {
    if let Some(running) = &state.running {
        let id = ids.running;
        return Some(
            div()
                .debug_selector(move || id.into())
                .text_color(cx.theme().muted_foreground)
                .child(format!("{running}\u{2026}"))
                .into_any_element(),
        );
    }
    let refused = state.refusal.clone()?;
    Some(refusal::render(
        refused,
        ids.refusal,
        ids.dismiss,
        move |cx| {
            let _ = panel.update(cx, |this, cx| {
                this.action_state().refusal = None;
                cx.notify();
            });
        },
        cx,
    ))
}

/// An action's icon button: `label` with its key from the live keymap as the
/// tooltip, running `on_click`; or, while `unavailable` says why, disabled
/// with that reason as the tooltip.
pub(crate) fn action_button(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    action: &dyn Action,
    context: &'static str,
    unavailable: Option<String>,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let button = Button::new(id).icon(icon).xsmall().ghost();
    match unavailable {
        None => button
            .tooltip_with_action(label, action, Some(context))
            .on_click(move |_, window, cx| on_click(window, cx))
            .into_any_element(),
        Some(reason) => crate::ui::icon_tooltip::with_tooltip_text(
            SharedString::from(format!("{id}-disabled")),
            format!("{label} isn't available: {reason}"),
            button.disabled(true),
        )
        .into_any_element(),
    }
}
