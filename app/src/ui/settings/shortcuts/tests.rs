//! Tests for how a keystroke reads as a recording.

use super::super::recorder::{Recorded, recorded};
use gpui_kit::Keystroke;

fn key(keys: &str) -> Keystroke {
    Keystroke::parse(keys).expect("a valid keystroke")
}

#[test]
fn escape_alone_cancels_and_anything_else_is_recorded() {
    assert_eq!(recorded(&key("escape")), Recorded::Cancelled);
    // Compared as GPUI spells keys back: `cmd` reads `super` off macOS.
    assert_eq!(
        recorded(&key("cmd-escape")),
        Recorded::Keys(key("cmd-escape").unparse()),
        "a modified Escape is a key like any other"
    );
    assert_eq!(
        recorded(&key("cmd-shift-k")),
        Recorded::Keys(key("cmd-shift-k").unparse())
    );
    assert_eq!(recorded(&key("d")), Recorded::Keys("d".into()));
}
