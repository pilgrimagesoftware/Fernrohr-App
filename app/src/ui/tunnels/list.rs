//! Section 4.1 of the `tunnel-management-ui` change: the Tunnels window shell - a
//! single-instance OS window listing every tunnel (name, host, read-only usage count,
//! running state from section 2.2's watched `ForwardKey` set) and a stale-bindings
//! section with a per-row Remove. A "New Tunnel" control and each row's Edit open
//! `editor::TunnelEditor` in the pane below; the window itself never assigns a
//! context to a tunnel (proposal.md, design.md decision 5) - that lives on the
//! context side (`ui/picker_tunnel.rs`, `util/shell.rs`'s `context.set_tunnel`).

use super::editor::{TunnelEditor, TunnelEditorEvent};
use crate::command::{Command, CommandRegistry};
use crate::config::tunnels::TunnelConfig;
use crate::k8s::cluster::kubeconfig;
use crate::k8s::cluster::tunnel::{self, ForwardKey};
use crate::tunnel::store::TunnelStore;
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Root, Sizable as _};
use gpui_kit::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

actions!(tunnels, [TunnelsManage]);

pub const TUNNELS_MANAGE_COMMAND_ID: &str = "tunnels.manage";
pub const TUNNELS_MANAGE_DEFAULT_BINDING: &str = "cmd-shift-t";

/// The command this module contributes to the app-wide [`CommandRegistry`] - see
/// `util/shell.rs::register_commands` for the sibling pattern this follows.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: TUNNELS_MANAGE_COMMAND_ID,
        title: "Manage Tunnels…",
        default_binding: TUNNELS_MANAGE_DEFAULT_BINDING,
        context: None,
        action: Box::new(TunnelsManage),
        // The Context menu: tunnels are how a context is reached, and the menu bar
        // makes the Tunnels window reachable with no cluster window open.
        menu: Some(crate::command::MenuSlot::Context),
    });
}

/// The one Tunnels window currently open, if any - `None` both before the first open
/// and after the last one closes.
struct TunnelsWindowHandle(Option<WindowHandle<Root>>);

impl Global for TunnelsWindowHandle {}

