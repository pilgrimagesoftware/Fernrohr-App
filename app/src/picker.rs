//! The cluster picker: shown in place of a window's panel workspace whenever that window
//! has no open or restored panels. Lists kubeconfig contexts through the same searchable
//! `Command` widget the app-wide command palette uses, drives a connection through
//! `ClusterRegistry`, and emits [`PickerEvent::Connected`] on success so `shell::MainWindow`
//! can switch that window into its normal panel workspace.

use crate::cluster::connection::{ClusterConnection, ConnectionState};
use crate::cluster::kubeconfig;
use crate::cluster::session::ClusterRegistry;
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::*;
use std::sync::{Arc, LazyLock};

pub enum PickerEvent {
    Connected { context_name: String },
}

/// One attempt in flight: which context was picked and its connection entity, so the
/// picker can render `Connecting`/`WaitingForTunnel`/`Failed` and let the user retry.
struct Attempt {
    context_name: String,
    connection: Entity<ClusterConnection>,
}

pub struct ClusterPicker {
    contexts: Result<Vec<String>, String>,
    command_state: Entity<CommandState>,
    attempt: Option<Attempt>,
    focus_handle: FocusHandle,
    /// Test-only stand-in for `ClusterRegistry::connection`. Real connections spawn
    /// tokio work on a runtime worker thread, which gpui's test scheduler rejects
    /// as cross-thread nondeterminism - so tests substitute a stub instead of
    /// driving a real connect.
    #[cfg(test)]
    connection_factory: Option<fn(&mut App, &str) -> Entity<ClusterConnection>>,
}

impl ClusterPicker {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            contexts: kubeconfig::list_context_names(None).map_err(|error| error.to_string()),
            command_state: cx.new(|cx| CommandState::new(window, cx)),
            attempt: None,
            focus_handle: cx.focus_handle(),
            #[cfg(test)]
            connection_factory: None,
        }
    }

    /// The connection a [`Self::select`] attempt should observe. Production always
    /// goes through the shared registry; only tests take the stub path.
    fn new_connection(
        &self,
        context_name: &str,
        cx: &mut Context<Self>,
    ) -> Entity<ClusterConnection> {
        #[cfg(test)]
        if let Some(factory) = self.connection_factory {
            return factory(cx, context_name);
        }
        ClusterRegistry::connection(cx, context_name)
    }

    fn select(&mut self, context_name: String, cx: &mut Context<Self>) {
        let connection = self.new_connection(&context_name, cx);
        cx.observe(&connection, {
            let context_name = context_name.clone();
            move |_this: &mut Self, connection, cx| {
                if let ConnectionState::Connected(_) = &connection.read(cx).state {
                    cx.emit(PickerEvent::Connected {
                        context_name: context_name.clone(),
                    });
                }
                cx.notify();
            }
        })
        .detach();
        self.attempt = Some(Attempt {
            context_name,
            connection,
        });
        cx.notify();
    }
}

impl EventEmitter<PickerEvent> for ClusterPicker {}

impl Focusable for ClusterPicker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Width of the picker card, matching the app's command-palette chrome.
const CARD_WIDTH: f32 = 480.;

/// A centered card matching the app's command-palette chrome (`popover`
/// surface, `border` outline, `shadow_lg`), so the picker reads as part of
/// the same design system rather than a bespoke first-run screen.
fn card(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .w(px(CARD_WIDTH))
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .shadow_lg()
        .flex()
        .flex_col()
        .gap_4()
        .p_6()
}

/// Displayed width of the logo, in logical pixels. The embedded asset is 1241px
/// wide, so this stays sharp to ~2x on a Retina display with room to spare.
const LOGO_WIDTH: f32 = 192.;

/// Displayed height, derived from the asset's 1241x1183 geometry so the mark is
/// never stretched.
const LOGO_HEIGHT: f32 = 183.;

// The picker's own layout, in logical pixels, as a sum the window minimum can be
// derived from. These track the tailwind-style spacing the render uses (`p_6`,
// `gap_4`, `gap_6` are 1.5rem/1rem/1.5rem at 16px per rem), and the `Command`
// list's default `max_h` of 18.75rem. Deriving the minimum from them rather than
// hardcoding a number is the point: doubling the logo above would otherwise have
// silently started clipping the picker.

/// Card padding top and bottom, plus the header row and the gap beneath it.
const CARD_CHROME_HEIGHT: f32 = 48. + 48. + 16.;

/// `Command`'s list at its `max_h` cap - the tallest it gets, however many
/// contexts the kubeconfig has. Sizing for the cap rather than a typical list is
/// what keeps the picker from clipping on a machine with many contexts.
const CARD_LIST_MAX_HEIGHT: f32 = 300.;

/// Gap between the logo and the card (`gap_6`).
const PICKER_CONTENT_GAP: f32 = 24.;

