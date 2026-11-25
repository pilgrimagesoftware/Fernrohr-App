//! Parsing and printing keystrokes for the OS the app was built for.
//!
//! gpui's `Keystroke::parse` takes only `&'static str`, and parsing or printing one with
//! `secondary-` needs the host OS (⌘ on macOS, ctrl elsewhere). Keys come from the keymap
//! file and from registered defaults alike, so every caller parses through [`parse`] and
//! prints with `Keystroke::unparse(HOST_OS)`.

use gpui_kit::{InvalidKeystrokeError, Keystroke, OperatingSystem};

/// The OS whose key conventions this build follows.
pub(crate) const HOST_OS: OperatingSystem = if cfg!(target_os = "macos") {
    OperatingSystem::Mac
} else if cfg!(target_os = "windows") {
    OperatingSystem::Windows
} else {
    OperatingSystem::Linux
};

/// `source` (`cmd-shift-p`, `secondary-k`, ...) as a keystroke on [`HOST_OS`].
pub(crate) fn parse(source: &str) -> Result<Keystroke, InvalidKeystrokeError> {
    Keystroke::parse_with_os(source, HOST_OS)
}

/// The platform modifier on [`HOST_OS`] (⌘ on macOS, ctrl elsewhere), for a test's
/// simulated click. gpui's test platform reports the host OS too, so this matches what
/// `Modifiers::secondary(cx)` checks there.
#[cfg(test)]
pub(crate) fn secondary_modifiers() -> gpui_kit::Modifiers {
    if HOST_OS == OperatingSystem::Mac {
        gpui_kit::Modifiers::command()
    } else {
        gpui_kit::Modifiers::control()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keystroke_round_trips() {
        for source in ["cmd-shift-p", "ctrl-alt-k", "enter", "f15"] {
            let keystroke = parse(source).expect("parses");
            assert_eq!(keystroke.unparse(HOST_OS), source);
        }
    }

    #[test]
    fn a_runtime_string_parses() {
        let owned = String::from("alt-1");
        assert!(parse(&owned).is_ok(), "not only &'static str");
    }
}
