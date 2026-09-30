//! The cluster picker: shown in place of a window's panel workspace whenever that window
//! has no open or restored panels. Lists kubeconfig contexts through the same searchable
//! `Command` widget the app-wide command palette uses, drives a connection through
//! `ClusterRegistry`, and emits [`PickerEvent::Connected`] on success so `shell::MainWindow`
//! can switch that window into its normal panel workspace.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::kubeconfig;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::tunnel::store::TunnelStore;
use crate::ui::picker_tunnel::{self, TunnelChoice};
use crate::ui::tunnels::TunnelsRevision;
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

pub enum PickerEvent {
    Connected { context_name: String },
}

/// One read of `tunnels.toml`'s choices and bindings, for [`ClusterPicker::new`] and
/// every [`ClusterPicker::set_tunnel`] afterward - kept as one function so the two
/// call sites can't drift into reading it two different ways.
///
/// `pub(crate)`: `ui/context_bar.rs`'s chips read the same cache for their tunnel
/// name, refreshed on the same [`TunnelsRevision`] signal - one read of the file's
/// shape, not two.
pub(crate) fn load_tunnels(
    tunnels_path: &std::path::Path,
) -> (Vec<TunnelChoice>, BTreeMap<String, String>) {
    let store = TunnelStore::new(tunnels_path.to_path_buf());
    let choices = picker_tunnel::tunnel_choices(&store);
    let bindings = store.bindings().into_iter().collect();
    (choices, bindings)
}

/// One attempt in flight: which context was picked and its connection entity, so the
/// picker can render `Connecting`/`WaitingForTunnel`/`Failed` and let the user retry.
struct Attempt {
    context_name: String,
    connection: Entity<ClusterConnection>,
    connected: bool,
}

pub struct ClusterPicker {
    contexts: Result<Vec<String>, String>,
    command_state: Entity<CommandState>,
    attempt: Option<Attempt>,
    focus_handle: FocusHandle,
    /// Where `tunnels.toml` lives - the real preference-dir path in production, a
    /// scratch file in tests (see section 3.1's tests below).
    tunnels_path: PathBuf,
    /// Every configured tunnel, for each row's selector dropdown. Loaded once at
    /// construction and refreshed only right after this picker's own bind/unbind
    /// (see [`Self::set_tunnel`]) rather than on every render - `render` is a hot
    /// path and must not read `tunnels.toml` on every frame.
    tunnel_choices: Vec<TunnelChoice>,
    /// Context name -> bound tunnel id, for each row's selector label and current
    /// choice. Same caching rule as `tunnel_choices`.
    tunnel_bindings: BTreeMap<String, String>,
    /// Reloads the two caches above whenever any view writes `tunnels.toml`.
    _tunnels_observation: Subscription,
    /// Test-only stand-in for `ClusterRegistry::connection`. Real connections spawn
    /// tokio work on a runtime worker thread, which gpui's test scheduler rejects
    /// as cross-thread nondeterminism - so tests substitute a stub instead of
    /// driving a real connect.
    #[cfg(test)]
    pub(crate) connection_factory: Option<fn(&mut App, &str) -> Entity<ClusterConnection>>,
}