/// Breathing room either side of the 480px card, so the minimum does not let it
/// sit flush against the window edge.
const PICKER_SIDE_MARGIN: f32 = 40.;

/// Minimum window size that fits the picker without clipping.
///
/// Set at window creation and not adjustable afterwards - gpui exposes
/// `window_min_size` only on `WindowOptions`, with no setter on `Window`. A
/// picker window is the same window the workspace is later shown in, so this
/// also floors the workspace: 560x619 against a 1024x768 default leaves the
/// dock room to breathe while still letting a user shrink a long way down.
pub const MIN_WINDOW_SIZE: Size<Pixels> = Size {
    width: px(CARD_WIDTH + 2. * PICKER_SIDE_MARGIN),
    height: px(LOGO_HEIGHT + PICKER_CONTENT_GAP + CARD_CHROME_HEIGHT + CARD_LIST_MAX_HEIGHT),
};

/// The Fernrohr logo, centered above the picker card.
///
/// Embedded at compile time rather than resolved from disk at runtime, so the
/// picker is unaffected by the working directory or by how the app is packaged.
/// 1.14MB is not a reason to load it at runtime: that is under 1% of even a
/// debug build, and it buys back a missing-file failure mode on a decorative
/// asset plus new packaging work for every platform's bundle layout.
///
/// The asset is **lossless** - verified bit-identical to the master, alpha
/// included - because a lossy encode is not good enough at this size. The mark
/// is effectively photographic (259,771 distinct colours across 1.57M pixels, so
/// continuous gradient rather than flat fills), which is why lossless WebP only
/// reaches 1.14MB against the 1.6MB PNG instead of the 6x a graphic would give.
/// An earlier q90 encode was 215KB and scored PSNR 40dB, but that reads as
/// clean at 96px and shows at 192px, and PSNR is the wrong instrument for
/// judging a logo.
///
/// Reproduce with (note the ordering - `-preset` must precede `-lossless`, or it
/// silently overwrites it and the output is lossy despite the flag):
///
/// ```sh
/// magick images/fernrohr-logo.png -trim +repage trimmed.png
/// cwebp -preset picture -lossless -z 9 -m 6 trimmed.png -o app/assets/fernrohr-logo.webp
/// ```
///
/// The asset is trimmed to its alpha bounding box so the rendered height tracks
/// the mark rather than the source canvas, which carried ~60px of horizontal and
/// ~90px of vertical transparent margin.
fn logo() -> impl IntoElement {
    // Built once and reused: `Image`'s `Hash` impl hashes its own bytes, which
    // is what gpui's asset cache keys on, so only the first render decodes it.
    static LOGO: LazyLock<Arc<Image>> = LazyLock::new(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Webp,
            include_bytes!("../assets/fernrohr-logo.webp").to_vec(),
        ))
    });

    img(LOGO.clone())
        .w(px(LOGO_WIDTH))
        .h(px(LOGO_HEIGHT))
        .object_fit(ObjectFit::Contain)
}

fn header(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .items_center()
        .gap_3()
        .child(
            Icon::new(IconName::Server)
                .size(px(28.))
                .text_color(theme.accent),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(div().text_lg().font_semibold().child("Select a cluster"))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Choose a kubeconfig context to connect to"),
                ),
        )
}

impl Render for ClusterPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        // The logo sits outside the card so it reads as app branding rather than
        // as part of the command-palette chrome the card deliberately mimics.
        let backdrop = |content: AnyElement| {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.background)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_6()
                        .child(logo())
                        .child(content),
                )
        };

        let contexts = match &self.contexts {
            Ok(contexts) if !contexts.is_empty() => contexts.clone(),
            Ok(_) => {
                return backdrop(
                    card(cx)
                        .child(header(cx))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("No kubeconfig contexts are available."),
                        )
                        .track_focus(&self.focus_handle)
                        .into_any_element(),
                )
                .into_any_element();
            }
            Err(error) => {
                return backdrop(
                    card(cx)
                        .child(header(cx))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.danger)
                                .child(format!("Could not read kubeconfig: {error}")),
                        )
                        .track_focus(&self.focus_handle)
                        .into_any_element(),
                )
                .into_any_element();
            }
        };

        let this = cx.weak_entity();
        let items: Vec<CommandItem> = contexts
            .iter()
            .map(|name| {
                CommandItem::new()
                    .icon(Icon::new(IconName::Server))
                    .label(name.clone())
            })
            .collect();
        let command_contexts = contexts.clone();
        let command = Command::new(&self.command_state)
            .items(items)
            .placeholder("Search contexts...")
            .on_confirm(move |index_path, _window, cx| {
                let Some(context_name) = command_contexts.get(index_path.row).cloned() else {
                    return;
                };
                let _ = this.update(cx, |this, cx| this.select(context_name, cx));
            });

        let status = self.attempt.as_ref().map(|attempt| {
            let context_name = attempt.context_name.clone();
            let (text, color) = match &attempt.connection.read(cx).state {
                ConnectionState::Connecting => (
                    format!("Connecting to {context_name}..."),
                    theme.muted_foreground,
                ),
                ConnectionState::WaitingForTunnel => (
                    format!("Waiting for tunnel to {context_name}..."),
                    theme.muted_foreground,
                ),
                ConnectionState::Failed(reason) => (
                    format!("Could not connect to {context_name}: {reason}"),
                    theme.danger,
                ),
                ConnectionState::Connected(_) => {
                    (format!("Connected to {context_name}"), theme.success)
                }
            };
            div().text_sm().text_color(color).child(text)
        });

        backdrop(
            card(cx)
                .child(header(cx))
                .child(command)
                .children(status)
                .track_focus(&self.focus_handle)
                .into_any_element(),
        )
        .into_any_element()
    }
}

