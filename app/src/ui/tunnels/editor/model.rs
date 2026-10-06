//! Owns `TunnelEditor`'s field state and the [`TunnelEditorEvent`] it reports back to
//! `TunnelsWindow`, plus the small parsing helpers construction and save share.

use super::*;

/// What the editor reports to `TunnelsWindow`, which closes its dialog and, after a
/// save or delete, refreshes the list.
pub enum TunnelEditorEvent {
    Saved,
    Deleted,
    Cancelled,
}

/// A fresh, process-unique tunnel id - stable identity is never derived from the
/// (editable) display name, so a rename never breaks an existing binding.
pub(super) fn generate_tunnel_id() -> String {
    format!("tunnel-{}", jiff::Timestamp::now().as_nanosecond())
}

/// Splits the jump-hosts field's free text on commas or newlines, trimming and
/// dropping empty entries, so "a, b,\nc" and "a,b,c" mean the same ordered list.
pub(super) fn split_jump_hosts(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

pub struct TunnelEditor {
    pub(super) tunnels_path: PathBuf,
    /// `None` for a brand new tunnel; `Some(id)` for an existing one.
    pub(super) editing_id: Option<String>,
    pub(super) name: Entity<InputState>,
    /// Which form is showing, and which kind Save writes.
    pub(super) kind: TunnelKind,
    pub(super) host: Entity<InputState>,
    pub(super) user: Entity<InputState>,
    pub(super) port: Entity<InputState>,
    pub(super) jump_hosts: Entity<InputState>,
    pub(super) auth: TunnelAuth,
    /// A new/replacement private key, pasted in. Left blank on an existing
    /// `KeychainKey` tunnel means "keep the stored secret" - `TunnelStore::update`'s
    /// own `None`-secret contract, so leaving this field alone changes nothing.
    pub(super) key_material: Entity<InputState>,
    /// The command form: multi-line, so a pasted backslash-continued command keeps
    /// its shape.
    pub(super) command_line: Entity<TextareaState>,
    pub(super) mode: CommandTunnelMode,
    pub(super) local_port: Entity<InputState>,
    pub(super) startup_timeout: Entity<InputState>,
    pub(super) field_errors: Vec<TunnelFieldError>,
    /// A failure `field_errors` can't name (a duplicate id on create, a keychain/io
    /// error) - every other `TunnelStoreError` variant.
    pub(super) general_error: Option<String>,
    /// Contexts bound to this tunnel, read at open and after every save - what the
    /// delete confirmation names as falling back to Direct.
    pub(super) bound_contexts: Vec<String>,
    pub(super) testing: bool,
    pub(super) test_result: Option<Result<(), String>>,
    pub(super) focus_handle: FocusHandle,
    /// The body's scroll, and a focus handle per section of it - grown as a form
    /// with more sections first shows - to tell which section holds focus.
    pub(super) scroll: ScrollHandle,
    pub(super) sections: Vec<FocusHandle>,
    /// The section last scrolled into view, so it is scrolled to only once.
    pub(super) revealed: Option<usize>,
}

impl TunnelEditor {
    /// Which form is showing. Test-only.
    #[cfg(test)]
    pub(in crate::ui::tunnels) fn kind(&self) -> TunnelKind {
        self.kind
    }

    /// The field holding focus, by label, if one does. Test-only.
    #[cfg(test)]
    pub(in crate::ui::tunnels) fn focused_field(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<&'static str> {
        let inputs = [
            ("Name", &self.name),
            ("Host", &self.host),
            ("User", &self.user),
            ("Port", &self.port),
            ("Jump hosts", &self.jump_hosts),
            ("Private key", &self.key_material),
            ("Local port", &self.local_port),
            ("Startup timeout", &self.startup_timeout),
        ];
        inputs
            .into_iter()
            .find(|(_, input)| input.read(cx).focus_handle(cx).is_focused(window))
            .map(|(label, _)| label)
            .or_else(|| {
                self.command_line
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
                    .then_some("Command")
            })
    }
}
