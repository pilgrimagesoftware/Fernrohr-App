//! Refreshing an OIDC `auth-provider` id-token as `kubectl` does (#188).
//!
//! A kubeconfig user logged in through Dex (or any OIDC issuer) carries an
//! `auth-provider` named `oidc` whose config holds a short-lived `id-token`
//! and the `refresh-token`, `idp-issuer-url`, `client-id` and `client-secret`
//! to renew it. `kube` built without its `oidc` feature sends the stored
//! id-token as it is, so once it expires every request is a 401; with the
//! feature it renews only in memory, so the issuer's rotated refresh token is
//! lost and the kubeconfig - `kubectl`'s too - stops working.
//!
//! So before a config is built from the kubeconfig, an expired (or nearly
//! expired) id-token is renewed here: the issuer's token endpoint is found by
//! OIDC discovery, the refresh token traded for a new id-token, and both
//! written back to the kubeconfig file that defines the user. A 401 later in
//! the session re-resolves the config (`session::pods_watch`), which renews
//! again. No token is ever logged.

mod issuer;
mod lock;
mod persist;

use crate::consts::OIDC_EXPIRY_MARGIN;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jiff::Timestamp;
use kube::config::Kubeconfig;
use std::collections::HashMap;
use std::path::PathBuf;

pub(in crate::k8s::cluster) use persist::kubeconfig_files;

/// The `auth-provider` name this module renews.
const PROVIDER: &str = "oidc";
/// Config keys, as `kubectl`'s oidc provider names them.
const ID_TOKEN: &str = "id-token";
const REFRESH_TOKEN: &str = "refresh-token";

/// Renews the id-token of `context`'s user (the `current-context`'s when
/// `None`) if it authenticates through the `oidc` auth-provider and the token
/// has expired or is about to: `kubeconfig` is updated in place, and the new
/// tokens are saved to whichever of `files` defines the user. A user without
/// that provider, or with a token still good, is left alone.
///
/// One renewal per user at a time (`lock`): one that waited re-reads the file
/// and uses the tokens the other just stored rather than spending the
/// already-rotated refresh token again.
///
/// Fails, with a reason to show, only when renewing was needed and didn't
/// work; failing to save the renewed tokens is logged, not fatal - the
/// connection still uses them.
pub(in crate::k8s::cluster) async fn refresh_expired(
    kubeconfig: &mut Kubeconfig,
    context: Option<&str>,
    files: &[PathBuf],
) -> Result<(), String> {
    let Some(user) = user_of(kubeconfig, context) else {
        return Ok(());
    };
    let Some(config) = oidc_config(kubeconfig, &user) else {
        return Ok(());
    };
    let token_expiry = config.get(ID_TOKEN).and_then(|token| expiry(token));
    if !needs_refresh(token_expiry, Timestamp::now()) {
        return Ok(());
    }
    // A token this can't read may still be good (an opaque one): without the
    // means to renew it, leave it for the API server to judge, as before.
    if token_expiry.is_none() && !issuer::can_refresh(config) {
        return Ok(());
    }
    let mut config = config.clone();

    let file = persist::defining_file(files, &user);
    let _renewal = lock::acquire(file.as_deref(), &user).await?;
    // Another renewal may have finished while this one waited: take what it
    // stored, and use its id-token if that is good.
    if let Some(stored) = file
        .as_deref()
        .and_then(|file| persist::stored(file, &user))
    {
        for key in [ID_TOKEN, REFRESH_TOKEN] {
            if let Some(value) = stored.get(key) {
                config.insert(key.to_string(), value.clone());
            }
        }
        let stored_expiry = config.get(ID_TOKEN).and_then(|token| expiry(token));
        if !needs_refresh(stored_expiry, Timestamp::now()) {
            apply(kubeconfig, &user, &config);
            return Ok(());
        }
    }

    let tokens = issuer::refresh(&config)
        .await
        .map_err(|error| format!("the OIDC id-token for user {user} has expired, and {error}"))?;
    config.insert(ID_TOKEN.to_string(), tokens.id_token.clone());
    if let Some(refresh_token) = &tokens.refresh_token {
        config.insert(REFRESH_TOKEN.to_string(), refresh_token.clone());
    }
    apply(kubeconfig, &user, &config);
    match file.map(|file| persist::save(&file, &user, &tokens)) {
        Some(Ok(())) => log::info!("renewed the OIDC id-token for user {user}"),
        Some(Err(error)) => log::warn!(
            "renewed the OIDC id-token for user {user}, but couldn't save it to the kubeconfig: {error}"
        ),
        None => log::warn!(
            "renewed the OIDC id-token for user {user}, but no kubeconfig file defines that user to save it to"
        ),
    }
    Ok(())
}

