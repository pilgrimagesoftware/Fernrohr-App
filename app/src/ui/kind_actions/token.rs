//! Create Token's result: a ServiceAccount's new token, shown once in a dialog
//! the way a revealed Secret value is shown (`ui::detail::secret_key_row`), with
//! Hide and Copy. Hiding it, or closing the dialog any way, drops it: the token
//! is held only as a [`SecretValue`], never in a title, a notification, a log
//! line or a file, and a new one needs Create Token again.

use super::flow::send;
use super::host::{KindHost, KindTarget};
use crate::k8s::resource::kind_actions;
use crate::k8s::resource::secret_value::{Reveal, SecretValue};
use gpui_kit::component::WindowExt as _;
use gpui_kit::*;

/// The token row's Hide button and value element ids, and the row's selector.
pub(crate) const TOKEN_HIDE_ID: &str = "kind-token-hide";
pub(crate) const TOKEN_VALUE_ID: &str = "kind-token-value";
pub(crate) const TOKEN_SELECTOR: &str = "kind-token";

/// Requests a token for `target`, then shows it.
pub(super) fn create<T: KindHost>(
    target: KindTarget,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let (namespace, name) = (target.namespace().to_string(), target.name.clone());
    let title = format!("Token for {}", target.noun());
    send(
        &target,
        format!("Create a token for {}", target.noun()),
        panel,
        &move |client| {
            let (namespace, name) = (namespace.clone(), name.clone());
            async move { kind_actions::create_token(client, &namespace, &name).await }
        },
        move |token, window, cx| show(title.clone(), token, window, cx),
        window,
        cx,
    );
}

/// The dialog holding `token`. It lives in the dialog's builder alone, so it is
/// gone once the dialog closes.
fn show(title: String, token: SecretValue, window: &mut Window, cx: &mut App) {
    let reveal = Reveal::Shown(token);
    window.open_dialog(cx, move |dialog, _window, cx| {
        let row = crate::ui::detail::secret_key_row(
            "token",
            0,
            Some(&reveal),
            ElementId::Name(TOKEN_HIDE_ID.into()),
            ElementId::Name(TOKEN_VALUE_ID.into()),
            |window, cx| window.close_dialog(cx),
            cx,
        );
        dialog.title(title.clone()).w(px(560.)).child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .child("Shown once: copy it now. Hide or close this to discard it."),
                )
                .child(div().debug_selector(|| TOKEN_SELECTOR.into()).child(row)),
        )
    });
}
