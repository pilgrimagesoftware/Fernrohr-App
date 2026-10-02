//! `ClusterPicker`'s construction and persisted state: the struct itself, building one
//! from a fresh kubeconfig read and `tunnels.toml` snapshot ([`ClusterPicker::new`]),
//! writing a tunnel binding back ([`ClusterPicker::set_tunnel`]), starting a connection
//! attempt and reporting it ([`ClusterPicker::select`], [`ClusterPicker::emit_connected`]),
//! and the `window-context-bar` popover's context filtering ([`ClusterPicker::exclude`]).
//! Click/keyboard selection lives in [`super::interaction`]; the picker's own `Render`
//! wiring lives in [`super::render`].

use super::*;

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
pub(super) struct Attempt {
    pub(super) context_name: String,
    pub(super) connection: Entity<ClusterConnection>,
    connected: bool,
}

pub struct ClusterPicker {
    pub(super) contexts: Result<Vec<String>, String>,
    pub(super) command_state: Entity<CommandState>,
    pub(super) attempt: Option<Attempt>,
    /// The context the highlight currently points at - what [`Self::connect_button`]
    /// targets, set by [`Self::handle_row_click`] or `Command`'s own hover/keyboard
    /// highlight (via the `on_select` wired in `render`). Kept separately from
    /// `command_state`'s own highlighted index because that index is only correct
    /// after `Command` has rendered at least once, which happens *after* this
    /// struct's own `render` body runs; initializing this to the first context up
    /// front (see [`Self::new`]) gives the Connect button a sensible target from
    /// the very first frame.
    pub(super) selected_context: Option<String>,
    pub(super) focus_handle: FocusHandle,
    /// Where `tunnels.toml` lives - the real preference-dir path in production, a
    /// scratch file in tests (see section 3.1's tests below).
    tunnels_path: PathBuf,
    /// Every configured tunnel, for each row's selector dropdown. Loaded once at
    /// construction and refreshed only right after this picker's own bind/unbind
    /// (see [`Self::set_tunnel`]) rather than on every render - `render` is a hot
    /// path and must not read `tunnels.toml` on every frame.
    pub(super) tunnel_choices: Vec<TunnelChoice>,
    /// Context name -> bound tunnel id, for each row's selector label and current
    /// choice. Same caching rule as `tunnel_choices`.
    pub(super) tunnel_bindings: BTreeMap<String, String>,
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
        let contexts = kubeconfig::list_context_names(None).map_err(|error| error.to_string());
        // Nothing is selected until the user clicks a row: the Connect button stays
        // disabled rather than pointing at a context nobody chose.
        let selected_context = None;
        Self {
            contexts,
            command_state: cx.new(|cx| CommandState::new(window, cx)),
            attempt: None,
            selected_context,
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
    pub(super) fn set_tunnel(
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

    /// Whether an attempt is actively connecting - [`Self::connect_button`]'s other
    /// disabling condition, alongside no selection, so a second click cannot start a
    /// redundant connect while one is already under way. A `Failed` or already
    /// `Connected` attempt does not count: retrying, or connecting a different
    /// context afterward, is exactly what should stay available.
    pub(super) fn is_connect_in_flight(&self, cx: &App) -> bool {
        self.attempt.as_ref().is_some_and(|attempt| {
            matches!(
                attempt.connection.read(cx).state,
                ConnectionState::Connecting | ConnectionState::WaitingForTunnel
            )
        })
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

    /// Test-only: replaces what the kubeconfig gave, so a test outside this
    /// module can show any of the picker's states without a real kubeconfig.
    #[cfg(test)]
    pub(crate) fn test_set_contexts(&mut self, contexts: Result<Vec<String>, String>) {
        self.contexts = contexts;
    }

    /// Where keyboard focus belongs when the picker is shown: its context list,
    /// or - with no contexts to list (none configured, or the kubeconfig
    /// unreadable) - the picker card, the only focusable element those states
    /// draw. Focusing the list there would focus an element that isn't drawn,
    /// leaving no focus path, so the window's own actions (the command palette)
    /// would be unreachable from the keyboard.
    pub fn initial_focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.contexts {
            Ok(contexts) if !contexts.is_empty() => self.command_focus_handle(cx),
            _ => self.focus_handle.clone(),
        }
    }

    /// `window-context-bar` section 3.2's "+" popover: the same picker, minus the
    /// contexts `used` already names. Filtering post-construction, rather than a
    /// second contexts source, is what keeps every other rule - tunnel bindings,
    /// search, the connect flow - identical to the picker window's own.
    pub(crate) fn exclude(&mut self, used: &[String]) {
        let Ok(contexts) = &mut self.contexts else {
            return;
        };
        contexts.retain(|name| !used.contains(name));
        // A selected context that was just filtered out (another window uses it)
        // is no longer selectable here: clear it rather than pick one for the user.
        if !self
            .selected_context
            .as_ref()
            .is_some_and(|name| contexts.contains(name))
        {
            self.selected_context = None;
        }
    }
}

impl EventEmitter<PickerEvent> for ClusterPicker {}

impl Focusable for ClusterPicker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests;
