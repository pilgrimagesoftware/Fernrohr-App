//! The interactive elements `render` (in [`super::render`]) assembles into the picker:
//! one context row per kubeconfig context ([`context_row`]), the Connect button
//! ([`connect_button`]), and the Tunnels-window shortcut ([`manage_tunnels_control`]).

use super::*;

/// Section 3.1: one context row's content - icon, name, and its tunnel selector at
/// the trailing end. Built fresh on every `Command` render (a `Fn`, not `FnMut`, per
/// `CommandItem::child`'s contract), so everything it needs is captured by value here
/// rather than borrowed from `ClusterPicker`.
///
/// The row wraps its own click handler rather than leaving clicks to `Command`'s
/// built-in one, which confirms - starting a connection - on *any* click
/// (`command/state.rs::render_item`). Our own `on_click`, which always calls
/// `cx.stop_propagation()`, intercepts first (mouse events dispatch
/// innermost-first), and [`ClusterPicker::handle_row_click`] decides select-only
/// versus connect from the click count. The tunnel selector is unaffected:
/// `Popover`'s trigger already stops a click at mouse-down, before it reaches even
/// this row.
pub(super) fn context_row(
    context_name: String,
    row_index: usize,
    selected: bool,
    bound_id: Option<String>,
    choices: Vec<TunnelChoice>,
    picker: WeakEntity<ClusterPicker>,
) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_window, cx| {
        let theme = cx.theme();
        let row_id = format!("picker-tunnel-{context_name}");
        let picker_for_pick = picker.clone();
        let context_for_pick = context_name.clone();
        let selector = picker_tunnel::selector(
            row_id,
            &choices,
            bound_id.as_deref(),
            move |tunnel_id, cx| {
                let _ = picker_for_pick.update(cx, |this, cx| {
                    this.set_tunnel(&context_for_pick, tunnel_id, cx)
                });
            },
        );

        let picker_for_click = picker.clone();
        let context_for_click = context_name.clone();

        div()
            .id(format!("picker-row-{context_name}"))
            .flex()
            .flex_1()
            .items_center()
            .justify_between()
            .gap_2()
            .px_1()
            .rounded(theme.radius)
            // The clicked selection, drawn by the row itself: `Command`'s own
            // highlight follows the mouse, which must not move what Connect targets.
            .when(selected, |row| row.bg(theme.selection))
            .on_click(move |event, window, cx| {
                cx.stop_propagation();
                let _ = picker_for_click.update(cx, |this, cx| {
                    this.handle_row_click(
                        context_for_click.clone(),
                        row_index,
                        event.click_count(),
                        window,
                        cx,
                    )
                });
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::Server)
                            .size(px(16.))
                            .text_color(theme.muted_foreground),
                    )
                    // Long names (`gke_<project>_<region>_<cluster>`) are cut with an
                    // ellipsis, full name on hover, so the tunnel selector stays visible.
                    .child({
                        let full = context_name.clone();
                        div()
                            .id(SharedString::from(format!("picker-name-{context_name}")))
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(context_name.clone())
                            .tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(full.clone())
                                    .build(window, cx)
                            })
                    }),
            )
            .child(div().flex_shrink_0().child(selector))
            .into_any_element()
    }
}

/// Section 4.1/design.md decision 5: the picker's own way to reach the Tunnels
/// window, alongside the app menu and the `tunnels.manage` command.
pub(super) fn manage_tunnels_control() -> impl IntoElement {
    Button::new("picker-manage-tunnels")
        .label("Manage tunnels…")
        .icon(IconName::Settings)
        .xsmall()
        .ghost()
        .on_click(|_event, _window, cx| crate::ui::tunnels::open_or_focus(cx))
}

/// The deliberate, visible way to connect the highlighted context - a single click
/// on a row no longer does this itself (see [`context_row`]'s doc comment).
/// Disabled with nothing highlighted or a connect already under way
/// ([`ClusterPicker::is_connect_in_flight`]), so this can never start a redundant
/// or targetless connect.
pub(super) fn connect_button(
    disabled: bool,
    picker: WeakEntity<ClusterPicker>,
) -> impl IntoElement {
    Button::new("picker-connect")
        .label("Connect")
        .primary()
        .disabled(disabled)
        .on_click(move |_event, _window, cx| {
            let _ = picker.update(cx, |this, cx| this.connect_selected(cx));
        })
}
