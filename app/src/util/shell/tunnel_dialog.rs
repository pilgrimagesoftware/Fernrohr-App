//! The Set Tunnel for Context dialog: its rows, and writing the chosen binding.

use super::*;
use gpui_kit::component::WindowExt as _;

/// Section 3.2: one row of `on_action_set_tunnel`'s dialog - writes the binding (or,
/// for `tunnel_id: None`, removes it) and closes the dialog. A plain `Button` rather
/// than a `PopupMenuItem`: this is a dialog's own content, not a popover menu.
pub(super) fn tunnel_dialog_option(
    label: String,
    checked: bool,
    tunnels_path: PathBuf,
    context_name: String,
    tunnel_id: Option<String>,
) -> AnyElement {
    let label = if checked {
        format!("\u{2713} {label}")
    } else {
        label
    };
    Button::new(SharedString::from(format!(
        "set-tunnel-{}",
        tunnel_id.as_deref().unwrap_or("direct")
    )))
    .label(label)
    .ghost()
    .w_full()
    .on_click(move |_event, window, cx| {
        set_context_tunnel_and_close(&tunnels_path, &context_name, tunnel_id.clone(), window, cx);
    })
    .into_any_element()
}

/// The "Set tunnel for <context>" chooser: Direct plus every tunnel, the current
/// binding checked. Shared by `context.set_tunnel` in a workspace and in the picker.
pub(super) fn open_tunnel_dialog(context_name: String, window: &mut Window, cx: &mut App) {
    let tunnels_path = paths::preference_dir().join("tunnels.toml");
    let store = TunnelStore::new(tunnels_path.clone());
    let choices = picker_tunnel::tunnel_choices(&store);
    let current = store.binding_for(&context_name);

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let mut options: Vec<AnyElement> = Vec::new();
        options.push(tunnel_dialog_option(
            "Direct".to_string(),
            current.is_none(),
            tunnels_path.clone(),
            context_name.clone(),
            None,
        ));
        for choice in &choices {
            let checked = current.as_deref() == Some(choice.id.as_str());
            options.push(tunnel_dialog_option(
                choice.name.clone(),
                checked,
                tunnels_path.clone(),
                context_name.clone(),
                Some(choice.id.clone()),
            ));
        }
        dialog
            .title(format!("Set tunnel for {context_name}"))
            .w(px(360.))
            .child(div().flex().flex_col().gap_1().children(options))
    });
}

/// Section 3.2's actual write: binds (`Some`) or unbinds (`None`) `context_name`
/// through the same `TunnelStore` `ui/picker_tunnel.rs`'s row selector uses. Free of
/// any GPUI context, so it's testable without a `Root` (which `open_dialog`/
/// `close_dialog` require) at all - this is `context.set_tunnel`'s handler binding,
/// in the sense tasks.md 3.2 asks for.
pub(super) fn write_context_tunnel(
    tunnels_path: &Path,
    context_name: &str,
    tunnel_id: Option<&str>,
) -> Result<(), crate::tunnel::store::TunnelStoreError> {
    let store = TunnelStore::new(tunnels_path.to_path_buf());
    match tunnel_id {
        Some(id) => store.bind(context_name, id),
        None => store.unbind(context_name),
    }
}

/// Writes `context_name`'s tunnel binding and closes the dialog the option came from.
pub(super) fn set_context_tunnel_and_close(
    tunnels_path: &Path,
    context_name: &str,
    tunnel_id: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    match write_context_tunnel(tunnels_path, context_name, tunnel_id.as_deref()) {
        Ok(()) => crate::ui::tunnels::notify_tunnels_changed(cx),
        Err(error) => log::warn!("failed to set {context_name}'s tunnel binding: {error:?}"),
    }
    window.close_dialog(cx);
}

#[cfg(test)]
mod tests;
