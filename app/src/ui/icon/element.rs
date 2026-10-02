//! Drawing a [`KindIcon`] beside text (`resource-kind-icons` 3.4 and
//! design.md decision 1).
//!
//! Neither of GPUI's ready-made routes draws the set faithfully at text size:
//! `svg()` is a single-colour mask, and `img()` given the SVG rasterizes it
//! once at its intrinsic size and lets the GPU shrink it, which smears edges at
//! 16-24px (the 1.1 spike). So each icon is parsed once, rasterized by GPUI's
//! own SVG renderer at exactly the device pixels it will occupy, and drawn
//! 1:1. Rasters are cached per (icon, device size), made on first use and
//! never per frame, and released once the cache no longer holds their size.

use super::KindIcon;
use crate::consts::ICON_RASTER_SIZES;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

/// Which text an icon sits beside, so it is that text's height and scales
/// with it: the theme's font size (the text-size preference has already
/// scaled it) times the role's rem factor, as `text_sm()` and friends size
/// their text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconSize {
    /// Body text (`1rem`): panel titles and tabs.
    Text,
    /// `text_sm()` text (`0.875rem`): cards, links, field values.
    Small,
    /// `text_xs()` text (`0.75rem`): the Resource panel's section headers.
    XSmall,
}

impl IconSize {
    fn rems(self) -> f32 {
        match self {
            Self::Text => 1.,
            Self::Small => 0.875,
            Self::XSmall => 0.75,
        }
    }

    /// The icon's logical size beside its text, at the current text size.
    pub fn logical(self, cx: &App) -> Pixels {
        cx.theme().font_size * self.rems()
    }
}

/// The device pixels an icon `logical` points tall covers at `scale_factor`:
/// what it is rasterized at, and so its cache key. A window moving to a
/// display with another scale factor asks for a new size.
pub fn device_px(logical: Pixels, scale_factor: f32) -> u32 {
    (f32::from(logical) * scale_factor).round().max(1.) as u32
}

/// `icon` drawn beside `size`'s text in `window`. Decorative: not focusable
/// and with no accessible name of its own, since the text beside it already
/// names the kind (`resource-icons` spec).
pub fn kind_icon(icon: KindIcon, size: IconSize, window: &mut Window, cx: &mut App) -> AnyElement {
    let scale_factor = window.scale_factor();
    let device = device_px(size.logical(cx), scale_factor);
    let renderer = cx.svg_renderer();
    let (raster, evicted) = cx
        .default_global::<IconCache>()
        .raster(icon, device, renderer);
    for image in evicted {
        cx.drop_image(image, Some(window));
    }
    let Some(raster) = raster else {
        // Bundled and covered by tests, so only a broken build gets here: keep
        // the text's alignment rather than draw nothing in its place.
        let logical = px(device as f32 / scale_factor);
        return div().size(logical).flex_none().into_any_element();
    };
    let raster_size = raster.size(0);
    img(ImageSource::Render(raster))
        .w(px(raster_size.width.0 as f32 / scale_factor))
        .h(px(raster_size.height.0 as f32 / scale_factor))
        .flex_none()
        .into_any_element()
}

/// Every icon parsed so far, and its rasters at the device sizes still in use.
/// An app global, so every window shares one set of rasters per size.
#[derive(Default)]
pub(super) struct IconCache {
    parsed: HashMap<KindIcon, Option<Rc<ParsedSvg>>>,
    rasters: HashMap<(KindIcon, u32), Arc<RenderImage>>,
    /// The device sizes the cache holds, oldest first.
    sizes: VecDeque<u32>,
}

impl Global for IconCache {}

impl IconCache {
    /// `icon`'s raster at `device` pixels - from the cache, or rasterized now
    /// and kept - and any rasters evicted to make room, for the caller to
    /// release. `None` only if the icon's SVG doesn't parse or render.
    ///
    /// The cache holds up to [`ICON_RASTER_SIZES`] device sizes. Asking for a
    /// new one past that drops every raster at the oldest size: a text-size
    /// change or a move to a display with another scale factor leaves its old
    /// sizes unused.
    pub(super) fn raster(
        &mut self,
        icon: KindIcon,
        device: u32,
        renderer: SvgRenderer,
    ) -> (Option<Arc<RenderImage>>, Vec<Arc<RenderImage>>) {
        if let Some(raster) = self.rasters.get(&(icon, device)) {
            return (Some(raster.clone()), Vec::new());
        }
        let evicted = self.make_room_for(device);
        let parsed = self
            .parsed
            .entry(icon)
            .or_insert_with(|| match renderer.parse_svg(icon.svg()) {
                Ok(parsed) => Some(Rc::new(parsed)),
                Err(error) => {
                    log::error!("the bundled {icon:?} icon doesn't parse: {error}");
                    None
                }
            })
            .clone();
        let size = gpui_kit::size(DevicePixels(device as i32), DevicePixels(device as i32));
        let raster = parsed.and_then(|parsed| {
            renderer
                .render_parsed(&parsed, SvgSize::Size(size))
                .inspect_err(|error| log::error!("the {icon:?} icon doesn't render: {error}"))
                .ok()
        });
        if let Some(raster) = &raster {
            self.rasters.insert((icon, device), raster.clone());
        }
        (raster, evicted)
    }

    /// Records `device` as in use, evicting the oldest size's rasters if that
    /// takes the cache past its size budget.
    fn make_room_for(&mut self, device: u32) -> Vec<Arc<RenderImage>> {
        if self.sizes.contains(&device) {
            return Vec::new();
        }
        self.sizes.push_back(device);
        if self.sizes.len() <= ICON_RASTER_SIZES {
            return Vec::new();
        }
        let Some(oldest) = self.sizes.pop_front() else {
            return Vec::new();
        };
        let stale: Vec<(KindIcon, u32)> = self
            .rasters
            .keys()
            .filter(|(_, size)| *size == oldest)
            .copied()
            .collect();
        stale
            .into_iter()
            .filter_map(|key| self.rasters.remove(&key))
            .collect()
    }

    #[cfg(test)]
    pub(super) fn cached_sizes(&self, icon: KindIcon) -> Vec<u32> {
        let mut sizes: Vec<u32> = self
            .rasters
            .keys()
            .filter(|(cached, _)| *cached == icon)
            .map(|(_, size)| *size)
            .collect();
        sizes.sort_unstable();
        sizes
    }
}

#[cfg(test)]
mod tests;
