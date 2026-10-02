//! The picker logo is drawn at exactly its device resolution.

use super::{LogoCache, Lookup, device_width, rasterize};
use crate::ui::picker::layout::{LOGO_HEIGHT, LOGO_WIDTH};

/// At 1x and 2x, the drawn raster is `round(LOGO_WIDTH x scale)` device
/// pixels wide, its height following the asset's aspect ratio - so it samples
/// 1:1 rather than leaving the GPU to shrink a 1241px source.
#[test]
fn the_raster_is_the_logos_device_size_at_1x_and_2x() {
    for scale in [1., 2.] {
        let width = device_width(scale);
        assert_eq!(width, (LOGO_WIDTH * scale).round() as u32);
        let raster = rasterize(width).expect("the embedded logo decodes");
        let size = raster.size(0);
        assert_eq!(size.width.0 as u32, width, "at {scale}x");
        let height = (LOGO_HEIGHT * scale).round() as i32;
        assert!(
            (size.height.0 - height).abs() <= 1,
            "at {scale}x the height {} keeps the aspect ratio ({height})",
            size.height.0
        );
    }
}

/// The resampled logo keeps its transparency: an opaque mark and a fully
/// transparent margin, not a flattened rectangle.
#[test]
fn the_resampled_logo_keeps_its_alpha() {
    let raster = rasterize(device_width(1.)).expect("the embedded logo decodes");
    let pixels = raster.as_bytes(0).expect("one frame");
    let bgra = pixels.as_chunks::<4>().0;
    assert!(bgra.iter().any(|[_, _, _, a]| *a == 255), "an opaque mark");
    assert!(
        bgra.iter().any(|[_, _, _, a]| *a == 0),
        "a transparent margin"
    );
}

/// A width is resampled once, and a third width releases the oldest.
#[test]
fn the_cache_keeps_two_widths() {
    let raster = rasterize(device_width(1.));
    let mut cache = LogoCache::default();
    for width in [192, 384] {
        assert!(matches!(cache.get_or_start(width), Lookup::Start));
        assert!(matches!(cache.get_or_start(width), Lookup::Pending));
        assert!(cache.finish(width, raster.clone()).is_empty());
        assert!(matches!(cache.get_or_start(width), Lookup::Ready(_)));
    }
    assert!(matches!(cache.get_or_start(288), Lookup::Start));
    let evicted = cache.finish(288, raster.clone());
    assert_eq!(evicted.len(), 1, "the oldest width, 192, is released");
    assert!(matches!(cache.get_or_start(192), Lookup::Start));
}

struct Sample;

impl gpui_kit::Render for Sample {
    fn render(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        super::logo(window, cx)
    }
}

/// The whole route: the first frame reserves the logo's box, the background
/// resample lands, and the redraw shows the raster at exactly the logo's
/// logical size - device pixels / scale factor, so 1:1.
#[gpui_kit::test]
fn the_logo_draws_its_raster_once_resampled(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::{AnyWindowHandle, VisualTestContext, px};

    let window: AnyWindowHandle = cx.add_window(|_, _| Sample).into();
    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.run_until_parked();
    let slot = vcx
        .debug_bounds("picker-logo")
        .expect("the logo's box is drawn");
    assert_eq!(slot.size.width, px(LOGO_WIDTH));
    let raster = vcx
        .debug_bounds("picker-logo-raster")
        .expect("the resampled logo is drawn once it lands");
    assert_eq!(raster.size.width, px(LOGO_WIDTH));
}
