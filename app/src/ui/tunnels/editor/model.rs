//! Owns `TunnelEditor`'s field state and the [`TunnelEditorEvent`] it reports back to
//! `TunnelsWindow`, plus the small parsing helpers construction and save share.

use super::*;

/// What the pane reports to `TunnelsWindow` so it can refresh its list and clear the
/// pane.
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
    pub(super) host: Entity<InputState>,
    pub(super) user: Entity<InputState>,
    pub(super) port: Entity<InputState>,
    pub(super) jump_hosts: Entity<InputState>,
    pub(super) auth: TunnelAuth,
    /// A new/replacement private key, pasted in. Left blank on an existing
    /// `KeychainKey` tunnel means "keep the stored secret" - `TunnelStore::update`'s
    /// own `None`-secret contract, so leaving this field alone changes nothing.
    pub(super) key_material: Entity<InputState>,
    pub(super) field_errors: Vec<TunnelFieldError>,
    /// A failure `field_errors` can't name (a duplicate id on create, a keychain/io
    /// error) - every other `TunnelStoreError` variant.
    pub(super) general_error: Option<String>,
    /// Contexts bound to this tunnel, read at open and after every save - what the
    /// delete confirmation names as falling back to Direct.
    pub(super) bound_contexts: Vec<String>,
    pub(super) confirming_delete: bool,
    pub(super) testing: bool,
    pub(super) test_result: Option<Result<(), String>>,
    pub(super) focus_handle: FocusHandle,
}
