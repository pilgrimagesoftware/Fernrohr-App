//! The platform's notification service, through `notify-rust`: the
//! freedesktop D-Bus service on Linux and the BSDs, the notification center
//! on macOS, and nothing on Windows.
//!
//! Only the D-Bus service reports activation without help from the main run
//! loop. macOS's `NSUserNotificationCenter` reports a click only through a
//! call that blocks on the main run loop and sends the notification itself,
//! so on macOS a notification is posted without one: clicking it still
//! brings Fernrohr forward, just not to a particular window.

use super::{Backend, Note, Posted};
use std::sync::Arc;

pub(super) fn backend() -> Arc<dyn Backend> {
    Arc::new(System)
}

struct System;

#[cfg(target_os = "macos")]
impl Backend for System {
    fn post(&self, note: &Note) -> Result<Posted, String> {
        static APPLICATION: std::sync::Once = std::sync::Once::new();
        APPLICATION.call_once(|| {
            // Fails only when it was set already, or in a build macOS doesn't
            // know by that identifier (`cargo run`); the spec accepts that a
            // development build may show nothing.
            if let Err(error) = notify_rust::set_application(crate::consts::BUNDLE_IDENTIFIER) {
                log::debug!(target: "fernrohr::notify", "notifications as {}: {error}", crate::consts::BUNDLE_IDENTIFIER);
            }
        });
        // The handle sends the notification when it drops, not waiting for
        // the user.
        notify_rust::Notification::new()
            .summary(&note.title)
            .body(&note.body)
            .show()
            .map_err(|error| error.to_string())?;
        Ok(Posted::Shown)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Backend for System {
    fn post(&self, note: &Note) -> Result<Posted, String> {
        /// The action a click on the notification's body reports.
        const DEFAULT_ACTION: &str = "default";
        let handle = notify_rust::Notification::new()
            .appname(crate::consts::APP_NAME)
            .summary(&note.title)
            .body(&note.body)
            // Without a default action, a click on the body reports nothing.
            .action(DEFAULT_ACTION, "Show")
            .show()
            .map_err(|error| error.to_string())?;
        Ok(Posted::Watching(Box::new(move || {
            let mut activated = false;
            handle.wait_for_action(|action| activated = action == DEFAULT_ACTION);
            activated
        })))
    }
}

#[cfg(not(unix))]
impl Backend for System {
    fn post(&self, _: &Note) -> Result<Posted, String> {
        Err("desktop notifications aren't supported on this platform".to_string())
    }
}
