//! Tests for how a keystroke reads as a recording.

use super::super::recorder::{Recorded, recorded};
use gpui_kit::Keystroke;

fn key(keys: &str) -> Keystroke {
    Keystroke::parse(keys).expect("a valid keystroke")
}

#[test]
fn escape_alone_cancels_and_anything_else_is_recorded() {
    assert_eq!(recorded(&key("escape")), Recorded::Cancelled);
    assert_eq!(
        recorded(&key("cmd-escape")),
        Recorded::Keys("cmd-escape".into()),
        "a modified Escape is a key like any other"
    );
    assert_eq!(
        recorded(&key("cmd-shift-k")),
        Recorded::Keys("cmd-shift-k".into())
    );
    assert_eq!(recorded(&key("d")), Recorded::Keys("d".into()));
}
