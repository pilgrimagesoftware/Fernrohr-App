//! Section 4.2 of the `tunnel-management-ui` change: the Tunnels window's editor
//! pane - create, edit, rename, and delete one tunnel, with section 1.2's field
//! errors shown inline, an auth choice (SSH config/agent, or a private key imported
//! into the keychain), an ordered jump-host list, and a Delete confirmation naming the
//! contexts that fall back to Direct. Section 4.3 adds Test, alongside Save/Delete.
//!
//! `command-tunnels` adds a kind switch at the top: the SSH form, or the command form
//! (`command_form`) - command line, Proxy/Forward mode, optional fixed port, startup
//! timeout. Both forms keep their values while the other is showing.
//!
//! `TunnelsWindow` (`list.rs`) owns whether the pane is open and for which tunnel;
//! this module owns what is inside it, reported back through [`TunnelEditorEvent`].

use super::notify_tunnels_changed;
use crate::config::tunnels::{
    CommandTunnelConfig, CommandTunnelMode, TunnelAuth, TunnelConfig, TunnelKind,
};
use crate::tunnel::ssh::{SshTunnelConfig, TransientIdentityFile, test_connection};
use crate::tunnel::store::{TunnelFieldError, TunnelStore, TunnelStoreError};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _};
use gpui_kit::*;
use std::path::PathBuf;

mod actions;
mod command_form;
mod construction;
mod model;
mod render;
mod traits;

use model::*;

#[cfg(test)]
pub(super) use command_form::TIMEOUT_FIELD_SELECTOR;
pub use model::{TunnelEditor, TunnelEditorEvent};
