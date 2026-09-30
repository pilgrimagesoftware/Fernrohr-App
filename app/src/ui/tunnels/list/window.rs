//! Owns the Tunnels window's singleton lifecycle (open-or-focus, design.md decision
//! 5's single-instance rule) and `TunnelsWindow`'s state: construction, re-reading
//! `tunnels.toml`/the kubeconfig, the running-state watch, and opening/closing the
//! embedded editor pane. `render.rs` owns turning that state into a layout.

use super::*;

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
                // An explicit, centered starting size: with only a minimum, the
                // platform default opened this small list-and-editor window huge.
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    TUNNELS_WINDOW_SIZE,
                    cx,
                ))),
                window_min_size: Some(TUNNELS_WINDOW_MIN_SIZE),
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
    pub(super) tunnels: Vec<(String, TunnelConfig)>,
    pub(super) usage: BTreeMap<String, usize>,
    pub(super) stale: Vec<(String, String)>,
    pub(super) running: BTreeSet<ForwardKey>,
    pub(super) editor: Option<Entity<TunnelEditor>>,
    pub(super) focus_handle: FocusHandle,
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
        cx.observe_global::<TunnelsRevision>(|this, cx| this.refresh(cx))
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

    pub(super) fn is_running(&self, tunnel_id: &str) -> bool {
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

    pub(super) fn open_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tunnels_path = self.tunnels_path.clone();
        let editor = cx.new(|cx| TunnelEditor::create(tunnels_path, window, cx));
        self.watch_editor(&editor, cx);
        self.editor = Some(editor);
        cx.notify();
    }

    pub(super) fn open_edit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let tunnels_path = self.tunnels_path.clone();
        let editor = cx.new(|cx| TunnelEditor::edit(tunnels_path, id, window, cx));
        self.watch_editor(&editor, cx);
        self.editor = Some(editor);
        cx.notify();
    }

    /// Tasks.md 4.1: a stale binding's Remove - `unbind` the context and refresh, so
    /// the row disappears the moment its binding is gone.
    pub(super) fn remove_stale(&mut self, context_name: String, cx: &mut Context<Self>) {
        let store = TunnelStore::new(self.tunnels_path.clone());
        if store.unbind(&context_name).is_ok() {
            notify_tunnels_changed(cx);
        }
        self.refresh(cx);
    }
}

impl Focusable for TunnelsWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests;
