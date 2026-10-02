//! The Fernrohr logo above the picker card, drawn at its device resolution
//! through [`crate::ui::raster`] - the asset is 1241px wide and drawn at 192pt,
//! which `img()` alone would leave the GPU to shrink 3.2x-6.5x.
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
use crate::ui::raster::{self, Asset};
use gpui_kit::{AnyElement, App, Window};

/// The embedded logo.
pub(super) const LOGO: Asset = Asset {
    name: "picker-logo",
    bytes: include_bytes!("../../../assets/fernrohr-logo.webp"),
    format: image::ImageFormat::WebP,
};

/// The logo, `LOGO_WIDTH` x `LOGO_HEIGHT` at the window's resolution.
pub(super) fn logo(window: &mut Window, cx: &mut App) -> AnyElement {
    raster::resampled(LOGO, LOGO_WIDTH, LOGO_HEIGHT, "picker-logo", window, cx)
}

#[cfg(test)]
mod tests;
