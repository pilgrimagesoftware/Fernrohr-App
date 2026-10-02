//! `resource-kind-icons` 3.4: an icon is rasterized at the device size it
//! covers, cached per size, and follows the text size beside it.

use super::{IconCache, IconSize, device_px, kind_icon};
use crate::config::ui::TextSize;
use crate::consts::ICON_RASTER_SIZES;
use crate::ui::icon::KindIcon;
use gpui_kit::{
    AnyWindowHandle, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Render, Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use std::sync::Arc;

/// A scale-factor change is a new cache key: the same logical size covers
/// twice the device pixels on a 2x display.
#[test]
fn the_device_size_follows_the_scale_factor() {
    assert_eq!(device_px(px(16.), 1.), 16);
    assert_eq!(device_px(px(16.), 2.), 32);
    assert_ne!(device_px(px(14.), 1.), device_px(px(14.), 2.));
    // Rounded to whole device pixels, so drawing it 1:1 lands on the grid.
    assert_eq!(device_px(px(14.), 1.5), 21);
    assert_eq!(device_px(px(13.3), 1.), 13);
}

/// The first request rasterizes; asking again for the same icon and size
/// hands back that same raster rather than drawing a new one. Another scale
/// factor's device size is cached beside it, not in its place.
#[gpui_kit::test]
fn a_raster_is_made_once_per_icon_and_device_size(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let renderer = cx.svg_renderer();
        let mut cache = IconCache::default();
        let (first, evicted) = cache.raster(KindIcon::Pod, 32, renderer.clone());
        let first = first.expect("the Pod icon rasterizes");
        assert!(evicted.is_empty());
        assert_eq!(first.size(0).width.0, 32, "rasterized at the device size");

        let (again, _) = cache.raster(KindIcon::Pod, 32, renderer.clone());
        assert!(
            Arc::ptr_eq(&first, &again.unwrap()),
            "served from the cache"
        );

        cache.raster(KindIcon::Pod, 16, renderer);
        assert_eq!(cache.cached_sizes(KindIcon::Pod), [16, 32]);
    });
}

/// Past its size budget the cache releases every raster at the oldest size,
/// and hands them back so the caller can free their textures.
#[gpui_kit::test]
fn the_oldest_size_is_released_past_the_budget(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let renderer = cx.svg_renderer();
        let mut cache = IconCache::default();
        let sizes: Vec<u32> = (0..=ICON_RASTER_SIZES as u32).map(|ix| 16 + ix).collect();
        let (oldest, _) = cache.raster(KindIcon::Pod, sizes[0], renderer.clone());
        cache.raster(KindIcon::Secret, sizes[0], renderer.clone());
        for &size in &sizes[1..ICON_RASTER_SIZES] {
            let (_, evicted) = cache.raster(KindIcon::Pod, size, renderer.clone());
            assert!(evicted.is_empty(), "{size} fits the budget");
        }
        let (_, evicted) = cache.raster(KindIcon::Pod, sizes[ICON_RASTER_SIZES], renderer);
        assert_eq!(evicted.len(), 2, "both icons' rasters at the oldest size");
        assert!(
            evicted
                .iter()
                .any(|image| Arc::ptr_eq(image, oldest.as_ref().unwrap()))
        );
        assert!(!cache.cached_sizes(KindIcon::Pod).contains(&sizes[0]));
        assert!(cache.cached_sizes(KindIcon::Secret).is_empty());
    });
}

struct Sample;

impl Render for Sample {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // `items_start`: a flex row stretches its children to its own height,
        // which would measure the row rather than the icon.
        div().flex().items_start().child(
            div().debug_selector(|| "icon-sm".into()).child(kind_icon(
                KindIcon::Pod,
                IconSize::Small,
                window,
                cx,
            )),
        )
    }
}

fn icon_height(cx: &mut TestAppContext, selector: &'static str) -> Pixels {
    let window: AnyWindowHandle = cx.add_window(|_, _| Sample).into();
    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.run_until_parked();
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .size
        .height
}

/// 3.4: an icon is as tall as the text beside it, and grows with the text
/// size. The set's icons are a little wider than tall, so the height is the
/// raster's, a touch under the text's.
#[gpui_kit::test]
fn the_icon_follows_the_text_size(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
    });
    let text = cx.update(|cx| IconSize::Small.logical(cx));
    let default_text = icon_height(cx, "icon-sm");
    assert!(
        default_text <= text && default_text >= text - px(1.),
        "{default_text:?} is within a pixel under its text's {text:?}"
    );

    cx.update(|cx| crate::ui::text_size::set(TextSize::MAX, cx));
    let larger = icon_height(cx, "icon-sm");
    let factor = TextSize::MAX.factor();
    assert!(
        (f32::from(larger) - f32::from(default_text) * factor).abs() <= 1.,
        "at {}% the icon is {larger:?}, not {default_text:?} scaled",
        TextSize::MAX.percent()
    );
}
