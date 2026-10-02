//! The Fernrohr logo above the picker card, drawn at its device resolution.
//!
//! The asset is far larger than it is drawn (1241px wide, shown at 192pt), and
//! handing that to `img()` leaves the GPU to shrink it 3.2x on a 2x display and
//! 6.5x on a 1x one - linearly, with no mipmaps - which comes out jagged and
//! soft. So, as `ui::icon` does for the kind icons, the logo is resampled once
//! to exactly the device pixels it covers, with a Lanczos3 filter, and drawn
//! 1:1. Resampling takes ~30ms in a release build (~1s in debug), so it runs on
//! the background executor; until it lands the picker reserves the logo's box
//! and draws nothing in it, so nothing shifts when it appears.
//!
//! **The asset.** Embedded at compile time rather than resolved from disk at
//! runtime, so the picker is unaffected by the working directory or by how the
//! app is packaged. 1.14MB is not a reason to load it at runtime: that is under
//! 1% of even a debug build, and it buys back a missing-file failure mode on a
//! decorative asset plus new packaging work for every platform's bundle layout.
//!
//! The asset is **lossless** - verified bit-identical to the master, alpha
//! included - because a lossy encode is not good enough at this size. The mark
//! is effectively photographic (259,771 distinct colours across 1.57M pixels, so
//! continuous gradient rather than flat fills), which is why lossless WebP only
//! reaches 1.14MB against the 1.6MB PNG instead of the 6x a graphic would give.
//! An earlier q90 encode was 215KB and scored PSNR 40dB, but that reads as
//! clean at 96px and shows at 192px, and PSNR is the wrong instrument for
//! judging a logo.
//!
//! Reproduce with (note the ordering - `-preset` must precede `-lossless`, or it
//! silently overwrites it and the output is lossy despite the flag):
//!
//! ```sh
//! magick images/fernrohr-logo.png -trim +repage trimmed.png
//! cwebp -preset picture -lossless -z 9 -m 6 trimmed.png -o app/assets/fernrohr-logo.webp
//! ```
//!
//! The asset is trimmed to its alpha bounding box so the rendered height tracks
//! the mark rather than the source canvas, which carried ~60px of horizontal and
//! ~90px of vertical transparent margin.

use super::layout::{LOGO_HEIGHT, LOGO_WIDTH};
use gpui_kit::*;
use image::imageops::FilterType;
use image::{Frame, ImageBuffer, Rgba, RgbaImage};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

const LOGO_WEBP: &[u8] = include_bytes!("../../../assets/fernrohr-logo.webp");

/// How many device widths the cache keeps: one per display scale factor a
/// picker is shown at, 1x and 2x in practice. Asking for another releases the
/// oldest.
const CACHED_WIDTHS: usize = 2;

/// The logo's width in device pixels at `scale_factor`.
pub(super) fn device_width(scale_factor: f32) -> u32 {
    (LOGO_WIDTH * scale_factor).round().max(1.) as u32
}

/// The logo at `scale_factor`'s resolution, or its empty box while that
/// resolution is still being resampled.
pub(super) fn logo(window: &mut Window, cx: &mut App) -> AnyElement {
    let scale_factor = window.scale_factor();
    let width = device_width(scale_factor);
    let slot = div()
        .flex_none()
        .w(px(LOGO_WIDTH))
        .h(px(LOGO_HEIGHT))
        .debug_selector(|| "picker-logo".into());
    match cx.default_global::<LogoCache>().get_or_start(width) {
        Lookup::Ready(raster) => {
            let size = raster.size(0);
            slot.child(
                img(ImageSource::Render(raster))
                    .debug_selector(|| "picker-logo-raster".into())
                    .w(px(size.width.0 as f32 / scale_factor))
                    .h(px(size.height.0 as f32 / scale_factor)),
            )
            .into_any_element()
        }
        Lookup::Pending => slot.into_any_element(),
        Lookup::Start => {
            cx.spawn(async move |cx| {
                let raster = cx
                    .background_executor()
                    .spawn(async move { rasterize(width) })
                    .await;
                cx.update(|cx| {
                    let evicted = cx.default_global::<LogoCache>().finish(width, raster);
                    for image in evicted {
                        cx.drop_image(image, None);
                    }
                    cx.refresh_windows();
                });
            })
            .detach();
            slot.into_any_element()
        }
    }
}