// `use super::*` here would re-import `gpui_kit`'s `test` attribute macro (this file's
// `use gpui_kit::*` brings it in), shadowing the builtin `#[test]` and sending a plain
// sync test into `#[gpui_kit::test]`'s async-runtime expansion instead - hence the
// explicit imports below rather than a glob.
#[cfg(test)]
mod tests {
    use super::{
        CARD_CHROME_HEIGHT, CARD_LIST_MAX_HEIGHT, CARD_WIDTH, LOGO_HEIGHT, LOGO_WIDTH,
        MIN_WINDOW_SIZE, PICKER_CONTENT_GAP,
    };
    use crate::cluster::kubeconfig;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    const FIXTURE: &str = r#"
apiVersion: v1
kind: Config
clusters:
  - name: kind-dev
    cluster:
      server: https://127.0.0.1:6443
contexts:
  - name: kind-dev
    context:
      cluster: kind-dev
      user: kind-dev
current-context: kind-dev
users:
  - name: kind-dev
    user: {}
"#;

    fn fixture_path() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fernrohr-picker-fixture-{n}.yaml"));
        fs::write(&path, FIXTURE).unwrap();
        path
    }

    #[test]
    fn lists_contexts_from_a_valid_kubeconfig() {
        let path = fixture_path();
        let contexts = kubeconfig::list_context_names(Some(&path)).unwrap();
        assert_eq!(contexts, vec!["kind-dev".to_string()]);
    }

    #[test]
    fn missing_kubeconfig_is_reported_as_an_error() {
        let missing = std::env::temp_dir().join("fernrohr-picker-fixture-does-not-exist.yaml");
        let result = kubeconfig::list_context_names(Some(&missing));
        assert!(result.is_err());
    }

    /// Guards the `include_bytes!` in [`super::logo`]: a truncated or placeholder
    /// asset compiles fine and only fails as a blank gap in the UI. WebP's RIFF
    /// header carries the total file size, so a length that disagrees with it
    /// catches truncation exactly rather than by proxy.
    #[test]
    fn the_embedded_logo_is_a_complete_webp() {
        let bytes = include_bytes!("../assets/fernrohr-logo.webp");
        assert!(bytes.starts_with(b"RIFF"), "missing RIFF signature");
        assert_eq!(&bytes[8..12], b"WEBP", "RIFF payload is not WebP");

        let declared = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(
            declared + 8,
            bytes.len(),
            "RIFF size disagrees with file length - asset looks truncated"
        );

        // The lossless full-resolution encode is ~1.14MB. The floor catches a
        // placeholder; the ceiling catches either the 1.6MB brand source in
        // `images/` or a lossy re-encode being committed here by mistake - this
        // asset is required to be bit-identical to the master, and both of those
        // are not.
        assert!(
            (1_000_000..1_400_000).contains(&bytes.len()),
            "logo is {} bytes, outside the expected lossless full-resolution range",
            bytes.len()
        );
    }

    /// The window minimum exists so the picker's centred column never overflows and
    /// clips the logo. These pin the composition it is derived from, and catch the
    /// one mistake derivation cannot catch on its own: a minimum *larger* than the
    /// default window, which would open every fresh window already violating its own
    /// floor.
    #[test]
    fn the_window_minimum_fits_the_picker() {
        let min_width = f32::from(MIN_WINDOW_SIZE.width);
        let min_height = f32::from(MIN_WINDOW_SIZE.height);

        assert!(
            min_width >= CARD_WIDTH,
            "minimum width {min_width} is narrower than the {CARD_WIDTH}px card"
        );

        let required = LOGO_HEIGHT + PICKER_CONTENT_GAP + CARD_CHROME_HEIGHT + CARD_LIST_MAX_HEIGHT;
        assert!(
            min_height >= required,
            "minimum height {min_height} cannot fit the picker, which needs {required}"
        );

        let default_layout = crate::config::workspace::WindowLayout::default();
        assert!(
            min_width <= default_layout.width && min_height <= default_layout.height,
            "minimum {}x{} exceeds the default window {}x{} - a fresh window would open \
             already violating its own minimum",
            min_width,
            min_height,
            default_layout.width,
            default_layout.height
        );
    }

    /// The asset is re-encoded at full source resolution precisely so that rendering it
    /// at `LOGO_WIDTH` stays sharp on a Retina display, and `LOGO_HEIGHT` is typed to its
    /// aspect so the mark is not stretched. Neither link is enforced at runtime, so read
    /// the real canvas out of the shipped bytes and hold both.
    #[test]
    fn the_embedded_logo_is_large_enough_for_the_size_it_renders_at() {
        /// Canvas dimensions of `app/assets/fernrohr-logo.webp`. A lossless WebP is a
        /// single `VP8L` chunk - unlike the lossy-with-alpha `VP8X` extended format -
        /// and packs the canvas into a 32-bit field: 12 bytes of RIFF header, then the
        /// chunk id and its length, then a 0x2f signature byte, then width-1 in bits
        /// 0-13 and height-1 in bits 14-27.
        fn canvas_size() -> (usize, usize) {
            let bytes = include_bytes!("../assets/fernrohr-logo.webp");
            assert_eq!(&bytes[12..16], b"VP8L", "expected a lossless (VP8L) WebP");
            assert_eq!(bytes[20], 0x2f, "missing the VP8L signature byte");
            let packed = u32::from_le_bytes(bytes[21..25].try_into().unwrap());
            (
                (packed & 0x3fff) as usize + 1,
                ((packed >> 14) & 0x3fff) as usize + 1,
            )
        }

        let (width, height) = canvas_size();
        assert!(
            width >= (2. * LOGO_WIDTH) as usize,
            "asset is {width}px wide but renders at {LOGO_WIDTH}px - soft on Retina"
        );

        let asset_aspect = width as f32 / height as f32;
        let rendered_aspect = LOGO_WIDTH / LOGO_HEIGHT;
        assert!(
            (asset_aspect - rendered_aspect).abs() < 0.01,
            "asset aspect {asset_aspect} does not match the rendered {rendered_aspect} - \
             the mark would be stretched"
        );
    }

    /// Section 3.2: drives `ClusterPicker` into a fake `Failed` attempt directly (via
    /// `ClusterConnection::test_with_state`, no real connect) rather than through
    /// `select`, so the failure path doesn't depend on network access or a real
    /// kubeconfig - then confirms the picker remains interactive by driving a second
    /// `select` call afterward.
    ///
    /// The retry also goes through the stub connection factory rather than
    /// `ClusterRegistry::connection`: a real connect spawns tokio work on a runtime
    /// worker thread, which gpui's test scheduler flags as nondeterminism and turns
    /// into a flaky failure. This test previously failed that way on `develop`.
    #[gpui_kit::test]
    async fn failed_attempt_shows_the_reason_and_stays_interactive(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use super::{Attempt, ClusterPicker};
        use crate::cluster::connection::{ClusterConnection, ConnectionState};
        use gpui_kit::{AppContext as _, Entity};

        /// Stands in for `ClusterRegistry::connection`: hands back a connection in a
        /// non-terminal state so selecting again replaces the attempt without any
        /// real I/O.
        fn stub_connection(
            cx: &mut gpui_kit::App,
            _context_name: &str,
        ) -> Entity<ClusterConnection> {
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting))
        }

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = cx.add_window(ClusterPicker::new);

        window
            .update(cx, |picker, _window, cx| {
                picker.connection_factory = Some(stub_connection);
                picker.attempt = Some(Attempt {
                    context_name: "kind-dev".to_string(),
                    connection: cx.new(|_| {
                        ClusterConnection::test_with_state(ConnectionState::Failed(
                            "connection refused".to_string(),
                        ))
                    }),
                });
                cx.notify();
            })
            .unwrap();

        window
            .update(cx, |picker, _window, cx| {
                let attempt = picker.attempt.as_ref().expect("attempt is still set");
                assert_eq!(attempt.context_name, "kind-dev");
                assert!(matches!(
                    attempt.connection.read(cx).state,
                    ConnectionState::Failed(ref reason) if reason == "connection refused"
                ));
            })
            .unwrap();

        // Stays interactive: a failed attempt doesn't leave the picker stuck - selecting
        // again (retry, or a different context) starts a fresh attempt.
        window
            .update(cx, |picker, _window, cx| {
                picker.select("kind-dev".to_string(), cx)
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |picker, _window, _cx| {
                assert_eq!(
                    picker
                        .attempt
                        .as_ref()
                        .expect("select started a new attempt")
                        .context_name,
                    "kind-dev"
                );
            })
            .unwrap();
    }
}
