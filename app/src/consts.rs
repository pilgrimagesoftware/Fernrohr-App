//! Crate-wide constants: values that are a *decision* (how often to poll, how long to
//! wait) rather than a single element's layout. See `.claude/rules/rust-structure.md`.

use std::time::Duration;

/// The product name, as every window title and the About window spell it.
/// Not `env!("CARGO_PKG_NAME")`, which is the lowercase crate name `fernrohr`
/// (`window-title-and-menu` design.md Risks).
pub(crate) const APP_NAME: &str = "Fernrohr";

/// The app id every window is opened with: the Wayland `app_id` and the X11
/// `WM_CLASS`, which Linux desktops match against the installed
/// `fernrohr.desktop` (its name, and its `StartupWMClass`) to give the windows
/// that entry's name and icon and group them under it.
pub(crate) const APP_ID: &str = "fernrohr";

// UNWIRED(#3): `SshTransport` (tunnel-subsystem section 3) has no caller until section
// 6.2's connect-path integration, so dead_code analysis can't see these are reachable.
#[allow(dead_code)]
/// `SshTransport::connect` readiness probe: how long to keep retrying a local TCP
/// dial to the forwarded port before giving up and reporting a timeout.
pub(crate) const SSH_READINESS_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[allow(dead_code)]
/// `SshTransport::connect` readiness probe: delay between failed dial attempts.
pub(crate) const SSH_READINESS_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A command tunnel's default startup timeout: how long its command has to start
/// listening on the local port. Vendor CLIs that open an IAP session can take a while.
pub(crate) const COMMAND_TUNNEL_STARTUP_TIMEOUT_SECS: u64 = 30;
/// How often a starting command tunnel's local port is dialled to see if it's ready.
pub(crate) const COMMAND_TUNNEL_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// How long a stopping command tunnel's process group has between `SIGTERM` and
/// `SIGKILL`.
pub(crate) const COMMAND_TUNNEL_STOP_GRACE: Duration = Duration::from_secs(3);
/// How many of a command tunnel's most recent output lines a failure reason carries.
pub(crate) const COMMAND_TUNNEL_OUTPUT_LINES: usize = 50;
/// How long resolving the login shell's `PATH` may take before falling back.
pub(crate) const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// How long before an OIDC id-token's expiry it is already renewed (#188), so
/// it can't lapse between the check and the requests it authorizes.
pub(crate) const OIDC_EXPIRY_MARGIN: Duration = Duration::from_secs(60);

/// How long one request to an OIDC issuer may take, connecting included
/// (#188): discovery, then the token refresh.
pub(crate) const OIDC_ISSUER_TIMEOUT: Duration = Duration::from_secs(15);

// UNWIRED(#3): `tunnel_store::TunnelStore` (section 5.3) is the first real caller of
// the section 5.2 keychain wrapper this backs.
#[allow(dead_code)]
/// `keyring` service name for tunnel secrets (`account` is the tunnel id). A distinct
/// service from `keychain.rs`'s smoke-test constant so a spike credential never
/// collides with a real tunnel's stored key.
pub(crate) const TUNNEL_KEYCHAIN_SERVICE: &str = "com.pilgrimagesoftware.fernrohr.tunnels";

/// `connection-status-bar` design.md decision 2: a paused connection's status bar item
/// escalates from its reason's usual color to danger once it has been paused this long.
/// A single named constant so the "arbitrary" 30 seconds (design.md's own word) is easy
/// to find and retune after real use, rather than a literal buried in `severity`.
pub(crate) const STATUS_ESCALATE_AFTER: Duration = Duration::from_secs(30);

/// #121: a pod whose last container restart finished within this window shows
/// its restart count in the bad tone (red) - it is restarting *now*, whatever its
/// total.
pub(crate) const RECENT_RESTART_WINDOW: Duration = Duration::from_secs(15 * 60);

/// #121: above this many restarts (and none recent), a pod's restart count shows
/// in the serious tone (orange) rather than the warning tone (yellow).
pub(crate) const MANY_RESTARTS: i32 = 10;

/// #150: a Logs panel following a label selector starts each container at its
/// last this many lines, as `kubectl logs -l` starts at its last 10 - a
/// workload's pods' whole histories at once would bury the live lines.
pub(crate) const LABEL_LOGS_TAIL_LINES: i64 = 100;

/// #150: at most this many containers stream into one label-following Logs
/// panel at once - a log request each - like `kubectl logs -l`'s
/// `--max-log-requests`. The panel says how many more matched.
pub(crate) const LABEL_LOGS_MAX_STREAMS: usize = 20;

