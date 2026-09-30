//! Section 4.2 of the `tunnel-management-ui` change: the Tunnels window's editor
//! pane - create, edit, rename, and delete one tunnel, with section 1.2's field
//! errors shown inline, an auth choice (SSH config/agent, or a private key imported
//! into the keychain), an ordered jump-host list, and a Delete confirmation naming the
//! contexts that fall back to Direct. Section 4.3 adds Test, alongside Save/Delete.
//!
//! `TunnelsWindow` (`list.rs`) owns whether the pane is open and for which tunnel;
//! this module owns what is inside it, reported back through [`TunnelEditorEvent`].

use crate::config::tunnels::{TunnelAuth, TunnelConfig};
use crate::tunnel::ssh::{SshTunnelConfig, TransientIdentityFile, test_connection};
use crate::tunnel::store::{TunnelFieldError, TunnelStore, TunnelStoreError};
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _};
use gpui_kit::*;
use std::path::PathBuf;

/// What the pane reports to `TunnelsWindow` so it can refresh its list and clear the
/// pane.
pub enum TunnelEditorEvent {
    Saved,
    Deleted,
    Cancelled,
}

/// A fresh, process-unique tunnel id - stable identity is never derived from the
/// (editable) display name, so a rename never breaks an existing binding.
fn generate_tunnel_id() -> String {
    format!("tunnel-{}", jiff::Timestamp::now().as_nanosecond())
}

