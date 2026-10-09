//! One status bar capsule per context, and the add control after them
//! (`toolbar-layout-with-gpui-kit` 1.1-1.3, formerly the context bar's chips): the
//! capsule draws a [`StatusItem`] as `<context> [<tunnel>] <state icon>`, then the
//! elapsed time unless connected (`status-capsule-icons`), and clicking it makes
//! its context the window's active one; its menu offers Disconnect. Every action
//! asks `MainWindow`, which owns the window's contexts.
//!
//! The context name is what a row of capsules is scanned for, so it leads, in the
//! frame font and the severity tint. The tunnel and elapsed time are secondary:
//! data font, a size down, muted - brackets included, so they never take the
//! tint. The state's name is the icon's tooltip.
//!
//! A context awaiting a manual tunnel's confirmation (`manual-confirmation-tunnels`
//! D8) keeps that layout, filled with the theme's warning color, and its menu
//! offers Proceed and Cancel above Disconnect.
//!
//! Every capsule's colors come from [`capsule_palette`], chosen by contrast rather
//! than taken from a theme field: the attention fill's text is black or white,
//! whichever reads better on it (a theme's `warning_foreground` can be as yellow as
//! the fill), and secondary text is lifted until it reads on the status bar. Every
//! piece of capsule text reaches the WCAG [`TEXT_CONTRAST`] of 4.5:1, which the
//! palette's tests hold every bundled theme to.

use super::{StatusBarView, StatusItem};
use crate::k8s::cluster::context_health::{ContextHealth, Severity};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::format_age;
use crate::tunnel::manual::{Decision, ManualConfirmations};
use crate::ui::icon_tooltip;
use crate::ui::manual_tunnel::{CancelManualTunnel, ProceedManualTunnel};
use crate::ui::picker::{ClusterPicker, PickerEvent};
use crate::ui::style::{TEXT_CONTRAST, contrast, meet, over, readable_on};
use crate::ui::typography::TypeRole as _;
use crate::util::context_lifecycle;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The tunnel's and elapsed time's size, in rems: one step below the capsule's
/// own `text_xs` (0.75rem). In rems so it follows the text-size preference.
const SECONDARY_TEXT_REMS: f32 = 0.6875;

/// How much the attention fill's secondary text is faded, to set it back from
/// the name - kept only where the faded text still reaches [`TEXT_CONTRAST`].
const ATTENTION_SECONDARY_ALPHA: f32 = 0.8;

/// One capsule's colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CapsulePalette {
    /// The capsule's own fill, for an attention capsule; `None` draws it on the
    /// status bar (or, when active, the selection fill).
    pub(crate) fill: Option<Hsla>,
    /// The name's and state icon's color.
    pub(crate) primary: Hsla,
    /// The tunnel's and elapsed time's color.
    pub(crate) secondary: Hsla,
}

/// The colors a capsule of `severity` is drawn in under `theme`.
pub(crate) fn capsule_palette(
    theme: &gpui_kit::component::Theme,
    severity: Severity,
) -> CapsulePalette {
    if severity == Severity::Attention {
        let fill = theme.warning;
        let primary = readable_on(fill);
        let faded = primary.opacity(ATTENTION_SECONDARY_ALPHA);
        let secondary = if contrast(faded, fill) >= TEXT_CONTRAST {
            faded
        } else {
            primary
        };
        return CapsulePalette {
            fill: Some(fill),
            primary,
            secondary,
        };
    }
    // Secondary text must read on the bar itself and on an active capsule's
    // selection fill; `meet` only moves it further from the background, so
    // meeting both keeps the first met.
    let surfaces = [theme.background, over(theme.selection, theme.background)];
    let secondary = surfaces
        .into_iter()
        .fold(theme.muted_foreground, |colour, surface| {
            meet(colour, surface, TEXT_CONTRAST)
        });
    CapsulePalette {
        fill: None,
        primary: color(theme, severity),
        secondary,
    }
}

/// The debug selectors of a capsule's parts, for `context_name`'s capsule.
fn name_selector(context_name: &str) -> String {
    format!("status-item-name-{context_name}")
}
fn tunnel_selector(context_name: &str) -> String {
    format!("status-item-tunnel-{context_name}")
}
pub(crate) fn icon_selector(context_name: &str) -> String {
    format!("status-item-icon-{context_name}")
}
fn elapsed_selector(context_name: &str) -> String {
    format!("status-item-elapsed-{context_name}")
}

