//! What the About window draws: the icon, the version and build a bug report
//! needs, and the credits.

use gpui_kit::base::StyledExt;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Bounds, Context, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Pixels,
    Render, StatefulInteractiveElement, Styled, Window, div, px, size,
};

use crate::consts::APP_NAME;
use crate::ui::raster::{self, Asset};
use std::cell::Cell;
use std::rc::Rc;

use super::build_info::{build_details, build_identifier, version};
use super::window::AboutWindow;

/// The Kubernetes community icon set's credit (`resource-kind-icons` 4.1):
/// whose the kind icons are, and the licence Fernrohr uses them under. The
/// licence text ships in the app bundle as `licenses/kubernetes-icons/LICENSE`.
pub(super) const KUBERNETES_ICONS_CREDIT: &str =
    "Kubernetes community icon set, under the Apache License 2.0";

/// The icon shown at the top of the window, at its 128px edge length - the
/// same size macOS's own About box shows the app icon at.
pub(super) const ICON_SIZE: f32 = 128.;

/// The app icon, 1254px square, resampled to its device resolution
/// ([`raster`]) rather than shrunk ~10x by the GPU.
pub(super) const ICON: Asset = Asset {
    name: "about-icon",
    bytes: include_bytes!("../../../../images/fernrohr-icon.png"),
    format: image::ImageFormat::Png,
};

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

        let details = build_details();
        let icon = raster::resampled(ICON, ICON_SIZE, ICON_SIZE, "about-icon", window, cx);
        let space = crate::ui::space::spacing(cx);

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
            .bg(cx.theme().background)
            .on_children_prepainted({
                let fitted = self.fitted_height.clone();
                move |children, window, cx| fit_window_to_content(&fitted, children, window, cx)
            })
            .child(
                // The content at its natural height - `flex_none`, in a
                // column, so it is neither stretched nor shrunk - so the
                // window can be fitted to it. With the window's height fixed
                // instead, this column overflowed, and flexbox shrank the icon
                // to fit: 128 wide but 9 tall, drawn as a tiny square.
                div()
                    .flex_none()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(space.control_gap)
                    // Twice the panel inset: the About window is a single
                    // centred card.
                    .p(space.panel_inset * 2.)
                    .child(icon)
                    .child(
                        div()
                            .text_2xl()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(APP_NAME),
                    )
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
                            .tooltip(|window, cx| {
                                Tooltip::new("Copy build details").build(window, cx)
                            })
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
                            .debug_selector(|| "about-credits".into())
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
                    }),
            )
    }
}

/// Resizes the window to its content's height whenever the content needs a
/// height it hasn't been given yet - the first frame, a text-size change, a
/// credit added later - so the content never overflows and nothing in it is
/// squeezed. Deferred until this paint finishes rather than resizing
/// mid-paint. `fitted` remembers the last height asked for, so a window the
/// platform can't or won't resize (a short screen) asks once rather than on
/// every frame.
fn fit_window_to_content(
    fitted: &Rc<Cell<Option<Pixels>>>,
    children: Vec<Bounds<Pixels>>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(content) = children.first() else {
        return;
    };
    let height = content.size.height.ceil();
    let viewport = window.viewport_size();
    let settled = (height - viewport.height).abs() <= px(1.);
    if settled || fitted.get() == Some(height) {
        return;
    }
    fitted.set(Some(height));
    window.defer(cx, move |window, _| {
        window.resize(size(viewport.width, height));
        window.refresh();
    });
}