/// The logo resampled to `width` device pixels, keeping its aspect ratio, as
/// GPUI's BGRA frame - or `None` if the embedded asset doesn't decode, which
/// only a broken build could cause.
///
/// The filter works in premultiplied alpha: on straight alpha, Lanczos pulls
/// the colour of the fully transparent margin into the mark's edges as a dark
/// fringe.
pub(super) fn rasterize(width: u32) -> Option<Arc<RenderImage>> {
    let source = image::load_from_memory_with_format(LOGO_WEBP, image::ImageFormat::WebP)
        .inspect_err(|error| log::error!("the embedded logo doesn't decode: {error}"))
        .ok()?
        .into_rgba8();
    let height = ((width as f32) * source.height() as f32 / source.width() as f32)
        .round()
        .max(1.) as u32;
    let premultiplied: ImageBuffer<Rgba<f32>, Vec<f32>> =
        ImageBuffer::from_fn(source.width(), source.height(), |x, y| {
            let [r, g, b, a] = source.get_pixel(x, y).0.map(|c| c as f32 / 255.);
            Rgba([r * a, g * a, b * a, a])
        });
    let resized = image::imageops::resize(&premultiplied, width, height, FilterType::Lanczos3);
    let bgra = RgbaImage::from_fn(width, height, |x, y| {
        let [r, g, b, a] = resized.get_pixel(x, y).0;
        let a = a.clamp(0., 1.);
        let straight = |c: f32| {
            if a > 0. {
                ((c / a).clamp(0., 1.) * 255.).round() as u8
            } else {
                0
            }
        };
        // GPUI's frames are BGRA.
        Rgba([
            straight(b),
            straight(g),
            straight(r),
            (a * 255.).round() as u8,
        ])
    });
    Some(Arc::new(RenderImage::new(vec![Frame::new(bgra)])))
}

/// What [`LogoCache::get_or_start`] found for a width.
enum Lookup {
    Ready(Arc<RenderImage>),
    /// Being resampled already.
    Pending,
    /// Not cached: the caller starts resampling it.
    Start,
}

/// The logo's rasters by device width, and which widths are being made. An
/// app global, so every picker window shares them.
#[derive(Default)]
pub(super) struct LogoCache {
    ready: HashMap<u32, Arc<RenderImage>>,
    pending: Vec<u32>,
    /// Finished widths, oldest first.
    order: VecDeque<u32>,
}

impl Global for LogoCache {}

impl LogoCache {
    fn get_or_start(&mut self, width: u32) -> Lookup {
        if let Some(raster) = self.ready.get(&width) {
            return Lookup::Ready(raster.clone());
        }
        if self.pending.contains(&width) {
            return Lookup::Pending;
        }
        self.pending.push(width);
        Lookup::Start
    }

    /// Stores `width`'s finished raster, returning any evicted to make room,
    /// for the caller to release.
    fn finish(&mut self, width: u32, raster: Option<Arc<RenderImage>>) -> Vec<Arc<RenderImage>> {
        self.pending.retain(|pending| *pending != width);
        let Some(raster) = raster else {
            return Vec::new();
        };
        self.ready.insert(width, raster);
        self.order.retain(|ready| *ready != width);
        self.order.push_back(width);
        let mut evicted = Vec::new();
        while self.order.len() > CACHED_WIDTHS {
            if let Some(oldest) = self.order.pop_front()
                && let Some(image) = self.ready.remove(&oldest)
            {
                evicted.push(image);
            }
        }
        evicted
    }
}

#[cfg(test)]
mod tests;