fn color(theme: &gpui_kit::component::Theme, severity: Severity) -> Hsla {
    match severity {
        Severity::Muted => theme.muted_foreground,
        Severity::Info => theme.info,
        Severity::Warning => theme.warning,
        Severity::Danger => theme.danger,
        // Drawn on the capsule's own fill: see `capsule_palette`.
        Severity::Attention => readable_on(theme.warning),
    }
}

/// The debug selector a capsule carries while it is filled for attention.
pub(crate) fn attention_selector(context_name: &str) -> String {
    format!("status-attention-{context_name}")
}

/// `item`'s capsule: its health and identity as one clickable body, then a menu
/// button offering Disconnect - and, while the context awaits a manual tunnel's
/// confirmation, Proceed and Cancel above it. Fully rounded and bordered, so the
/// row reads as a set of contexts; the active one is filled, and an awaiting one
/// is filled for attention.
pub(super) fn render_capsule(
    item: StatusItem,
    this: WeakEntity<StatusBarView>,
    theme: &gpui_kit::component::Theme,
) -> AnyElement {
    let palette = capsule_palette(theme, item.severity);
    let tint = palette.primary;
    let attention = palette.fill.is_some();
    let secondary_tint = palette.secondary;
    let selector = format!("status-item-{}", item.context_name);
    let tooltip = item.tooltip();
    let context = item.context_name.clone();
    let secondary = |text: String, selector: String| {
        div()
            .debug_selector(move || selector)
            .data_font()
            .text_size(rems(SECONDARY_TEXT_REMS))
            .text_color(secondary_tint)
            .child(text)
    };
    let icon_selector = icon_selector(&context);
    let icon = icon_tooltip::with_tooltip_text(
        SharedString::from(icon_selector.clone()),
        tooltip,
        Icon::new(item.icon).size(px(12.)).text_color(tint),
    )
    .debug_selector(move || icon_selector);
    let name = item.context_name.clone();
    let name_for_fill = item.context_name.clone();
    let click_target = this.clone();
    let body = Button::new(SharedString::from(format!(
        "context-chip-{}",
        item.context_name
    )))
    .ghost()
    .xsmall()
    .child(
        div()
            .debug_selector(move || selector)
            .flex()
            .items_center()
            .gap_1p5()
            .text_color(tint)
            .child({
                let selector = name_selector(&context);
                div()
                    .debug_selector(move || selector)
                    .child(item.context_name.clone())
            })
            .children(
                item.tunnel
                    .map(|tunnel| secondary(format!("[{tunnel}]"), tunnel_selector(&context))),
            )
            .child(icon)
            .children(item.elapsed.map(|elapsed| {
                secondary(
                    format_age(elapsed.as_secs() as i64),
                    elapsed_selector(&context),
                )
            })),
    )
    .on_click({
        let name = name.clone();
        move |_event, window, cx| {
            let name = name.clone();
            let _ = click_target.update(cx, |bar, cx| bar.activate(&name, window, cx));
        }
    });

    // While the context waits on a manual tunnel, Proceed and Cancel lead its menu.
    let awaiting_tunnel = match &item.health {
        ContextHealth::AwaitingConfirmation { tunnel_id, .. } => Some(tunnel_id.clone()),
        _ => None,
    };
    let menu_button = Button::new(SharedString::from(format!(
        "context-chip-menu-{}",
        item.context_name
    )))
    .icon(IconName::ChevronDown)
    .xsmall()
    .ghost()
    .dropdown_menu(move |menu, _window, _cx| {
        let this = this.clone();
        let name = name.clone();
        let menu = match awaiting_tunnel.clone() {
            Some(tunnel_id) => menu
                .item(answer_item(&tunnel_id, Decision::Proceed))
                .item(answer_item(&tunnel_id, Decision::Cancel))
                .separator(),
            None => menu,
        };
        menu.item(
            PopupMenuItem::new("Disconnect").on_click(move |_event, window, cx| {
                let name = name.clone();
                let _ = this.update(cx, |bar, cx| bar.open_disconnect_dialog(name, window, cx));
            }),
        )
    });

    div()
        .flex()
        .items_center()
        .gap_0p5()
        .pl_1()
        .pr_0p5()
        .rounded_full()
        .border_1()
        .border_color(if attention {
            theme.warning
        } else {
            theme.border
        })
        .when(item.active && !attention, |el| el.bg(theme.selection))
        .when_some(palette.fill, |el, fill| {
            let selector = attention_selector(&name_for_fill);
            el.bg(fill).debug_selector(move || selector)
        })
        .child(body)
        .child(menu_button)
        .into_any_element()
}

