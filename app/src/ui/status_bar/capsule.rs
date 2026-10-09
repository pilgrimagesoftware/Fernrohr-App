//! One status bar capsule per context, and the add control after them
//! (`toolbar-layout-with-gpui-kit` 1.1-1.3, formerly the context bar's chips): the
//! capsule draws a [`StatusItem`] - name, tunnel, state icon and text, elapsed time -
//! and clicking it makes its context the window's active one; its menu offers
//! Disconnect. Every action asks `MainWindow`, which owns the window's contexts.

use super::{StatusBarView, StatusItem};
use crate::k8s::cluster::context_health::{ContextHealth, Severity};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::format_age;
use crate::ui::picker::{ClusterPicker, PickerEvent};
use crate::util::context_lifecycle;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

fn color(theme: &gpui_kit::component::Theme, severity: Severity) -> Hsla {
    match severity {
        Severity::Muted => theme.muted_foreground,
        Severity::Info => theme.info,
        Severity::Warning => theme.warning,
        Severity::Danger => theme.danger,
        // On the capsule's warning fill.
        Severity::Attention => theme.warning_foreground,
    }
}

/// The debug selector of a capsule's state icon.
pub(crate) fn state_icon_selector(context_name: &str) -> String {
    format!("status-state-icon-{context_name}")
}

/// The debug selector a capsule carries while it is filled for attention.
pub(crate) fn attention_selector(context_name: &str) -> String {
    format!("status-attention-{context_name}")
}

/// The state icon's tooltip, for a state that has one: the awaiting state's text,
/// how long it has waited, and the tunnel's instruction.
pub(crate) fn state_tooltip_text(item: &StatusItem) -> Option<String> {
    let ContextHealth::AwaitingConfirmation { message, .. } = &item.health else {
        return None;
    };
    let waited = item
        .elapsed
        .map(|elapsed| format!(", waiting {}", format_age(elapsed.as_secs() as i64)))
        .unwrap_or_default();
    Some(match message {
        Some(message) => format!("{}{waited}. {message}", item.text),
        None => format!("{}{waited}", item.text),
    })
}

/// `item`'s capsule: its health and identity as one clickable body, then a menu
/// button whose one item is Disconnect. Fully rounded and bordered, so the row
/// reads as a set of contexts; the active one is filled.
pub(super) fn render_capsule(
    item: StatusItem,
    this: WeakEntity<StatusBarView>,
    theme: &gpui_kit::component::Theme,
) -> AnyElement {
    let tint = color(theme, item.severity);
    let attention = item.severity == Severity::Attention;
    let tunnel_tint = if attention {
        tint
    } else {
        theme.muted_foreground
    };
    // The state icon, with its tooltip while it has one to say - the awaiting state's
    // elapsed wait and instruction, so a long message never widens the bar.
    let icon_selector = state_icon_selector(&item.context_name);
    let icon = div()
        .debug_selector(move || icon_selector)
        .child(Icon::new(item.icon).size(px(12.)).text_color(tint));
    let icon = match state_tooltip_text(&item) {
        Some(text) => crate::ui::icon_tooltip::with_text_tooltip(
            format!("status-state-{}", item.context_name),
            text,
            icon,
        )
        .into_any_element(),
        None => icon.into_any_element(),
    };
    let selector = format!("status-item-{}", item.context_name);
    let name = item.context_name.clone();
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
            .child(item.context_name.clone())
            .children(
                item.tunnel
                    .map(|tunnel| div().text_xs().text_color(tunnel_tint).child(tunnel)),
            )
            .child(item.text)
            .children(
                item.elapsed
                    .map(|elapsed| format!("({} ago)", format_age(elapsed.as_secs() as i64))),
            ),
    )
    .on_click({
        let name = name.clone();
        move |_event, window, cx| {
            let name = name.clone();
            let _ = click_target.update(cx, |bar, cx| bar.activate(&name, window, cx));
        }
    });

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
        // The state icon sits before the button rather than in it, so its tooltip
        // has a hover target of its own.
        .child(icon)
        .when(attention, |el| {
            let selector = attention_selector(&item.context_name);
            el.bg(theme.warning).debug_selector(move || selector)
        })
        .child(body)
        .child(menu_button)
        .into_any_element()
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
mod tests;