impl ClusterPicker {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tunnels_path = crate::util::paths::preference_dir().join("tunnels.toml");
        let (tunnel_choices, tunnel_bindings) = load_tunnels(&tunnels_path);
        let tunnels_observation = cx.observe_global::<TunnelsRevision>(|this, cx| {
            let (choices, bindings) = load_tunnels(&this.tunnels_path);
            this.tunnel_choices = choices;
            this.tunnel_bindings = bindings;
            cx.notify();
        });
        Self {
            contexts: kubeconfig::list_context_names(None).map_err(|error| error.to_string()),
            command_state: cx.new(|cx| CommandState::new(window, cx)),
            attempt: None,
            focus_handle: cx.focus_handle(),
            tunnels_path,
            tunnel_choices,
            tunnel_bindings,
            _tunnels_observation: tunnels_observation,
            #[cfg(test)]
            connection_factory: None,
        }
    }

    /// Section 3.1: binds (or, for `None`, unbinds) `context_name` and refreshes the
    /// cached choices/bindings the next render reads - the write itself is the only
    /// I/O; `render` never touches `tunnels.toml`.
    fn set_tunnel(
        &mut self,
        context_name: &str,
        tunnel_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let store = TunnelStore::new(self.tunnels_path.clone());
        let result = match &tunnel_id {
            Some(id) => store.bind(context_name, id),
            None => store.unbind(context_name),
        };
        if let Err(error) = result {
            log::warn!("failed to update {context_name}'s tunnel binding: {error:?}");
            return;
        }
        // The `TunnelsRevision` observer reloads this picker's caches along with every
        // other open picker's.
        crate::ui::tunnels::notify_tunnels_changed(cx);
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

    pub(crate) fn select(&mut self, context_name: String, cx: &mut Context<Self>) {
        let connection = self.new_connection(&context_name, cx);
        cx.observe(&connection, {
            move |this: &mut Self, _connection, cx| {
                this.emit_connected(cx);
                cx.notify();
            }
        })
        .detach();
        self.attempt = Some(Attempt {
            context_name,
            connection,
            connected: false,
        });
        self.emit_connected(cx);
        cx.notify();
    }

    fn emit_connected(&mut self, cx: &mut Context<Self>) {
        let Some(attempt) = &mut self.attempt else {
            return;
        };
        if attempt.connected
            || !matches!(
                &attempt.connection.read(cx).state,
                ConnectionState::Connected(_)
            )
        {
            return;
        }
        attempt.connected = true;
        cx.emit(PickerEvent::Connected {
            context_name: attempt.context_name.clone(),
        });
    }

    pub fn command_focus_handle(&self, cx: &App) -> FocusHandle {
        self.command_state.read(cx).focus_handle(cx)
    }

    /// `window-context-bar` section 3.2's "+" popover: the same picker, minus the
    /// contexts `used` already names. Filtering post-construction, rather than a
    /// second contexts source, is what keeps every other rule - tunnel bindings,
    /// search, the connect flow - identical to the picker window's own.
    pub(crate) fn exclude(&mut self, used: &[String]) {
        if let Ok(contexts) = &mut self.contexts {
            contexts.retain(|name| !used.contains(name));
        }
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
            include_bytes!("../../assets/fernrohr-logo.webp").to_vec(),
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

/// Section 3.1: one context row's content - icon, name, and its tunnel selector at
/// the trailing end. Built fresh on every `Command` render (a `Fn`, not `FnMut`, per
/// `CommandItem::child`'s contract), so everything it needs is captured by value here
/// rather than borrowed from `ClusterPicker`.
fn context_row(
    context_name: String,
    bound_id: Option<String>,
    choices: Vec<TunnelChoice>,
    picker: WeakEntity<ClusterPicker>,
) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_window, cx| {
        let theme = cx.theme();
        let row_id = format!("picker-tunnel-{context_name}");
        let picker_for_pick = picker.clone();
        let context_for_pick = context_name.clone();
        let selector = picker_tunnel::selector(
            row_id,
            &choices,
            bound_id.as_deref(),
            move |tunnel_id, cx| {
                let _ = picker_for_pick.update(cx, |this, cx| {
                    this.set_tunnel(&context_for_pick, tunnel_id, cx)
                });
            },
        );
        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::Server)
                            .size(px(16.))
                            .text_color(theme.muted_foreground),
                    )
                    .child(context_name.clone()),
            )
            .child(selector)
            .into_any_element()
    }
}

/// Section 4.1/design.md decision 5: the picker's own way to reach the Tunnels
/// window, alongside the app menu and the `tunnels.manage` command.
fn manage_tunnels_control() -> impl IntoElement {
    Button::new("picker-manage-tunnels")
        .label("Manage tunnels…")
        .icon(IconName::Settings)
        .xsmall()
        .ghost()
        .tab_stop(false)
        .on_click(|_event, _window, cx| crate::ui::tunnels::open_or_focus(cx))
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
        let tunnel_choices = self.tunnel_choices.clone();
        let tunnel_bindings = self.tunnel_bindings.clone();
        let items: Vec<CommandItem> = contexts
            .iter()
            .map(|name| {
                let bound_id = tunnel_bindings.get(name).cloned();
                CommandItem::new().label(name.clone()).child(context_row(
                    name.clone(),
                    bound_id,
                    tunnel_choices.clone(),
                    this.clone(),
                ))
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
                .child(div().flex().justify_end().child(manage_tunnels_control()))
                .track_focus(&self.focus_handle)
                .into_any_element(),
        )
        .into_any_element()
    }
}

// A sibling `tests.rs` rather than an inline module: this file is at the 700-line
// cap (rust-structure.md), and `use super::*` there would re-import `gpui_kit`'s own
// `test` attribute macro (this file's `use gpui_kit::*` brings it in), shadowing the
// builtin `#[test]` and sending a plain sync test into `#[gpui_kit::test]`'s
// async-runtime expansion instead - hence that file's explicit imports rather than a
// glob.
#[cfg(test)]
mod tests;