/// A waiting capsule's Proceed or Cancel row: it answers this capsule's own tunnel -
/// every context sharing it - and shows the matching command's key, which the menu
/// reads off the row's action. The click handler runs instead of the action, so the
/// row never asks which tunnel when several are waiting.
fn answer_item(tunnel_id: &str, decision: Decision) -> PopupMenuItem {
    let (label, action): (&str, Box<dyn Action>) = match decision {
        Decision::Proceed => ("Proceed", Box::new(ProceedManualTunnel)),
        Decision::Cancel => ("Cancel", Box::new(CancelManualTunnel)),
    };
    let tunnel_id = tunnel_id.to_string();
    PopupMenuItem::new(label)
        .action(action)
        .on_click(move |_event, _window, cx| {
            ManualConfirmations::resolve(cx, &tunnel_id, decision);
        })
}

/// The add control after the capsules.
pub(super) fn render_add_button(this: WeakEntity<StatusBarView>) -> impl IntoElement {
    Button::new("context-bar-add")
        .icon(IconName::Plus)
        .xsmall()
        .ghost()
        .tooltip("Add a context")
        .on_click(move |_event, window, cx| {
            let _ = this.update(cx, |bar, cx| bar.open_add_dialog(window, cx));
        })
}

impl StatusBarView {
    /// A capsule click: makes `context_name` the window's active context.
    pub(super) fn activate(
        &mut self,
        context_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(main_window) = self.main_window.upgrade() else {
            return;
        };
        main_window.update(cx, |main_window, cx| {
            main_window.set_active_context(context_name, window, cx);
        });
    }

    /// The window's active context, for the palette's Disconnect.
    pub(crate) fn active_context(&self) -> Option<String> {
        self.context_names.get(self.active).cloned()
    }

    /// The Disconnect confirmation, naming how many panels will close and, if
    /// another window still uses the context, that it stays connected there.
    /// Confirming calls back into `MainWindow::disconnect_context`.
    pub(crate) fn open_disconnect_dialog(
        &mut self,
        context_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(main_window) = self.main_window.upgrade() else {
            return;
        };
        let panel_count = main_window.read(cx).context_panel_count(&context_name);
        let other_windows = ClusterRegistry::holder_count(cx, &context_name).saturating_sub(1);
        let body = context_lifecycle::disconnect_confirmation_body(
            &context_name,
            panel_count,
            other_windows,
        );
        let title = format!("Disconnect {context_name}?");
        let target = self.main_window.clone();
        let confirmation = crate::ui::confirm_dialog::Confirmation {
            title: title.into(),
            body,
            confirm: "Disconnect".into(),
            id_prefix: "context-disconnect",
            severity: crate::ui::confirm_dialog::Severity::Recoverable,
        };
        crate::ui::confirm_dialog::open(
            confirmation,
            move |window, cx| {
                let context_name = context_name.clone();
                let _ = target.update(cx, |main_window, cx| {
                    main_window.disconnect_context(context_name, window, cx);
                });
            },
            window,
            cx,
        );
    }

    /// The add popover: the cluster picker filtered to contexts this window doesn't
    /// already use, so tunnel labels and connect progress come for free.
    /// `PickerEvent::Connected` only fires once the chosen context connects, which
    /// is what makes a failed add open no panel.
    pub(crate) fn open_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let used = self.context_names.clone();
        let picker = cx.new(|cx| {
            let mut picker = ClusterPicker::new(window, cx);
            picker.exclude(&used);
            picker
        });
        let target = self.main_window.clone();
        cx.subscribe_in(&picker, window, {
            move |_this: &mut Self, _picker, event, window, cx| {
                let PickerEvent::Connected { context_name } = event;
                let context_name = context_name.clone();
                let _ = target.update(cx, |main_window, cx| {
                    main_window.add_context(context_name, window, cx);
                });
                window.close_dialog(cx);
            }
        })
        .detach();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .title("Add a context")
                .w(px(480.))
                .child(picker.clone())
        });
    }
}

#[cfg(test)]
mod contrast_tests;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod tests;
