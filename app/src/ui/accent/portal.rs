//! The accent on Linux (Fernrohr#100): the XDG desktop portal's
//! `org.freedesktop.appearance` `accent-color`, which GNOME and KDE both
//! publish. A D-Bus call - one that can stall when no portal answers - so it
//! is read off the main thread, one read at a time, and the windows redraw
//! when a new colour lands.

use gpui_kit::*;

/// The portal's accent as last read, and whether a read is under way.
#[derive(Default)]
struct PortalAccent {
    last: Option<Hsla>,
    reading: bool,
}

impl Global for PortalAccent {}

/// The accent as last read; `None` before the first read lands, or where the
/// user has set none or no portal answers.
pub(super) fn cached(cx: &App) -> Option<Hsla> {
    cx.try_global::<PortalAccent>()
        .and_then(|accent| accent.last)
}

/// Starts a read unless one is under way. When it lands it becomes the
/// system accent, and the windows redraw if that changed it.
pub(super) fn request(cx: &mut App) {
    let state = cx.default_global::<PortalAccent>();
    if state.reading {
        return;
    }
    state.reading = true;
    let read = cx.background_spawn(read());
    cx.spawn(async move |cx| {
        let accent = read.await;
        cx.update(|cx| {
            let state = cx.default_global::<PortalAccent>();
            state.reading = false;
            state.last = accent;
            if super::apply(accent, cx) {
                cx.refresh_windows();
            }
        });
    })
    .detach();
}

async fn read() -> Option<Hsla> {
    let settings = ashpd::desktop::settings::Settings::new().await.ok()?;
    let color = settings.accent_color().await.ok()?;
    super::from_unit_rgb(color.red(), color.green(), color.blue())
}
