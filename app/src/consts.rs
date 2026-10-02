//! Crate-wide constants: values that are a *decision* (how often to poll, how long to
//! wait) rather than a single element's layout. See `.claude/rules/rust-structure.md`.

use std::time::Duration;

/// The product name, as every window title and the About window spell it.
/// Not `env!("CARGO_PKG_NAME")`, which is the lowercase crate name `fernrohr`
/// (`window-title-and-menu` design.md Risks).
pub(crate) const APP_NAME: &str = "Fernrohr";

// UNWIRED(#3): `SshTransport` (tunnel-subsystem section 3) has no caller until section
// 6.2's connect-path integration, so dead_code analysis can't see these are reachable.
#[allow(dead_code)]
/// `SshTransport::connect` readiness probe: how long to keep retrying a local TCP
/// dial to the forwarded port before giving up and reporting a timeout.
pub(crate) const SSH_READINESS_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[allow(dead_code)]
/// `SshTransport::connect` readiness probe: delay between failed dial attempts.
pub(crate) const SSH_READINESS_POLL_INTERVAL: Duration = Duration::from_millis(50);

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

/// `connection-status-bar` design decision 4: how often the status bar refreshes elapsed
/// time and re-checks the escalation threshold while any item it shows is not connected.
/// Idle (fully connected) windows run no timer at all, so this only ever wakes a window
/// with a problem to show.
pub(crate) const STATUS_TICK_INTERVAL: Duration = Duration::from_secs(1);

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

/// The frame and data roles' size at 100% text size: the rem every window uses,
/// gpui-component's own default, which every size the app used before the
/// text-size preference is relative to.
pub(crate) const DEFAULT_FONT_SIZE: f32 = 16.;
/// The code role's size at 100% text size: gpui-component's default mono size.
pub(crate) const DEFAULT_MONO_FONT_SIZE: f32 = 13.;

/// `resource-kind-icons`: how many device sizes the kind-icon raster cache keeps
/// at once (`ui::icon::element`). Two text roles on 1x and 2x displays, with room
/// for a text-size change; asking for one more size releases the oldest.
pub(crate) const ICON_RASTER_SIZES: usize = 6;
