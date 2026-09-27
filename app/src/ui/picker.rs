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

    pub fn command_focus_handle(&self, cx: &App) -> FocusHandle {
        self.command_state.read(cx).focus_handle(cx)
    }
}

impl EventEmitter<PickerEvent> for ClusterPicker {}

impl Focusable for ClusterPicker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// A centered card matching the app's command-palette chrome (`popover`
/// surface, `border` outline, `shadow_lg`), so the picker reads as part of
/// the same design system rather than a bespoke first-run screen.
fn card(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .w(px(480.))
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

/// The Fernrohr logo, centered above the picker card.
///
/// Embedded at compile time rather than resolved from disk at runtime:
/// `images/` holds the untouched brand sources (1254px, multi-megabyte), while
/// `app/assets/` holds the trimmed, screen-sized render that ships in the
/// binary. Embedding means the picker is unaffected by the working directory
/// or by how the app is packaged.
fn logo() -> impl IntoElement {
    // Built once and reused: `Image`'s `Hash` impl hashes its own bytes, which
    // is what gpui's asset cache keys on, so only the first render decodes it.
    static LOGO: LazyLock<Arc<Image>> = LazyLock::new(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../../assets/fernrohr-logo.png").to_vec(),
        ))
    });

    img(LOGO.clone())
        .w(px(96.))
        .h(px(93.)) // matches the 192x186 source aspect so the mark isn't stretched
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
    /// asset compiles fine and only fails as a blank gap in the UI, so assert the
    /// embedded bytes are a complete PNG rather than just non-empty.
    #[test]
    fn the_embedded_logo_is_a_complete_png() {
        const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
        let bytes = include_bytes!("../../assets/fernrohr-logo.png");
        assert!(bytes.starts_with(SIGNATURE), "missing PNG signature");
        assert!(
            bytes.ends_with(b"IEND\xae\x42\x60\x82"),
            "missing PNG IEND trailer"
        );
        assert!(bytes.len() > 1024, "logo looks truncated");
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