/// Sets `user`'s id-token and refresh token in `kubeconfig` to `config`'s.
fn apply(kubeconfig: &mut Kubeconfig, user: &str, config: &HashMap<String, String>) {
    if let Some(target) = oidc_config_mut(kubeconfig, user) {
        for key in [ID_TOKEN, REFRESH_TOKEN] {
            if let Some(value) = config.get(key) {
                target.insert(key.to_string(), value.clone());
            }
        }
    }
}

/// The new tokens an issuer returned: always an id-token, and a refresh token
/// when the issuer rotates it (Dex does).
#[derive(Clone, PartialEq, Eq)]
pub(in crate::k8s::cluster) struct Tokens {
    pub(in crate::k8s::cluster) id_token: String,
    pub(in crate::k8s::cluster) refresh_token: Option<String>,
}

/// Never prints a token, should one end up in a log or panic message.
impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens")
            .field("id_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// The name of the user `context` (or the current context) authenticates as.
fn user_of(kubeconfig: &Kubeconfig, context: Option<&str>) -> Option<String> {
    let name = context.or(kubeconfig.current_context.as_deref())?;
    kubeconfig
        .contexts
        .iter()
        .find(|named| named.name == name)?
        .context
        .as_ref()?
        .user
        .clone()
}

/// `user`'s `oidc` auth-provider config, if that's how it authenticates.
fn oidc_config<'a>(kubeconfig: &'a Kubeconfig, user: &str) -> Option<&'a HashMap<String, String>> {
    let provider = kubeconfig
        .auth_infos
        .iter()
        .find(|named| named.name == user)?
        .auth_info
        .as_ref()?
        .auth_provider
        .as_ref()?;
    (provider.name == PROVIDER).then_some(&provider.config)
}

fn oidc_config_mut<'a>(
    kubeconfig: &'a mut Kubeconfig,
    user: &str,
) -> Option<&'a mut HashMap<String, String>> {
    let provider = kubeconfig
        .auth_infos
        .iter_mut()
        .find(|named| named.name == user)?
        .auth_info
        .as_mut()?
        .auth_provider
        .as_mut()?;
    (provider.name == PROVIDER).then_some(&mut provider.config)
}

/// Whether an id-token expiring at `expiry` must be renewed at `now`: it
/// expires within [`OIDC_EXPIRY_MARGIN`], or its expiry is unknown (it's
/// missing, or not a JWT with an `exp` claim) - as `kubectl` renews a token it
/// can't read.
fn needs_refresh(expiry: Option<Timestamp>, now: Timestamp) -> bool {
    match expiry {
        Some(expiry) => now
            .checked_add(OIDC_EXPIRY_MARGIN)
            .is_ok_and(|soon| expiry <= soon),
        None => true,
    }
}

/// The `exp` claim of a JWT. The signature isn't checked: the API server
/// does that, and this only decides when to renew.
fn expiry(jwt: &str) -> Option<Timestamp> {
    #[derive(serde::Deserialize)]
    struct Claims {
        exp: i64,
    }
    let payload = jwt.split('.').nth(1)?;
    let payload = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let claims: Claims = serde_json::from_slice(&payload).ok()?;
    Timestamp::from_second(claims.exp).ok()
}

#[cfg(test)]
pub(in crate::k8s::cluster) mod test_support;
#[cfg(test)]
mod tests;
