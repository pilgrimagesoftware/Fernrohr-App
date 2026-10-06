//! The UI setup every test app starts from: `gpui_kit::init` with motion
//! reduced, so new harnesses get it without having to know why.
//!
//! A dialog slides in, and its `with_animation` frames are timed on the wall
//! clock (`Instant::now`), which the test executor's `advance_clock` never
//! moves. On a slow runner a button found in one frame has slid on by the
//! frame a click is hit-tested in, and the click misses: a picker stays open,
//! a Cancel lands on the backdrop. Reduced motion draws every animation at its
//! end, so an element stays where a test found it.

use gpui_kit::App;

/// `gpui_kit::init`, then reduced motion.
pub(crate) fn init(cx: &mut App) {
    gpui_kit::init(cx);
    cx.set_reduce_motion(true);
}
