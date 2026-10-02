//! What the About window draws: the icon, the version and build a bug report
//! needs, and the credits.

use std::sync::{Arc, LazyLock};

use gpui_kit::base::StyledExt;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Context, Image, ImageFormat, InteractiveElement, IntoElement, KeyDownEvent, ParentElement,
    Render, StatefulInteractiveElement, Styled, Window, div, img, px,
};

use crate::consts::APP_NAME;

use super::build_info::{build_details, build_identifier, version};
use super::window::AboutWindow;

/// The Kubernetes community icon set's credit (`resource-kind-icons` 4.1):
/// whose the kind icons are, and the licence Fernrohr uses them under. The
/// licence text ships in the app bundle as `licenses/kubernetes-icons/LICENSE`.
pub(super) const KUBERNETES_ICONS_CREDIT: &str =
    "Kubernetes community icon set, under the Apache License 2.0";

/// The icon shown at the top of the window, at its 128px edge length - the
/// same size macOS's own About box shows the app icon at.
const ICON_SIZE: f32 = 128.;

impl AboutWindow {
    /// The heading over a group of credits.
    fn credit_heading(cx: &Context<Self>, text: &'static str) -> gpui_kit::Div {
        div()
            .mt_2()
            .text_xs()
            .text_center()
            .font_semibold()
            .text_color(cx.theme().muted_foreground)
            .child(text)
    }

    /// One credit line. Muted and small: the credits are the least of what
    /// someone opens this window to read, and must not compete with the
    /// version.
    fn credit(cx: &Context<Self>, text: &'static str) -> gpui_kit::Div {
        div()
            .text_xs()
            .text_center()
            .text_color(cx.theme().muted_foreground)
            .child(text)
    }
}

impl Render for AboutWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Focusing here rather than at open time: the focus handle only has
        // an element to attach to once this tree exists.
        if window.focused(cx).is_none() {
            window.focus(&self.focus.clone(), cx);
        }

        // Built once and reused: `Image`'s `Hash` impl hashes its own bytes,
        // which is what gpui's asset cache keys on, so only the first render
        // decodes it.
        static ICON: LazyLock<Arc<Image>> = LazyLock::new(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../../../../images/fernrohr-icon.png").to_vec(),
            ))
        });

        let details = build_details();

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, window, _| {
                // Scoped to this window's own focus handle rather than an
                // app-wide `Escape` binding, which would take the key away
                // from every other window.
                if event.keystroke.key == "escape" {
                    window.remove_window();
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(crate::ui::space::spacing(cx).control_gap)
            // Twice the panel inset: the About window is a single centred card.
            .p(crate::ui::space::spacing(cx).panel_inset * 2.)
            .bg(cx.theme().background)
            .child(img(ICON.clone()).w(px(ICON_SIZE)).h(px(ICON_SIZE)))
            .child(div().text_2xl().font_semibold().child(APP_NAME))
            // The version and build are the text a bug report needs, so the
            // text itself copies them - a copy button beside it was one more
            // thing to aim at.
            .child(
                div()
                    .id("about-build-details")
                    .flex()
                    .flex_col()
                    .items_center()
                    .px_3()
                    .py_1()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|style| style.bg(cx.theme().muted))
                    .tooltip(|window, cx| Tooltip::new("Copy build details").build(window, cx))
                    .on_click(move |_, _, app| {
                        app.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                            details.clone(),
                        ));
                    })
                    .child(
                        div()
                            .text_sm()
                            .text_center()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Version {}", version())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_center()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Build {}", build_identifier())),
                    ),
            )
            .child(
                div()
                    .mt_2()
                    .text_xs()
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child("Copyright (c) Pilgrimage Software"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .mt_4()
                    .gap_1()
                    .items_center()
                    .child(Self::credit_heading(cx, "Built with"))
                    .child(Self::credit(cx, "GPUI"))
                    .child(Self::credit(cx, "gpui-kit"))
                    .child(Self::credit_heading(cx, "Icons"))
                    .child(Self::credit(cx, KUBERNETES_ICONS_CREDIT)),
            )
            // A macOS About box has no button: it is closed from its window
            // chrome. Elsewhere that is not the expectation, so the window
            // carries an explicit Close.
            .when(!cfg!(target_os = "macos"), |column| {
                column.child(
                    div().mt_4().child(
                        Button::new("about-close")
                            .label("Close")
                            .on_click(|_, window: &mut Window, _| window.remove_window()),
                    ),
                )
            })
    }
}