/// Opens the Tunnels window, or brings an already-open one to the front -
/// design.md decision 5's single-instance rule. Reached from `tunnels.manage`
/// (bound above), the menu bar's Context menu (`ui/menu.rs`), and the picker's "Manage tunnels…"
/// control (`ui/picker.rs`).
pub fn open_or_focus(cx: &mut App) {
    if !cx.has_global::<TunnelsWindowHandle>() {
        cx.set_global(TunnelsWindowHandle(None));
    }
    if let Some(handle) = cx.global::<TunnelsWindowHandle>().0
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }

    let handle = cx
        .open_window(
            WindowOptions {
                window_min_size: Some(size(px(640.), px(520.))),
                ..Default::default()
            },
            |window, cx| {
                let tunnels_path = crate::util::paths::preference_dir().join("tunnels.toml");
                let view = cx.new(|cx| TunnelsWindow::new(tunnels_path, None, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("failed to open the tunnels window");

    let _ = handle.update(cx, |_, window, cx| {
        window.on_window_should_close(cx, |_, cx| {
            if cx.has_global::<TunnelsWindowHandle>() {
                cx.global_mut::<TunnelsWindowHandle>().0 = None;
            }
            true
        });
    });
    cx.set_global(TunnelsWindowHandle(Some(handle)));
}

pub struct TunnelsWindow {
    tunnels_path: PathBuf,
    /// `None` resolves to `$KUBECONFIG`/`~/.kube/config`, exactly like
    /// `kubeconfig::list_context_names`'s own default - overridden in tests with a
    /// fixture so the stale-bindings check doesn't depend on this machine's real
    /// kubeconfig.
    kubeconfig_path: Option<PathBuf>,
    tunnels: Vec<(String, TunnelConfig)>,
    usage: BTreeMap<String, usize>,
    stale: Vec<(String, String)>,
    running: BTreeSet<ForwardKey>,
    editor: Option<Entity<TunnelEditor>>,
    focus_handle: FocusHandle,
}

impl TunnelsWindow {
    /// `kubeconfig_path` is `None` in production (resolving to `$KUBECONFIG`/
    /// `~/.kube/config`, the same default every other kubeconfig read uses) and
    /// `Some(fixture)` in tests, so the stale-bindings check never depends on this
    /// machine's real kubeconfig.
    fn new(
        tunnels_path: PathBuf,
        kubeconfig_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            tunnels_path,
            kubeconfig_path,
            tunnels: Vec::new(),
            usage: BTreeMap::new(),
            stale: Vec::new(),
            running: BTreeSet::new(),
            editor: None,
            focus_handle: cx.focus_handle(),
        };
        this.refresh(cx);
        this.watch_running_state(cx);
        // A binding changed from a picker or `context.set_tunnel` moves this window's
        // usage counts, so re-read on every write, not only this window's own.
        cx.observe_global::<super::TunnelsRevision>(|this, cx| this.refresh(cx))
            .detach();
        let _ = window;
        this
    }

    /// Re-reads `tunnels.toml` and the kubeconfig's context list - the only I/O this
    /// view performs, and only right after construction or a write (create/edit/
    /// delete/remove-stale), never from `render` itself.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let store = TunnelStore::new(self.tunnels_path.clone());
        let mut tunnels = store.list();
        tunnels.sort_by(|a, b| a.1.name.cmp(&b.1.name).then_with(|| a.0.cmp(&b.0)));
        self.tunnels = tunnels;
        self.usage = store.usage_counts();
        // An unreadable kubeconfig means the context list is unknown, not empty: treating
        // it as empty would flag every binding stale and invite removing good ones.
        self.stale = match kubeconfig::list_context_names(self.kubeconfig_path.as_deref()) {
            Ok(contexts) => store.stale_bindings(&contexts),
            Err(_) => Vec::new(),
        };
        cx.notify();
    }

    /// Section 2.2/2.3's watched live-key set, relayed onto this view so each row's
    /// running/idle label follows a real acquire/release without polling - see
    /// `k8s::cluster::tunnel::drive_live_keys`.
    fn watch_running_state(&mut self, cx: &mut Context<Self>) {
        let live_rx = tunnel::live_forward_keys(cx);
        let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
            tunnel::drive_live_keys(live_rx, tx).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, move |keys| {
                let _ = this.update(cx, |this, cx| {
                    this.running = keys;
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }

    fn is_running(&self, tunnel_id: &str) -> bool {
        self.running.iter().any(|key| key.tunnel_id == tunnel_id)
    }

    fn watch_editor(&mut self, editor: &Entity<TunnelEditor>, cx: &mut Context<Self>) {
        cx.subscribe(editor, |this, _editor, event, cx| match event {
            TunnelEditorEvent::Saved | TunnelEditorEvent::Deleted => {
                this.editor = None;
                this.refresh(cx);
            }
            TunnelEditorEvent::Cancelled => {
                this.editor = None;
                cx.notify();
            }
        })
        .detach();
    }

    fn open_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tunnels_path = self.tunnels_path.clone();
        let editor = cx.new(|cx| TunnelEditor::create(tunnels_path, window, cx));
        self.watch_editor(&editor, cx);
        self.editor = Some(editor);
        cx.notify();
    }

    fn open_edit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let tunnels_path = self.tunnels_path.clone();
        let editor = cx.new(|cx| TunnelEditor::edit(tunnels_path, id, window, cx));
        self.watch_editor(&editor, cx);
        self.editor = Some(editor);
        cx.notify();
    }

    /// Tasks.md 4.1: a stale binding's Remove - `unbind` the context and refresh, so
    /// the row disappears the moment its binding is gone.
    fn remove_stale(&mut self, context_name: String, cx: &mut Context<Self>) {
        let store = TunnelStore::new(self.tunnels_path.clone());
        if store.unbind(&context_name).is_ok() {
            super::notify_tunnels_changed(cx);
        }
        self.refresh(cx);
    }
}

impl Focusable for TunnelsWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn usage_label(count: usize) -> String {
    match count {
        0 => "Unused".to_string(),
        1 => "In use by 1 context".to_string(),
        n => format!("In use by {n} contexts"),
    }
}

impl Render for TunnelsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        let weak_new = cx.weak_entity();
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(div().text_lg().font_semibold().child("Tunnels"))
            .child(
                Button::new("tunnels-new")
                    .label("New Tunnel")
                    .icon(IconName::Plus)
                    .primary()
                    .small()
                    .on_click(move |_event, window, cx| {
                        let _ = weak_new.update(cx, |this, cx| this.open_create(window, cx));
                    }),
            );

        let rows =
            self.tunnels.iter().map(|(id, tunnel)| {
                let usage = self.usage.get(id).copied().unwrap_or(0);
                let running = self.is_running(id);
                let weak_edit = cx.weak_entity();
                let id_for_edit = id.clone();
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .py_1()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().font_medium().child(tunnel.name.clone()))
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                format!(
                                    "{}@{}:{}",
                                    tunnel.bastion_user, tunnel.bastion_host, tunnel.bastion_port
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(usage_label(usage)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if running {
                                        theme.success
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .child(if running { "Running" } else { "Idle" }),
                            )
                            .child(
                                Button::new(format!("tunnels-edit-{id}"))
                                    .label("Edit")
                                    .outline()
                                    .xsmall()
                                    .on_click(move |_event, window, cx| {
                                        let _ = weak_edit.update(cx, |this, cx| {
                                            this.open_edit(id_for_edit.clone(), window, cx)
                                        });
                                    }),
                            ),
                    )
            });

        let stale_section = (!self.stale.is_empty()).then(|| {
            let rows = self.stale.iter().map(|(context, tunnel_id)| {
                let weak_remove = cx.weak_entity();
                let context_for_remove = context.clone();
                let tunnel_name = self
                    .tunnels
                    .iter()
                    .find(|(id, _)| id == tunnel_id)
                    .map(|(_, tunnel)| tunnel.name.clone())
                    .unwrap_or_else(|| tunnel_id.clone());
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .child(format!("{context} \u{2192} {tunnel_name}")),
                    )
                    .child(
                        Button::new(format!("tunnels-stale-remove-{context}"))
                            .label("Remove")
                            .outline()
                            .xsmall()
                            .on_click(move |_event, _window, cx| {
                                let _ = weak_remove.update(cx, |this, cx| {
                                    this.remove_stale(context_for_remove.clone(), cx)
                                });
                            }),
                    )
            });
            div()
                .flex()
                .flex_col()
                .gap_2()
                .pt_2()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(theme.danger)
                        .child("Stale bindings"),
                )
                .children(rows)
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .bg(theme.background)
            .track_focus(&self.focus_handle)
            .child(header)
            .child(div().flex().flex_col().gap_1().children(rows))
            .children(stale_section)
            .children(self.editor.clone().map(|editor| {
                div()
                    .border_t_1()
                    .border_color(theme.border)
                    .pt_3()
                    .child(editor)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::{TUNNELS_MANAGE_COMMAND_ID, TunnelsWindow, register_commands};
    use crate::command::CommandRegistry;
    use crate::config::tunnels::{TunnelAuth, TunnelConfig};
    use crate::k8s::cluster::tunnel::ForwardKey;
    use crate::tunnel::store::TunnelStore;
    use gpui_kit::TestAppContext;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// The `tunnels.manage` command is registered with a title, alongside every
    /// other palette command - see `ui/nav.rs`'s sibling test.
    #[test]
    fn tunnels_manage_is_a_registered_command() {
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);

        let command = registry
            .get(TUNNELS_MANAGE_COMMAND_ID)
            .expect("tunnels.manage must be registered");
        assert_eq!(command.title, "Manage Tunnels…");
    }

    fn temp_tunnels_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fernrohr-tunnels-window-test-{n}.toml"));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn missing_kubeconfig_path() -> PathBuf {
        std::env::temp_dir().join("fernrohr-tunnels-window-test-no-such-kubeconfig.yaml")
    }

    fn sample_tunnel(name: &str) -> TunnelConfig {
        TunnelConfig {
            name: name.to_string(),
            bastion_user: "ops".into(),
            bastion_host: "bastion.example.com".into(),
            bastion_port: 22,
            jump_hosts: Vec::new(),
            auth: TunnelAuth::default(),
        }
    }

    /// Tasks.md 4.1: opening the Tunnels window twice must not create a second
    /// window - the second call focuses the one already open. Exercises
    /// `TunnelsWindow` construction and `is_running` directly (the single-instance
    /// `open_or_focus`/`WindowHandle` bookkeeping is exercised structurally by its own
    /// short-circuit on `activate_window`, which needs a real platform window this
    /// harness does not create) - see `a_running_state_follows_acquire_and_release`
    /// below for the behavior `open_or_focus` exists to show.
    #[gpui_kit::test]
    async fn a_second_construction_over_the_same_files_reads_the_same_state(
        cx: &mut TestAppContext,
    ) {
        // `TunnelsWindow::new` always starts `watch_running_state`'s real tokio task
        // (via `spawn_stream`), even with no forward ever acquired - the same seam
        // `cluster::session`'s and `cluster::connection`'s own tests allow-park for.
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let tunnels_path = temp_tunnels_path();
        let store = TunnelStore::new(tunnels_path.clone());
        store
            .create("qa-bastion", sample_tunnel("QA"), None)
            .unwrap();

        let kubeconfig_path = missing_kubeconfig_path();
        let window = cx.add_window({
            let tunnels_path = tunnels_path.clone();
            let kubeconfig_path = kubeconfig_path.clone();
            move |window, cx| TunnelsWindow::new(tunnels_path, Some(kubeconfig_path), window, cx)
        });
        let second = cx.add_window({
            let tunnels_path = tunnels_path.clone();
            let kubeconfig_path = kubeconfig_path.clone();
            move |window, cx| TunnelsWindow::new(tunnels_path, Some(kubeconfig_path), window, cx)
        });

        window
            .update(cx, |this, _window, _cx| {
                assert_eq!(this.tunnels.len(), 1);
                assert_eq!(this.tunnels[0].0, "qa-bastion");
            })
            .unwrap();
        second
            .update(cx, |this, _window, _cx| {
                assert_eq!(this.tunnels.len(), 1);
            })
            .unwrap();

        let _ = std::fs::remove_file(&tunnels_path);
    }

    /// Tasks.md 4.1: running state follows a fake forward's acquire and release -
    /// simulated the same way `k8s::cluster::tunnel`'s own `drive_live_keys` test
    /// simulates one: a hand-driven `BTreeSet<ForwardKey>`, since `TunnelForwards`'s
    /// registry is private and only ever populated through a real `ssh` acquire.
    #[gpui_kit::test]
    async fn a_running_state_follows_acquire_and_release(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let tunnels_path = temp_tunnels_path();
        let store = TunnelStore::new(tunnels_path.clone());
        store
            .create("qa-bastion", sample_tunnel("QA"), None)
            .unwrap();

        let kubeconfig_path = missing_kubeconfig_path();
        let window = cx.add_window({
            let tunnels_path = tunnels_path.clone();
            move |window, cx| TunnelsWindow::new(tunnels_path, Some(kubeconfig_path), window, cx)
        });

        window
            .update(cx, |this, _window, _cx| {
                assert!(!this.is_running("qa-bastion"));
                this.running = std::collections::BTreeSet::from([ForwardKey {
                    tunnel_id: "qa-bastion".to_string(),
                    host: "10.0.0.1".to_string(),
                    port: 6443,
                }]);
                assert!(this.is_running("qa-bastion"));
                this.running.clear();
                assert!(!this.is_running("qa-bastion"));
            })
            .unwrap();

        let _ = std::fs::remove_file(&tunnels_path);
    }

    /// Tasks.md 4.1: Remove deletes a stale binding, and it drops out of the list on
    /// the very next refresh.
    #[gpui_kit::test]
    async fn remove_deletes_a_stale_binding(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let tunnels_path = temp_tunnels_path();
        let store = TunnelStore::new(tunnels_path.clone());
        store
            .create("qa-bastion", sample_tunnel("QA"), None)
            .unwrap();
        store.bind("renamed-away", "qa-bastion").unwrap();

        // An empty (but present) kubeconfig fixture: `renamed-away` is bound but
        // absent from it, so it is genuinely stale rather than "unknown because the
        // kubeconfig couldn't be read".
        let kubeconfig_path = std::env::temp_dir().join(format!(
            "fernrohr-tunnels-window-test-empty-kubeconfig-{}.yaml",
            std::process::id()
        ));
        std::fs::write(
            &kubeconfig_path,
            "apiVersion: v1\nkind: Config\nclusters: []\ncontexts: []\nusers: []\n",
        )
        .unwrap();

        let window = cx.add_window({
            let tunnels_path = tunnels_path.clone();
            let kubeconfig_path = kubeconfig_path.clone();
            move |window, cx| TunnelsWindow::new(tunnels_path, Some(kubeconfig_path), window, cx)
        });

        window
            .update(cx, |this, _window, _cx| {
                assert_eq!(
                    this.stale,
                    vec![("renamed-away".to_string(), "qa-bastion".to_string())]
                );
            })
            .unwrap();

        window
            .update(cx, |this, _window, cx| {
                this.remove_stale("renamed-away".to_string(), cx);
            })
            .unwrap();

        window
            .update(cx, |this, _window, _cx| {
                assert!(this.stale.is_empty());
            })
            .unwrap();
        assert!(store.bindings().is_empty());

        let _ = std::fs::remove_file(&tunnels_path);
        let _ = std::fs::remove_file(&kubeconfig_path);
    }

    /// An unreadable kubeconfig leaves the context list unknown, so no binding is
    /// offered for removal - otherwise every good binding would look stale.
    #[gpui_kit::test]
    async fn an_unreadable_kubeconfig_flags_nothing_stale(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let tunnels_path = temp_tunnels_path();
        let store = TunnelStore::new(tunnels_path.clone());
        store
            .create("qa-bastion", sample_tunnel("QA"), None)
            .unwrap();
        store.bind("greedygoat", "qa-bastion").unwrap();
        let missing = std::env::temp_dir().join(format!(
            "fernrohr-tunnels-window-test-missing-kubeconfig-{}.yaml",
            std::process::id()
        ));

        let window = cx.add_window({
            let tunnels_path = tunnels_path.clone();
            move |window, cx| TunnelsWindow::new(tunnels_path, Some(missing), window, cx)
        });
        window
            .update(cx, |this, _window, _cx| assert!(this.stale.is_empty()))
            .unwrap();

        let _ = std::fs::remove_file(&tunnels_path);
    }
}
