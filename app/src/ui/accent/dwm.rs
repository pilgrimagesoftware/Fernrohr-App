//! The accent on Windows (Fernrohr#100): DWM's colorization colour, the one
//! the user picks under Personalization > Colors. A cheap, synchronous call,
//! read like macOS's.

use windows_sys::Win32::Foundation::BOOL;
use windows_sys::Win32::Graphics::Dwm::DwmGetColorizationColor;

/// The colorization colour as `0xAARRGGBB`, or `None` if DWM won't say.
pub(super) fn colorization() -> Option<u32> {
    let mut color: u32 = 0;
    let mut opaque: BOOL = 0;
    // SAFETY: both arguments point at live, writable locals of the types the
    // API documents (a `DWORD` and a `BOOL`); DWM only writes through them for
    // the duration of the call, and nothing else aliases them.
    let result = unsafe { DwmGetColorizationColor(&mut color, &mut opaque) };
    (result == 0).then_some(color)
}
