//! Revealing a revision's values or manifest (helm-releases D6): fetched only
//! when the user asks, for the one tab and revision on screen, kept as a
//! [`SecretValue`], and dropped on hide, on a tab or revision change, and
//! with the panel. A manifest is redacted before it is kept.

use super::panel::{HelmReleasePanel, ReleaseTab};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::resource::helm::decode_release;
use crate::k8s::resource::helm::manifest::redacted_manifest;
use crate::k8s::resource::secret_value::SecretValue;
use gpui_kit::*;
use k8s_openapi::api::core::v1::Secret;
use kube::Api;

/// The text revealed in one tab, for one revision.
pub(crate) struct Revealed {
    pub(crate) tab: ReleaseTab,
    pub(crate) version: u32,
    pub(crate) state: RevealState,
}

pub(crate) enum RevealState {
    Loading,
    Shown(SecretValue),
    Failed(String),
}

impl HelmReleasePanel {
    /// The text revealed in `tab` for the selected revision, if it is.
    pub(crate) fn revealed_text(&self, tab: ReleaseTab, cx: &App) -> Option<&str> {
        let version = self.selected_revision(cx)?;
        match &self.revealed {
            Some(Revealed {
                tab: shown,
                version: of,
                state: RevealState::Shown(text),
            }) if *shown == tab && *of == version => text.expose(),
            _ => None,
        }
    }

    /// `r`: reveals the current tab's text for the selected revision, or
    /// hides it if it shows.
    pub(super) fn toggle_reveal(&mut self, cx: &mut Context<Self>) {
        if !self.tab.hidden_until_revealed() {
            return;
        }
        if self.revealed.take().is_some() {
            cx.notify();
            return;
        }
        let Some(version) = self.selected_revision(cx) else {
            return;
        };
        let Some(revision) = self
            .history(cx)
            .into_iter()
            .find(|revision| revision.version == version)
        else {
            return;
        };
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let (client, namespace, tab) = (client.clone(), self.key.namespace.clone(), self.tab);
        self.revealed = Some(Revealed {
            tab,
            version,
            state: RevealState::Loading,
        });
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let api: Api<Secret> = Api::namespaced(client, &namespace);
            let text = match api.get(&revision.secret).await {
                Ok(secret) => tokio::task::spawn_blocking(move || part(&secret, tab))
                    .await
                    .unwrap_or_else(|_| Err("the revision couldn't be read".to_string())),
                Err(_) => Err("the revision couldn't be fetched".to_string()),
            };
            let _ = tx.send(text).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |text| {
                let _ = this.update(cx, |this, cx| {
                    // Only if the same reveal is still asked for.
                    if let Some(revealed) = &mut this.revealed
                        && revealed.tab == tab
                        && revealed.version == version
                    {
                        revealed.state = match text {
                            Ok(text) => RevealState::Shown(text),
                            Err(reason) => RevealState::Failed(reason),
                        };
                        cx.notify();
                    }
                });
            })
            .await;
        })
        .detach();
        cx.notify();
    }

    /// From History: switches to `tab` on the selected revision.
    pub(super) fn view(&mut self, tab: ReleaseTab, cx: &mut Context<Self>) {
        if let Some(version) = self.selected_revision(cx) {
            self.selected = Some(version);
        }
        self.show(tab, cx);
    }

    /// Copies the current tab's revealed text - only ever offered while it
    /// shows.
    pub(super) fn copy_revealed(&self, tab: ReleaseTab, cx: &mut App) {
        if let Some(text) = self.revealed_text(tab, cx) {
            let text = text.to_string();
            crate::ui::copy::copy_text(&text, cx);
        }
    }
}

/// `tab`'s text from a revision Secret: its values, or its manifest redacted.
fn part(secret: &Secret, tab: ReleaseTab) -> Result<SecretValue, String> {
    let payload = secret
        .data
        .as_ref()
        .and_then(|data| data.get("release"))
        .ok_or_else(|| "the revision holds no release".to_string())?;
    let release = decode_release(&payload.0).map_err(|error| error.to_string())?;
    match tab {
        ReleaseTab::Values => Ok(release.values),
        _ => {
            let manifest = release.manifest.expose().unwrap_or_default();
            Ok(SecretValue::new(redacted_manifest(manifest).into_bytes()))
        }
    }
}