/// `connection-status-bar` design decision 4: how often the status bar refreshes elapsed
/// time and re-checks the escalation threshold while any item it shows is not connected.
/// Idle (fully connected) windows run no timer at all, so this only ever wakes a window
/// with a problem to show.
pub(crate) const STATUS_TICK_INTERVAL: Duration = Duration::from_secs(1);

/// `unwatchable-kinds`: how often a list panel re-lists a kind it can't watch -
/// one whose discovery has no `watch` verb, or whose list carries no
/// `resourceVersion` to start a watch from (`componentstatuses`).
pub(crate) const LIST_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// How long API discovery waits for one API group before reporting it unavailable
/// (`discovery-resilience`). kube retries a 503 with a backoff that can run for
/// minutes, and the other groups' kinds wait for the slowest group, so a dead
/// aggregated API must not hold the Resource panel's list back longer than this.
pub(crate) const DISCOVERY_GROUP_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a window must stop moving or resizing before its layout is saved
/// (Fernrohr#51). A drag fires a bounds change per frame; this coalesces them
/// into one write, while keeping a kill or crash from losing the layout.
pub(crate) const BOUNDS_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// The Resource panel's starting width, and how far its divider may be dragged. The
/// minimum keeps kind names readable; the maximum keeps the dock usable.
pub const RESOURCE_PANEL_WIDTH: gpui_kit::Pixels = gpui_kit::px(255.);
pub const RESOURCE_PANEL_MIN_WIDTH: gpui_kit::Pixels = gpui_kit::px(180.);
pub const RESOURCE_PANEL_MAX_WIDTH: gpui_kit::Pixels = gpui_kit::px(520.);

/// The Tunnels window's starting size (centered on screen) and the smallest it may be
/// resized to: room for the tunnel list above the editor, no more.
pub const TUNNELS_WINDOW_SIZE: gpui_kit::Size<gpui_kit::Pixels> =
    gpui_kit::size(gpui_kit::px(720.), gpui_kit::px(560.));
pub const TUNNELS_WINDOW_MIN_SIZE: gpui_kit::Size<gpui_kit::Pixels> =
    gpui_kit::size(gpui_kit::px(560.), gpui_kit::px(440.));

/// The Settings window's starting size (centered on screen) and the smallest it may be
/// resized to: the sections sidebar beside a list of every command and its key.
pub const SETTINGS_WINDOW_SIZE: gpui_kit::Size<gpui_kit::Pixels> =
    gpui_kit::size(gpui_kit::px(820.), gpui_kit::px(600.));
pub const SETTINGS_WINDOW_MIN_SIZE: gpui_kit::Size<gpui_kit::Pixels> =
    gpui_kit::size(gpui_kit::px(600.), gpui_kit::px(420.));

/// `pod-configuration-tab` 5.1: a ConfigMap value longer than this many characters, or
/// spanning more than one line, is large and starts collapsed. Secret values never collapse.
pub const COLLAPSE_VALUE_OVER_CHARS: usize = 100;

/// `pod-configuration-tab` 5.1: a collapsed value shows its first line's first this-many
/// characters plus an ellipsis.
pub const COLLAPSED_VALUE_PREVIEW_CHARS: usize = 20;

/// `visual-refresh-typography-spacing` design.md: every text size, in percent of the
/// default, that Increase/Decrease Text Size and the Settings stepper step through.
/// 10% steps from 90 to 150, plus an 85% floor for dense screens. Ascending.
pub(crate) const TEXT_SIZE_STEPS: [u16; 8] = [85, 90, 100, 110, 120, 130, 140, 150];

/// `pending-chord-indicator`: the Shortcut timeout preference's range and default,
/// in whole seconds - how long a chord whose keys so far are also a whole
/// binding waits for its next key before the shorter binding runs.
pub(crate) const SHORTCUT_TIMEOUT_MIN_SECS: u8 = 1;
pub(crate) const SHORTCUT_TIMEOUT_MAX_SECS: u8 = 10;
pub(crate) const SHORTCUT_TIMEOUT_DEFAULT_SECS: u8 = 3;

/// GPUI's own wait before it flushes an ambiguous pending chord - its
/// crate-private `PENDING_INPUT_TIMEOUT` in gpui-pre 0.3.7, which can't be read
/// from here. The status bar pauses that timer for the rest of the preference
/// and lets this much run out at the end, so the total is the preference. A test
/// pins the total wait, so a GPUI release that changes this fails it.
pub(crate) const GPUI_PENDING_INPUT_TIMEOUT: Duration = Duration::from_secs(1);

