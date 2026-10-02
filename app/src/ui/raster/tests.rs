//! Resampling embedded bitmaps to their device resolution.

use super::{Asset, Lookup, RasterCache, rasterize};

const LOGO: Asset = Asset {
    name: "test-logo",
    bytes: include_bytes!("../../../assets/fernrohr-logo.webp"),
    format: image::ImageFormat::WebP,
};

/// The resample keeps the artwork's transparency: an opaque mark and a fully
/// transparent margin, not a flattened rectangle.
#[test]
fn a_resampled_asset_keeps_its_alpha() {
    let raster = rasterize(LOGO, 192).expect("the asset decodes");
    let pixels = raster.as_bytes(0).expect("one frame");
    let bgra = pixels.as_chunks::<4>().0;
    assert!(bgra.iter().any(|[_, _, _, a]| *a == 255), "an opaque mark");
    assert!(
        bgra.iter().any(|[_, _, _, a]| *a == 0),
        "a transparent margin"
    );
}

/// Each width is resampled once; past two widths of one asset, its oldest is
/// released - and another asset's widths aren't touched.
#[test]
fn the_cache_keeps_two_widths_per_asset() {
    let raster = rasterize(LOGO, 64);
    let other = Asset {
        name: "test-other",
        ..LOGO
    };
    let mut cache = RasterCache::default();
    assert!(matches!(cache.get_or_start(other, 64), Lookup::Start));
    cache.finish(other, 64, raster.clone());
    for width in [192, 384] {
        assert!(matches!(cache.get_or_start(LOGO, width), Lookup::Start));
        assert!(matches!(cache.get_or_start(LOGO, width), Lookup::Pending));
        assert!(cache.finish(LOGO, width, raster.clone()).is_empty());
        assert!(matches!(cache.get_or_start(LOGO, width), Lookup::Ready(_)));
    }
    cache.get_or_start(LOGO, 288);
    let evicted = cache.finish(LOGO, 288, raster.clone());
    assert_eq!(evicted.len(), 1, "the oldest width, 192, is released");
    assert!(matches!(cache.get_or_start(LOGO, 192), Lookup::Start));
    assert!(
        matches!(cache.get_or_start(other, 64), Lookup::Ready(_)),
        "another asset's raster stays"
    );
}
