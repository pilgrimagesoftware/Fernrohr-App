//! Large bitmap artwork drawn at exactly its device resolution: the picker's
//! logo and the About window's app icon.
//!
//! Both assets are far larger than they are drawn, and handing them to
//! `img()` leaves the GPU to shrink them several times over - linearly, with
//! no mipmaps - which comes out jagged and soft. So, as `ui::icon` does for the
//! kind icons, each is resampled once to exactly the device pixels it covers,
//! with a Lanczos3 filter, and drawn 1:1. Resampling takes tens of
//! milliseconds in a release build (about a second in debug), so it runs on
//! the background executor; until it lands, the element reserves its box and
//! draws nothing in it, so nothing shifts when it appears.

use gpui_kit::*;
use image::imageops::FilterType;
use image::{Frame, ImageBuffer, ImageFormat, Rgba, RgbaImage};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// How many device widths the cache keeps per asset: one per display scale
/// factor it is shown at, 1x and 2x in practice. Asking for another releases
/// that asset's oldest.
const WIDTHS_PER_ASSET: usize = 2;

/// One embedded bitmap, by a name the cache keys it on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Asset {
    pub name: &'static str,
    pub bytes: &'static [u8],
    pub format: ImageFormat,
}

/// `logical`'s width in device pixels at `scale_factor`.
pub fn device_width(logical: f32, scale_factor: f32) -> u32 {
    (logical * scale_factor).round().max(1.) as u32
}

/// `asset` drawn `width` x `height` logical pixels at the window's
/// resolution, or that box empty while the resolution is still being
/// resampled. `selector` names the box (and, as `{selector}-raster`, the
/// drawn image) for tests. The box never shrinks in a flex layout.
pub fn resampled(
    asset: Asset,
    width: f32,
    height: f32,
    selector: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let scale_factor = window.scale_factor();
    let device = device_width(width, scale_factor);
    let slot = div()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .debug_selector(move || selector.to_string());
    match cx
        .default_global::<RasterCache>()
        .get_or_start(asset, device)
    {
        Lookup::Ready(raster) => {
            let size = raster.size(0);
            slot.child(
                img(ImageSource::Render(raster))
                    .debug_selector(move || format!("{selector}-raster"))
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
                    .spawn(async move { rasterize(asset, device) })
                    .await;
                cx.update(|cx| {
                    let evicted = cx
                        .default_global::<RasterCache>()
                        .finish(asset, device, raster);
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

/// `asset` resampled to `width` device pixels, keeping its aspect ratio, as
/// GPUI's BGRA frame - or `None` if the embedded asset doesn't decode, which
/// only a broken build could cause.
///
/// The filter works in premultiplied alpha: on straight alpha, Lanczos pulls
/// the colour of fully transparent pixels into the artwork's edges as a dark
/// fringe.
pub fn rasterize(asset: Asset, width: u32) -> Option<Arc<RenderImage>> {
    let source = image::load_from_memory_with_format(asset.bytes, asset.format)
        .inspect_err(|error| log::error!("the embedded {} doesn't decode: {error}", asset.name))
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

/// What [`RasterCache::get_or_start`] found.
enum Lookup {
    Ready(Arc<RenderImage>),
    /// Being resampled already.
    Pending,
    /// Not cached: the caller starts resampling it.
    Start,
}

/// Every asset's rasters by device width, and which are being made. An app
/// global, so every window shares them.
#[derive(Default)]
pub(crate) struct RasterCache {
    ready: HashMap<(&'static str, u32), Arc<RenderImage>>,
    pending: Vec<(&'static str, u32)>,
    /// Each asset's finished widths, oldest first.
    order: HashMap<&'static str, VecDeque<u32>>,
}

impl Global for RasterCache {}

impl RasterCache {
    fn get_or_start(&mut self, asset: Asset, width: u32) -> Lookup {
        let key = (asset.name, width);
        if let Some(raster) = self.ready.get(&key) {
            return Lookup::Ready(raster.clone());
        }
        if self.pending.contains(&key) {
            return Lookup::Pending;
        }
        self.pending.push(key);
        Lookup::Start
    }

    /// Stores `asset`'s finished raster at `width`, returning any of its
    /// rasters evicted to make room, for the caller to release.
    fn finish(
        &mut self,
        asset: Asset,
        width: u32,
        raster: Option<Arc<RenderImage>>,
    ) -> Vec<Arc<RenderImage>> {
        let key = (asset.name, width);
        self.pending.retain(|pending| *pending != key);
        let Some(raster) = raster else {
            return Vec::new();
        };
        self.ready.insert(key, raster);
        let order = self.order.entry(asset.name).or_default();
        order.retain(|ready| *ready != width);
        order.push_back(width);
        let mut evicted = Vec::new();
        while order.len() > WIDTHS_PER_ASSET {
            if let Some(oldest) = order.pop_front()
                && let Some(image) = self.ready.remove(&(asset.name, oldest))
            {
                evicted.push(image);
            }
        }
        evicted
    }
}

#[cfg(test)]
mod tests;