/// The frame and data roles' size at 100% text size: the rem every window uses,
/// gpui-component's own default, which every size the app used before the
/// text-size preference is relative to.
pub(crate) const DEFAULT_FONT_SIZE: f32 = 16.;
/// The code role's size at 100% text size: gpui-component's default mono size.
pub(crate) const DEFAULT_MONO_FONT_SIZE: f32 = 13.;

/// `resource-kind-icons`: how many device sizes the kind-icon raster cache keeps
/// at once (`ui::icon::element`). Three roles (text, small, header) on 1x and 2x
/// displays are six sizes in steady use; twice that leaves room for a text-size
/// change's new sizes before the old ones age out. Asking for one more size
/// releases the oldest.
pub(crate) const ICON_RASTER_SIZES: usize = 12;

/// How often a pod's Events tab re-checks its time window, so events age out of
/// it while the panel sits open (`pod-events-time-window` D2).
pub(crate) const POD_EVENTS_WINDOW_TICK: Duration = Duration::from_secs(60);

/// How long a quick look's selection must rest on a pod before the popover
/// restarts its event watch there, so scanning with Up/Down doesn't churn a
/// watch per row (`pod-quick-look` D2). The pod's fields follow at once.
pub(crate) const QUICK_LOOK_EVENTS_DEBOUNCE: Duration = Duration::from_millis(250);

/// The quick look's width bounds (`0-quick-look-layout`): as wide as its
/// fields need, at least `QUICK_LOOK_MIN_WIDTH`, and at most this share of the
/// window, itself capped at `QUICK_LOOK_MAX_WIDTH` - a long value past that
/// ellipsizes rather than widening the popover off the window.
pub(crate) const QUICK_LOOK_MIN_WIDTH: f32 = 320.;
pub(crate) const QUICK_LOOK_MAX_WIDTH: f32 = 720.;
pub(crate) const QUICK_LOOK_MAX_WIDTH_FRACTION: f32 = 0.6;

/// How often a Terminating pod's detail panel re-renders, so its grace period
/// counts down by the second (`live-detail-panels`).
pub(crate) const TERMINATING_COUNTDOWN_TICK: Duration = Duration::from_secs(1);

/// How opaque a detail panel draws a deleted object's last known state, kept
/// on screen below its "deleted at" banner (`live-detail-panels`).
pub(crate) const STALE_OPACITY: f32 = 0.6;

/// How many chunks of typed input may wait for the shell's stdin before the
/// terminal refuses more - room for a paste or a burst of keys, not a log.
pub(crate) const EXEC_INPUT_QUEUE: usize = 256;
/// How long a shell's terminal size must hold before it is sent to the
/// container (`embedded-exec-terminal` decision 1): a drag that resizes the
/// panel many times sends only the size it ends at.
pub(crate) const EXEC_RESIZE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(120);
/// How long a shell's exit status may take to arrive once its output has
/// ended. A connection that drops uncleanly may never deliver one; past this
/// the session ends as a lost connection rather than staying "running".
pub(crate) const EXEC_STATUS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// How long a note about refused input stays under the shell.
pub(crate) const EXEC_INPUT_NOTICE: std::time::Duration = std::time::Duration::from_secs(4);

/// A port-forward started from a resource row (`k9s-remaining-keybindings` 4):
/// how often its Pod is checked to still be Running, and how a failed check
/// backs off before trying again - the SSH tunnels' own pace.
pub(crate) const PORT_FORWARD_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);
pub(crate) const PORT_FORWARD_BACKOFF_INITIAL: Duration = Duration::from_secs(1);
pub(crate) const PORT_FORWARD_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// `config::saved_layouts::slugify`'s filename stem for a display name that
/// slugifies to nothing (all punctuation, or non-ASCII with no ASCII
/// fallback) - `saved-panel-layouts` design.md D2.
pub(crate) const SAVED_LAYOUT_PLACEHOLDER_STEM: &str = "layout";

/// `SavedLayout.version`: bumped on a breaking change to its on-disk shape
/// (`saved-panel-layouts` design.md D2).
pub(crate) const SAVED_LAYOUT_SCHEMA_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    /// The checked-in desktop entry groups windows by the app id they open with.
    #[test]
    fn the_desktop_entry_matches_the_window_app_id() {
        let entry = include_str!("../assets/linux/fernrohr.desktop");
        assert!(
            entry
                .lines()
                .any(|line| line == format!("StartupWMClass={}", super::APP_ID)),
            "fernrohr.desktop's StartupWMClass must be {}",
            super::APP_ID
        );
    }
}
