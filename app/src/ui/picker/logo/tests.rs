//! The picker logo is drawn at its full size and exactly its device
//! resolution.

use super::{LOGO, logo};
use crate::ui::picker::layout::{LOGO_HEIGHT, LOGO_WIDTH};
use crate::ui::raster::{device_width, rasterize};

/// At 1x and 2x, the logo's raster is `round(LOGO_WIDTH x scale)` device
/// pixels wide, its height following the asset's aspect ratio - so it samples
/// 1:1 rather than leaving the GPU to shrink a 1241px source.
#[test]
fn the_raster_is_the_logos_device_size_at_1x_and_2x() {
    for scale in [1., 2.] {
        let width = device_width(LOGO_WIDTH, scale);
        assert_eq!(width, (LOGO_WIDTH * scale).round() as u32);
        let raster = rasterize(LOGO, width).expect("the embedded logo decodes");
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

struct Sample;

impl gpui_kit::Render for Sample {
    fn render(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        logo(window, cx)
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
