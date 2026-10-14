//! A Node's taints: checking a new one from Add Taint's prompt, and choosing
//! which of several Remove Taint removes - one button per taint, as the port
//! picker offers one per port.

use crate::k8s::resource::kind_actions::{Taint, TaintEffect};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::*;
use std::rc::Rc;

/// The chooser's Cancel, and each taint's button.
pub(crate) const CANCEL_TAINT_ID: &str = "kind-taint-cancel";
pub(crate) fn taint_button_id(index: usize) -> SharedString {
    format!("kind-taint-{index}").into()
}

/// The taint Add Taint's fields describe, or why not: the key as a label key
/// is checked, the value may be empty, the effect is one of three - and the
/// node mustn't have that key and effect already, which `kubectl taint` refuses
/// without `--overwrite`.
pub(crate) fn parse(
    key: &str,
    value: &str,
    effect: &str,
    existing: &[Taint],
) -> Result<Taint, String> {
    crate::ui::bulk::check_key(key)?;
    let effect: TaintEffect = effect.parse()?;
    let value = (!value.is_empty()).then_some(value);
    let taint = Taint::new(key, value, effect);
    if existing.iter().any(|other| other.same_as(&taint)) {
        return Err(format!(
            "The node already has a {key}:{effect} taint. Remove it first to change it."
        ));
    }
    Ok(taint)
}

/// Runs `chosen` with the taint to remove: at once with only one, else after
/// asking which.
pub(crate) fn choose(
    taints: Vec<Taint>,
    chosen: impl Fn(Taint, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    if let [only] = taints.as_slice() {
        return chosen(only.clone(), window, cx);
    }
    let chosen = Rc::new(chosen);
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let mut footer = DialogFooter::new();
        for (index, taint) in taints.iter().enumerate() {
            let (chosen, taint) = (chosen.clone(), taint.clone());
            footer = footer.child(
                Button::new(taint_button_id(index))
                    .label(taint.label())
                    .on_click(move |_event, window, cx| {
                        window.close_dialog(cx);
                        chosen(taint.clone(), window, cx);
                    }),
            );
        }
        dialog
            .title("Remove Which Taint?")
            .child("The node has more than one taint.")
            .footer(
                footer.child(
                    Button::new(CANCEL_TAINT_ID)
                        .label("Cancel")
                        .ghost()
                        .on_click(|_event, window, cx| window.close_dialog(cx)),
                ),
            )
    });
}
