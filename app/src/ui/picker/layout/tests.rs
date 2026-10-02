// `use super::*` here would re-import `gpui_kit`'s `test` attribute macro (this file's
// `use gpui_kit::*` brings it in), shadowing the builtin `#[test]` and sending a plain
// sync test into `#[gpui_kit::test]`'s async-runtime expansion instead - hence the
// explicit imports below rather than a glob.
use super::{
    CARD_CHROME_HEIGHT, CARD_LIST_MAX_HEIGHT, CARD_WIDTH, LOGO_HEIGHT, LOGO_WIDTH, MIN_WINDOW_SIZE,
    PICKER_CONTENT_GAP,
};

/// Guards the `include_bytes!` in [`super::logo`]: a truncated or placeholder
/// asset compiles fine and only fails as a blank gap in the UI. WebP's RIFF
/// header carries the total file size, so a length that disagrees with it
/// catches truncation exactly rather than by proxy.
#[test]
fn the_embedded_logo_is_a_complete_webp() {
    let bytes = include_bytes!("../../../../assets/fernrohr-logo.webp");
    assert!(bytes.starts_with(b"RIFF"), "missing RIFF signature");
    assert_eq!(&bytes[8..12], b"WEBP", "RIFF payload is not WebP");

    let declared = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    assert_eq!(
        declared + 8,
        bytes.len(),
        "RIFF size disagrees with file length - asset looks truncated"
    );

    // The lossless full-resolution encode is ~1.14MB. The floor catches a
    // placeholder; the ceiling catches either the 1.6MB brand source in
    // `images/` or a lossy re-encode being committed here by mistake - this
    // asset is required to be bit-identical to the master, and both of those
    // are not.
    assert!(
        (1_000_000..1_400_000).contains(&bytes.len()),
        "logo is {} bytes, outside the expected lossless full-resolution range",
        bytes.len()
    );
}

/// The window minimum exists so the picker's centred column never overflows and
/// clips the logo. These pin the composition it is derived from, and catch the
/// one mistake derivation cannot catch on its own: a minimum *larger* than the
/// default window, which would open every fresh window already violating its own
/// floor.
#[test]
fn the_window_minimum_fits_the_picker() {
    let min_width = f32::from(MIN_WINDOW_SIZE.width);
    let min_height = f32::from(MIN_WINDOW_SIZE.height);

    assert!(
        min_width >= CARD_WIDTH,
        "minimum width {min_width} is narrower than the {CARD_WIDTH}px card"
    );

    let required = LOGO_HEIGHT + PICKER_CONTENT_GAP + CARD_CHROME_HEIGHT + CARD_LIST_MAX_HEIGHT;
    assert!(
        min_height >= required,
        "minimum height {min_height} cannot fit the picker, which needs {required}"
    );

    let default_layout = crate::config::workspace::WindowLayout::default();
    assert!(
        min_width <= default_layout.width && min_height <= default_layout.height,
        "minimum {}x{} exceeds the default window {}x{} - a fresh window would open \
         already violating its own minimum",
        min_width,
        min_height,
        default_layout.width,
        default_layout.height
    );
}

/// The asset is re-encoded at full source resolution precisely so that rendering it
/// at `LOGO_WIDTH` stays sharp on a Retina display, and `LOGO_HEIGHT` is typed to its
/// aspect so the mark is not stretched. Neither link is enforced at runtime, so read
/// the real canvas out of the shipped bytes and hold both.
#[test]
fn the_embedded_logo_is_large_enough_for_the_size_it_renders_at() {
    /// Canvas dimensions of `app/assets/fernrohr-logo.webp`. A lossless WebP is a
    /// single `VP8L` chunk - unlike the lossy-with-alpha `VP8X` extended format -
    /// and packs the canvas into a 32-bit field: 12 bytes of RIFF header, then the
    /// chunk id and its length, then a 0x2f signature byte, then width-1 in bits
    /// 0-13 and height-1 in bits 14-27.
    fn canvas_size() -> (usize, usize) {
        let bytes = include_bytes!("../../../../assets/fernrohr-logo.webp");
        assert_eq!(&bytes[12..16], b"VP8L", "expected a lossless (VP8L) WebP");
        assert_eq!(bytes[20], 0x2f, "missing the VP8L signature byte");
        let packed = u32::from_le_bytes(bytes[21..25].try_into().unwrap());
        (
            (packed & 0x3fff) as usize + 1,
            ((packed >> 14) & 0x3fff) as usize + 1,
        )
    }

    let (width, height) = canvas_size();
    assert!(
        width >= (2. * LOGO_WIDTH) as usize,
        "asset is {width}px wide but renders at {LOGO_WIDTH}px - soft on Retina"
    );

    let asset_aspect = width as f32 / height as f32;
    let rendered_aspect = LOGO_WIDTH / LOGO_HEIGHT;
    assert!(
        (asset_aspect - rendered_aspect).abs() < 0.01,
        "asset aspect {asset_aspect} does not match the rendered {rendered_aspect} - \
         the mark would be stretched"
    );
}
