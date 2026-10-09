//! Desktop notifications (`desktop-notifications`, manual-confirmation-tunnels
//! design D6): [`post`] puts a title and body in the operating system's
//! notification center and runs `on_activate` on the main thread if the user
//! clicks it, where the platform reports that.
//!
//! Posting happens on a blocking Tokio task, so neither the UI thread nor any
//! connection waits on the notification service. A failure - no service on a
//! Linux session, notifications denied, an unbundled macOS build - is logged
//! at `warn` and otherwise ignored: every notification has an in-app
//! counterpart that carries the same condition, so it is never the only
//! signal.
//!
//! Who posts is a [`Backend`]: the platform's ([`system`], through
//! `notify-rust`), or a fake in tests.

mod system;

use gpui_kit::{AnyWindowHandle, App, Global};
use std::sync::Arc;
use tokio::sync::oneshot;

/// One notification's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Note {
    pub(crate) title: String,
    pub(crate) body: String,
}

/// A notification a [`Backend`] showed.
pub(crate) enum Posted {
    /// Shown, on a platform that doesn't report what the user did with it.
    // Only the macOS backend returns it (D-Bus reports activation, and no
    // Windows backend shows anything), so elsewhere it is built in tests alone.
    #[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
    Shown,
    /// Shown; the function blocks until the user acts on it, and says whether
    /// they activated it (rather than dismissing it, or it expiring).
    // Only the D-Bus backend reports activation (`system`'s doc comment says
    // why macOS doesn't), so elsewhere it is built in tests alone.
    #[cfg_attr(not(any(test, all(unix, not(target_os = "macos")))), allow(dead_code))]
    Watching(Box<dyn FnOnce() -> bool + Send>),
}

/// Shows notifications. Called on a blocking thread, so it may block.
pub(crate) trait Backend: Send + Sync {
    fn post(&self, note: &Note) -> Result<Posted, String>;
}

/// The backend [`post`] uses in place of the platform's, when one is set.
struct ActiveBackend(Arc<dyn Backend>);

impl Global for ActiveBackend {}

/// Posts a notification with `title` and `body`, and runs `on_activate` on
/// the main thread if the user activates it. Returns at once.
pub(crate) fn post(
    title: impl Into<String>,
    body: impl Into<String>,
    on_activate: impl FnOnce(&mut App) + 'static,
    cx: &mut App,
) {
    let note = Note {
        title: title.into(),
        body: body.into(),
    };
    let backend = cx
        .try_global::<ActiveBackend>()
        .map(|active| active.0.clone())
        .unwrap_or_else(system::backend);
    let (activated, on_activation) = oneshot::channel::<()>();
    crate::runtime::handle(cx).spawn_blocking(move || match backend.post(&note) {
        Ok(Posted::Shown) => {}
        Ok(Posted::Watching(wait)) => {
            if wait() {
                let _ = activated.send(());
            }
        }
        Err(error) => log::warn!(
            target: "fernrohr::notify",
            "the notification {:?} wasn't shown: {error}",
            note.title
        ),
    });
    // Ends when the posting task does: activated, or dropped without it.
    cx.spawn(async move |cx| {
        if on_activation.await.is_ok() {
            cx.update(on_activate);
        }
    })
    .detach();
}

/// An `on_activate` for [`post`]: brings Fernrohr to the front with `window`
/// focused, if it is still open.
pub(crate) fn focus(window: AnyWindowHandle) -> impl FnOnce(&mut App) {
    move |cx| {
        cx.activate(true);
        // A window closed since the notification went up has nothing to focus.
        let _ = window.update(cx, |_, window, _| window.activate_window());
    }
}

/// Test-only: makes `backend` the one [`post`] uses.
#[cfg(test)]
pub(crate) fn set_backend(backend: Arc<dyn Backend>, cx: &mut App) {
    cx.set_global(ActiveBackend(backend));
}

#[cfg(test)]
mod tests;
