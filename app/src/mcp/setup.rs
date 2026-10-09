//! What the user runs to register Fernrohr with an agent harness (design.md:
//! In-app agent setup): one command per harness, built from this
//! installation's own executable, so it works whatever `PATH` holds.
//!
//! [`HARNESSES`] is the one table of harnesses and their command syntax;
//! adding a harness, or following a harness's new syntax, touches nothing
//! else. [`AgentSetup::current`] decides what the Agent access section and
//! Copy MCP Setup Command can offer: commands, a warning that the executable
//! won't stay where it is, or nothing on a platform without the endpoint.
//!
//! Pure apart from reading the executable path once, and platform-neutral:
//! Windows builds it too, to say agent access is unavailable there.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// An agent harness Fernrohr knows how to register with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Harness {
    /// The harness's name as its users know it: "Claude Code".
    pub(crate) name: &'static str,
    /// A stable id for element ids and tests: "claude-code".
    pub(crate) id: &'static str,
    /// The copy button's tooltip, naming the harness.
    pub(crate) copy_tooltip: &'static str,
    /// The registration command, with `{exe}` where the shell-quoted
    /// executable goes. Every one registers at user scope.
    template: &'static str,
}

/// Every supported harness, in the order the section lists them.
pub(crate) const HARNESSES: [Harness; 4] = [
    Harness {
        name: "Claude Code",
        id: "claude-code",
        copy_tooltip: "Copy Claude Code command",
        template: "claude mcp add --scope user fernrohr -- {exe} mcp",
    },
    Harness {
        name: "Codex",
        id: "codex",
        copy_tooltip: "Copy Codex command",
        template: "codex mcp add fernrohr -- {exe} mcp",
    },
    Harness {
        name: "Gemini CLI",
        id: "gemini-cli",
        copy_tooltip: "Copy Gemini CLI command",
        template: "gemini mcp add --scope user fernrohr {exe} mcp",
    },
    Harness {
        name: "OpenCode",
        id: "opencode",
        copy_tooltip: "Copy OpenCode command",
        template: "opencode mcp add fernrohr --global -- {exe} mcp",
    },
];

/// The harness whose config also gets a snippet: its schema differs between
/// major versions and older releases lack `mcp add`.
pub(crate) const SNIPPET_HARNESS: &str = "opencode";

/// Where OpenCode's snippet goes, for the note beside it.
pub(crate) const SNIPPET_NOTE: &str =
    "Or in opencode.json, under mcp.fernrohr (1.x) or mcp.servers.fernrohr (2.x):";

impl Harness {
    /// The command registering `exe` with this harness.
    pub(crate) fn command(&self, exe: &Path) -> String {
        self.template
            .replace("{exe}", &shell_quote(&exe.to_string_lossy()))
    }
}

/// OpenCode's config entry for `exe`, as JSON.
pub(crate) fn opencode_snippet(exe: &Path) -> String {
    serde_json::json!({
        "type": "local",
        "command": [exe.to_string_lossy(), "mcp"],
    })
    .to_string()
}

/// `text` as one POSIX shell word: in single quotes, each `'` inside written
/// `'\''`. Nothing inside single quotes is special to a POSIX shell.
pub(crate) fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Why an executable path won't keep working.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unstable {
    /// macOS App Translocation: the app was opened straight from Downloads or
    /// a disk image, and runs from a random read-only copy that goes away.
    Translocated,
    /// A Cargo build directory (`target/debug`, `target/release`): the next
    /// build replaces it, and `cargo clean` removes it.
    BuildTree,
}

impl Unstable {
    /// What the section says instead of offering a command.
    pub(crate) fn explanation(self) -> &'static str {
        match self {
            Self::Translocated => {
                "Fernrohr is running from a temporary copy macOS made because it was \
                 opened from Downloads or a disk image, so a command using this path \
                 stops working once the app quits. Move Fernrohr to Applications, open \
                 it from there, then come back here."
            }
            Self::BuildTree => {
                "Fernrohr is running from a build directory, which the next build or \
                 `cargo clean` replaces, so a command using this path won't last. \
                 Install Fernrohr, open the installed app, then come back here."
            }
        }
    }
}

/// What agent setup can offer in this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentSetup {
    /// Commands for `exe`.
    Ready { exe: PathBuf },
    /// `exe` won't stay where it is: warn rather than offer a command.
    Unstable { exe: PathBuf, why: Unstable },
    /// No MCP endpoint on this platform, or no executable path to name.
    Unavailable,
}

impl AgentSetup {
    /// This process's setup, worked out once - the path doesn't change while
    /// it runs, and the section renders it every frame.
    pub(crate) fn current() -> &'static AgentSetup {
        static SETUP: OnceLock<AgentSetup> = OnceLock::new();
        SETUP.get_or_init(|| {
            if !cfg!(unix) {
                return AgentSetup::Unavailable;
            }
            Self::for_executable(
                std::env::current_exe().ok(),
                std::env::var_os("APPIMAGE").map(PathBuf::from),
            )
        })
    }

    /// The setup for a process whose executable is `current_exe`, or
    /// `appimage` when it runs from an AppImage - whose own executable is
    /// inside a mount that changes every launch.
    pub(crate) fn for_executable(current_exe: Option<PathBuf>, appimage: Option<PathBuf>) -> Self {
        let Some(exe) = appimage.filter(|path| path.is_absolute()).or(current_exe) else {
            return Self::Unavailable;
        };
        match unstable(&exe) {
            Some(why) => Self::Unstable { exe, why },
            None => Self::Ready { exe },
        }
    }
}

/// Why `exe` won't keep working, if it won't.
pub(crate) fn unstable(exe: &Path) -> Option<Unstable> {
    let parts: Vec<&str> = exe
        .components()
        .filter_map(|part| part.as_os_str().to_str())
        .collect();
    if parts.contains(&"AppTranslocation") {
        return Some(Unstable::Translocated);
    }
    // `target/debug`, `target/release`, or a cross build's
    // `target/<triple>/debug`.
    let build_tree = parts.iter().enumerate().any(|(index, part)| {
        *part == "target"
            && parts[index + 1..]
                .iter()
                .take(2)
                .any(|next| matches!(*next, "debug" | "release"))
    });
    build_tree.then_some(Unstable::BuildTree)
}

#[cfg(test)]
mod tests;
