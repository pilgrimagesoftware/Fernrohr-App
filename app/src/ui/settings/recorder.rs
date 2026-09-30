//! Recording one keystroke for the Keyboard Shortcuts editor.
//!
//! A keystroke interceptor sees each key-down before GPUI looks up bindings,
//! and stopping propagation there ends the dispatch, so the key being recorded
//! can't also run whatever it's bound to - recording `cmd-w` records it rather
//! than closing the window. Modifier-only presses never reach interceptors,
//! so they can't be recorded; chords are out of scope (one keystroke).

use gpui_kit::{AnyWindowHandle, App, Subscription, Window};

/// What a recording ended with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// A keystroke, as `keymap.toml` spells it (`cmd-shift-k`).
    Keys(String),
    /// Escape with no modifiers.
    Cancelled,
}

/// A recording in progress. Dropping it stops recording.
pub struct Recorder {
    _subscription: Subscription,
}

impl Recorder {
    /// Records key-downs in `window` - and only there - until dropped, calling
    /// `on_key` for each. Keys in other windows go through as usual.
    pub fn start(
        window: &Window,
        cx: &mut App,
        mut on_key: impl FnMut(Recorded, &mut Window, &mut App) + 'static,
    ) -> Self {
        let target: AnyWindowHandle = window.window_handle();
        let subscription = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != target {
                return;
            }
            cx.stop_propagation();
            on_key(recorded(&event.keystroke), window, cx);
        });
        Self {
            _subscription: subscription,
        }
    }
}

/// How a keystroke reads as a recording.
pub fn recorded(keystroke: &gpui_kit::Keystroke) -> Recorded {
    let modifiers = keystroke.modifiers;
    let unmodified = !(modifiers.control
        || modifiers.alt
        || modifiers.shift
        || modifiers.platform
        || modifiers.function);
    if keystroke.key == "escape" && unmodified {
        Recorded::Cancelled
    } else {
        Recorded::Keys(keystroke.unparse())
    }
}
