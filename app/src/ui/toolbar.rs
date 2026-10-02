//! The window's top bar (`toolbar-layout-with-gpui-kit` 3.1): a gpui-kit
//! `TitleBar` - which drags the window, zooms it on double-click and leaves room
//! for the macOS traffic lights - holding a `Toolbar` with the app icon and name.
//! Cluster contexts aren't here: they're the status bar's capsules. The window
//! opens with [`titlebar_options`] so the title bar draws itself.

use crate::consts::APP_NAME;
use crate::ui::raster::Asset;
use gpui_kit::component::toolbar::Toolbar;
use gpui_kit::component::{ActiveTheme as _, TitleBar};
use gpui_kit::*;

/// The app icon: the picker's logo pre-shrunk to 64x62 (lossless), since the
/// 1241px original costs a second to decode in a debug build for a 16px draw.
/// Reproduce with `dwebp app/assets/fernrohr-logo.webp -o logo.png && cwebp
/// -lossless -exact -z 9 -resize 64 0 logo.png -o app/assets/fernrohr-icon-64.webp`.
const ICON: Asset = Asset {
    name: "toolbar-icon",
    bytes: include_bytes!("../../assets/fernrohr-icon-64.webp"),
    format: image::ImageFormat::WebP,
};
const ICON_ASPECT: f32 = 64. / 62.;

/// The icon's height in the bar, at the default text size.
const ICON_HEIGHT: f32 = 16.;

/// Titlebar options for a window that draws [`window_toolbar`]: transparent, so
/// the toolbar is the bar, with the traffic lights where `TitleBar` expects them.
/// `title` still names the window in the Window menu and the accessibility tree.
pub fn titlebar_options(title: SharedString) -> TitlebarOptions {
    TitlebarOptions {
        title: Some(title),
        ..TitleBar::title_bar_options()
    }
}

/// The top bar: the app icon and name.
pub fn window_toolbar(window: &mut Window, cx: &mut App) -> impl IntoElement {
    let icon = crate::ui::raster::resampled(
        ICON,
        ICON_HEIGHT * ICON_ASPECT,
        ICON_HEIGHT,
        "window-toolbar-icon",
        window,
        cx,
    );
    let name = div()
        .debug_selector(|| "window-toolbar-name".into())
        .text_sm()
        .text_color(cx.theme().foreground)
        .child(APP_NAME);
    TitleBar::new().child(
        Toolbar::new("window-toolbar")
            .items_center()
            .gap_2()
            .content(icon)
            .content(name),
    )
}