/// Splits the jump-hosts field's free text on commas or newlines, trimming and
/// dropping empty entries, so "a, b,\nc" and "a,b,c" mean the same ordered list.
fn split_jump_hosts(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

pub struct TunnelEditor {
    tunnels_path: PathBuf,
    /// `None` for a brand new tunnel; `Some(id)` for an existing one.
    editing_id: Option<String>,
    name: Entity<InputState>,
    host: Entity<InputState>,
    user: Entity<InputState>,
    port: Entity<InputState>,
    jump_hosts: Entity<InputState>,
    auth: TunnelAuth,
    /// A new/replacement private key, pasted in. Left blank on an existing
    /// `KeychainKey` tunnel means "keep the stored secret" - `TunnelStore::update`'s
    /// own `None`-secret contract, so leaving this field alone changes nothing.
    key_material: Entity<InputState>,
    field_errors: Vec<TunnelFieldError>,
    /// A failure `field_errors` can't name (a duplicate id on create, a keychain/io
    /// error) - every other `TunnelStoreError` variant.
    general_error: Option<String>,
    /// Contexts bound to this tunnel, read at open and after every save - what the
    /// delete confirmation names as falling back to Direct.
    bound_contexts: Vec<String>,
    confirming_delete: bool,
    testing: bool,
    test_result: Option<Result<(), String>>,
    focus_handle: FocusHandle,
}

impl TunnelEditor {
    pub fn create(tunnels_path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(tunnels_path, None, window, cx)
    }

    pub fn edit(
        tunnels_path: PathBuf,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(tunnels_path, Some(id), window, cx)
    }

    fn new(
        tunnels_path: PathBuf,
        editing_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = TunnelStore::new(tunnels_path.clone());
        let existing = editing_id.as_deref().and_then(|id| store.get(id));
        let bound_contexts: Vec<String> = editing_id
            .as_deref()
            .map(|id| {
                let mut contexts: Vec<String> = store
                    .bindings()
                    .into_iter()
                    .filter(|(_, tunnel_id)| tunnel_id == id)
                    .map(|(context, _)| context)
                    .collect();
                contexts.sort();
                contexts
            })
            .unwrap_or_default();

        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let host = cx.new(|cx| {
            InputState::new(window, cx).placeholder("bastion.example.com, or an ssh config alias")
        });
        let user = cx.new(|cx| InputState::new(window, cx).placeholder("User"));
        let port = cx.new(|cx| InputState::new(window, cx).placeholder("22"));
        let jump_hosts = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("user@hop1, user@hop2 (optional, nearest-to-target last)")
        });
        let key_material = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Private key")
        });

        let auth = existing
            .as_ref()
            .map(|tunnel| tunnel.auth)
            .unwrap_or_default();

        if let Some(tunnel) = &existing {
            name.update(cx, |state, cx| {
                state.set_value(tunnel.name.clone(), window, cx)
            });
            host.update(cx, |state, cx| {
                state.set_value(tunnel.bastion_host.clone(), window, cx)
            });
            user.update(cx, |state, cx| {
                state.set_value(tunnel.bastion_user.clone(), window, cx)
            });
            port.update(cx, |state, cx| {
                state.set_value(tunnel.bastion_port.to_string(), window, cx)
            });
            jump_hosts.update(cx, |state, cx| {
                state.set_value(tunnel.jump_hosts.join(", "), window, cx)
            });
        } else {
            port.update(cx, |state, cx| state.set_value("22", window, cx));
        }

        Self {
            tunnels_path,
            editing_id,
            name,
            host,
            user,
            port,
            jump_hosts,
            auth,
            key_material,
            field_errors: Vec::new(),
            general_error: None,
            bound_contexts,
            confirming_delete: false,
            testing: false,
            test_result: None,
            focus_handle: cx.focus_handle(),
        }
    }

    fn set_auth(&mut self, auth: TunnelAuth, cx: &mut Context<Self>) {
        self.auth = auth;
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(TunnelEditorEvent::Cancelled);
    }

    fn request_delete(&mut self, cx: &mut Context<Self>) {
        self.confirming_delete = true;
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.confirming_delete = false;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.editing_id.clone() else {
            return;
        };
        let store = TunnelStore::new(self.tunnels_path.clone());
        match store.delete(&id) {
            Ok(_unbound) => {
                self.confirming_delete = false;
                cx.emit(TunnelEditorEvent::Deleted);
            }
            Err(error) => {
                self.general_error = Some(format!("could not delete this tunnel: {error:?}"));
            }
        }
        cx.notify();
    }

    /// Section 1.2/4.2: validates and writes the tunnel through `TunnelStore`,
    /// generating a fresh id on first save. `TunnelStoreError::Invalid` is shown
    /// inline per field; every other error is shown once, generically.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = TunnelStore::new(self.tunnels_path.clone());
        let name = self.name.read(cx).value().to_string();
        let host = self.host.read(cx).value().to_string();
        let user = self.user.read(cx).value().to_string();
        let port: u16 = self.port.read(cx).value().trim().parse().unwrap_or(0);
        let jump_hosts = split_jump_hosts(&self.jump_hosts.read(cx).value());
        let key_text = self.key_material.read(cx).value().to_string();
        let secret = (!key_text.is_empty()).then_some(key_text);

        let tunnel = TunnelConfig {
            name,
            bastion_user: user,
            bastion_host: host,
            bastion_port: port,
            jump_hosts,
            auth: self.auth,
        };

        let result = match self.editing_id.clone() {
            Some(id) => store.update(&id, tunnel, secret.as_deref()),
            None => {
                let id = generate_tunnel_id();
                let result = store.create(&id, tunnel, secret.as_deref());
                if result.is_ok() {
                    self.editing_id = Some(id);
                }
                result
            }
        };

        match result {
            Ok(()) => {
                self.field_errors.clear();
                self.general_error = None;
                self.key_material
                    .update(cx, |state, cx| state.set_value("", window, cx));
                cx.emit(TunnelEditorEvent::Saved);
            }
            Err(TunnelStoreError::Invalid(errors)) => {
                self.field_errors = errors;
                self.general_error = None;
            }
            Err(other) => {
                self.field_errors.clear();
                self.general_error = Some(format!("could not save this tunnel: {other:?}"));
            }
        }
        cx.notify();
    }

    /// Section 4.3: runs the connectivity test against the pane's current (possibly
    /// unsaved) field values, on the tokio runtime, without ever touching
    /// `ForwardRegistry` - see `tunnel::ssh::test_connection`.
    fn run_test(&mut self, cx: &mut Context<Self>) {
        let host = self.host.read(cx).value().to_string();
        let user = self.user.read(cx).value().to_string();
        let port: u16 = self.port.read(cx).value().trim().parse().unwrap_or(0);
        let jump_hosts = split_jump_hosts(&self.jump_hosts.read(cx).value());
        let key_text = self.key_material.read(cx).value().to_string();
        let id_for_identity = self
            .editing_id
            .clone()
            .unwrap_or_else(|| "unsaved-tunnel".to_string());
        let stored_secret = self.editing_id.as_deref().and_then(|id| {
            TunnelStore::new(self.tunnels_path.clone())
                .secret(id)
                .ok()
                .flatten()
        });
        let secret = if key_text.is_empty() {
            stored_secret
        } else {
            Some(key_text)
        };

        let identity_file = match secret
            .as_deref()
            .map(|secret| TransientIdentityFile::write(&id_for_identity, secret))
        {
            Some(Err(error)) => {
                self.test_result = Some(Err(format!("failed to prepare the test key: {error}")));
                cx.notify();
                return;
            }
            Some(Ok(file)) => Some(file),
            None => None,
        };

        let config = SshTunnelConfig {
            bastion_user: user,
            bastion_host: host,
            bastion_port: port,
            jump_hosts,
            remote_host: String::new(),
            remote_port: 0,
            local_port: 0,
            identity_file: identity_file.as_ref().map(TransientIdentityFile::path),
            known_hosts_file: None,
            ssh_config_file: None,
        };

        self.testing = true;
        self.test_result = None;
        cx.notify();

        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let result = test_connection(&config).await;
            let _ = tx.send(result).await;
            // Kept alive until the test finishes; Drop removes the file.
            drop(identity_file);
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, move |result| {
                let _ = this.update(cx, |editor, cx| {
                    editor.testing = false;
                    editor.test_result = Some(result);
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }
}

impl EventEmitter<TunnelEditorEvent> for TunnelEditor {}

impl Focusable for TunnelEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TunnelEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_new = self.editing_id.is_none();

        let field = |label: &'static str,
                     input: &Entity<InputState>,
                     error: Option<&'static str>| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(label),
                )
                .child(Input::new(input))
                .children(
                    error.map(|message| div().text_xs().text_color(theme.danger).child(message)),
                )
        };

        let host_error = self
            .field_errors
            .contains(&TunnelFieldError::EmptyHost)
            .then_some("Host is required.");
        let user_error = self
            .field_errors
            .contains(&TunnelFieldError::EmptyUser)
            .then_some("User is required.");
        let port_error = self
            .field_errors
            .contains(&TunnelFieldError::InvalidPort)
            .then_some("Port must be between 1 and 65535.");

        let auth_label = match self.auth {
            TunnelAuth::SshConfig => "SSH config/agent",
            TunnelAuth::KeychainKey => "Private key (keychain)",
        };
        let ssh_weak = cx.weak_entity();
        let key_weak = cx.weak_entity();
        let auth_button = Button::new("tunnel-auth")
            .label(auth_label)
            .icon(IconName::ChevronDown)
            .small()
            .outline()
            .dropdown_menu(move |menu, _window, _cx| {
                let ssh_weak = ssh_weak.clone();
                let key_weak = key_weak.clone();
                menu.item(PopupMenuItem::new("SSH config/agent").on_click(
                    move |_event, _window, cx| {
                        let _ = ssh_weak
                            .update(cx, |editor, cx| editor.set_auth(TunnelAuth::SshConfig, cx));
                    },
                ))
                .item(PopupMenuItem::new("Private key (keychain)").on_click(
                    move |_event, _window, cx| {
                        let _ = key_weak.update(cx, |editor, cx| {
                            editor.set_auth(TunnelAuth::KeychainKey, cx)
                        });
                    },
                ))
            });

        let key_field = matches!(self.auth, TunnelAuth::KeychainKey).then(|| {
            field(
                "Private key (leave blank to keep the existing one)",
                &self.key_material,
                None,
            )
        });

        let test_status = if self.testing {
            Some(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Testing…")
                    .into_any_element(),
            )
        } else {
            self.test_result.as_ref().map(|result| match result {
                Ok(()) => div()
                    .text_sm()
                    .text_color(theme.success)
                    .child("Reachable.")
                    .into_any_element(),
                Err(reason) => div()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(reason.clone())
                    .into_any_element(),
            })
        };

        let delete_confirm = self.confirming_delete.then(|| {
            let weak_confirm = cx.weak_entity();
            let weak_cancel = weak_confirm.clone();
            let message = if self.bound_contexts.is_empty() {
                "Delete this tunnel? No contexts are bound to it.".to_string()
            } else {
                let noun = if self.bound_contexts.len() == 1 {
                    "This context"
                } else {
                    "These contexts"
                };
                format!(
                    "Delete this tunnel? {noun} will fall back to a direct connection: {}",
                    self.bound_contexts.join(", ")
                )
            };
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_2()
                .border_1()
                .border_color(theme.danger)
                .rounded_md()
                .child(div().text_sm().child(message))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("tunnel-delete-confirm")
                                .label("Delete")
                                .danger()
                                .small()
                                .on_click(move |_event, _window, cx| {
                                    let _ = weak_confirm
                                        .update(cx, |editor, cx| editor.confirm_delete(cx));
                                }),
                        )
                        .child(
                            Button::new("tunnel-delete-cancel")
                                .label("Keep")
                                .outline()
                                .small()
                                .on_click(move |_event, _window, cx| {
                                    let _ = weak_cancel
                                        .update(cx, |editor, cx| editor.cancel_delete(cx));
                                }),
                        ),
                )
        });

        let weak_save = cx.weak_entity();
        let weak_test = cx.weak_entity();
        let weak_cancel_pane = cx.weak_entity();
        let weak_delete = cx.weak_entity();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .track_focus(&self.focus_handle)
            .child(div().text_base().font_semibold().child(if is_new {
                "New Tunnel"
            } else {
                "Edit Tunnel"
            }))
            .children(self.general_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(error.clone())
            }))
            .child(field("Name", &self.name, None))
            .child(field("Host", &self.host, host_error))
            .child(field("User", &self.user, user_error))
            .child(field("Port", &self.port, port_error))
            .child(field(
                "Jump hosts (comma-separated, optional)",
                &self.jump_hosts,
                None,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Authentication"),
                    )
                    .child(auth_button),
            )
            .children(key_field)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("tunnel-test")
                            .label("Test")
                            .outline()
                            .small()
                            .disabled(self.testing)
                            .on_click(move |_event, _window, cx| {
                                let _ = weak_test.update(cx, |editor, cx| editor.run_test(cx));
                            }),
                    )
                    .children(test_status),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("tunnel-save")
                                    .label("Save")
                                    .primary()
                                    .small()
                                    .on_click(move |_event, window, cx| {
                                        let _ = weak_save
                                            .update(cx, |editor, cx| editor.save(window, cx));
                                    }),
                            )
                            .child(
                                Button::new("tunnel-cancel")
                                    .label("Cancel")
                                    .ghost()
                                    .small()
                                    .on_click(move |_event, _window, cx| {
                                        let _ = weak_cancel_pane
                                            .update(cx, |editor, cx| editor.cancel(cx));
                                    }),
                            ),
                    )
                    .children((!is_new).then(|| {
                        Button::new("tunnel-delete")
                            .label("Delete")
                            .danger()
                            .small()
                            .on_click(move |_event, _window, cx| {
                                let _ =
                                    weak_delete.update(cx, |editor, cx| editor.request_delete(cx));
                            })
                    })),
            )
            .children(delete_confirm)
    }
}

// A sibling `tests.rs` rather than an inline module: this file is at the 700-line
// cap (rust-structure.md).
#[cfg(test)]
mod tests;
