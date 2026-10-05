//! `connection-status-bar`: the workspace window's bottom status bar, one capsule per
//! cluster context the window uses - context name, bound tunnel, state icon and text,
//! and, for any state other than connected, elapsed time - colored by [`Severity`],
//! then the add-context control. `util/shell.rs` renders this under the workspace body
//! in `WindowMode::Workspace`, not in picker mode.
//!
//! `toolbar-layout-with-gpui-kit` folded the old context bar in here, so each context
//! appears once in the chrome: [`capsule`] draws the capsules and owns add and
//! disconnect. Tunnel names are cached and refreshed only on [`TunnelsRevision`], the
//! same rule `ui/picker.rs` follows - `render` never reads `tunnels.toml` itself.

use crate::consts::STATUS_TICK_INTERVAL;
use crate::k8s::cluster::context_health::{ContextHealth, Severity, severity};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::cluster::watch_registry::PauseReason;
use crate::ui::picker::load_tunnels;
use crate::ui::picker_tunnel::TunnelChoice;
use crate::ui::tunnels::TunnelsRevision;
use crate::util::shell::MainWindow;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod capsule;
mod chord;
mod commands;
mod theme_switch;

pub(crate) use commands::{AddContext, DisconnectActiveContext, register_commands};
/// The chord indicator's debug selectors, for tests outside this module.
#[cfg(test)]
pub(crate) mod chord_selectors {
    pub(crate) use super::chord::{
        INDICATOR_SELECTOR, MORE_SELECTOR, POPOVER_SELECTOR, completion_selector,
    };
}

/// The bar's height at the default text size - layout-only, so it stays local rather
/// than in `consts.rs` (see `.claude/rules/rust-structure.md`). It scales with the
/// text, as the spacing tokens do.
const STATUS_BAR_HEIGHT: f32 = 28.;

/// One status bar item's content, computed from [`ClusterRegistry::health`] - a plain
/// struct rather than an `AnyElement`, so tests can assert on it directly instead of
/// inspecting rendered output (design.md decision 2's "testable without rendering",
/// extended from `severity` to the whole item).
#[derive(Debug, Clone, PartialEq)]
pub struct StatusItem {
    pub context_name: String,
    /// The bound tunnel's name, `None` for a direct connection.
    pub tunnel: Option<String>,
    /// Whether this is the window's active context - the Resource panel's.
    pub active: bool,
    pub health: ContextHealth,
    pub severity: Severity,
    pub icon: IconName,
    pub text: &'static str,
    /// `None` only for `ContextHealth::Connected` - every other state shows how long
    /// it's been in it, per the spec's "Window status bar" requirement.
    pub elapsed: Option<Duration>,
}

/// Where "now" comes from for a [`StatusItem`]'s elapsed time and escalation: real time
/// in production, a manually advanced value in tests (tasks.md 2.2) - so a 30-second
/// escalation is provable without a 30-second-long test, independent of whether the
/// tick's own timer has fired.
#[derive(Clone)]
enum Clock {
    System,
    #[cfg(test)]
    Fake(std::rc::Rc<std::cell::Cell<Instant>>),
}

impl Clock {
    fn now(&self) -> Instant {
        match self {
            Clock::System => Instant::now(),
            #[cfg(test)]
            Clock::Fake(cell) => cell.get(),
        }
    }
}

/// The workspace window's bottom status bar. See the module doc comment.
pub struct StatusBarView {
    context_names: Vec<String>,
    /// Index into `context_names` of the window's active context.
    active: usize,
    /// The window this bar belongs to - every action (a capsule click, add,
    /// disconnect) only *asks* `MainWindow` to change its state, the one-writer rule
    /// the Resource panel's cluster dropdown follows too. Invalid for a bar built
    /// outside a window ([`Self::new`]), where those actions do nothing.
    main_window: WeakEntity<MainWindow>,
    tunnels_path: PathBuf,
    /// Every configured tunnel and every context's binding, for each capsule's tunnel
    /// label - loaded at construction and refreshed only on [`TunnelsRevision`].
    tunnel_choices: Vec<TunnelChoice>,
    tunnel_bindings: BTreeMap<String, String>,
    _tunnels_observation: Subscription,
    clock: Clock,
    // Kept alive for the view's lifetime; never read again once subscribed.
    _registry_observation: Subscription,
    _connection_observations: Vec<Subscription>,
    /// A one-second refresh loop, running only while [`Self::items`] has anything other
    /// than `Connected` (design.md decision 4) - `None` once everything is healthy, so an
    /// idle, fully connected window never wakes on a timer.
    tick: Option<Task<()>>,
    /// The chord the window is waiting to complete, if any ([`chord`]).
    chord: Option<chord::PendingChord>,
    _pending_input: Option<Subscription>,
    /// The rest of the Shortcut timeout, while GPUI's own timer is paused for
    /// the pending chord ([`chord`]); dropping it cancels the wait.
    extension: Option<Task<()>>,
    /// Whether the pending chord has had its wait extended already, so the
    /// observer firing again for the same keys doesn't start another.
    extended: bool,
}

impl StatusBarView {
    /// A bar outside any window, for tests: it reports and draws its contexts, but
    /// its add and disconnect controls have no window to act on.
    #[cfg(test)]
    pub fn new(context_names: Vec<String>, cx: &mut Context<Self>) -> Self {
        Self::new_with_clock(context_names, WeakEntity::new_invalid(), Clock::System, cx)
    }

    /// The bar for `main_window`'s workspace, whose actions reach that window.
    pub(crate) fn for_window(
        context_names: Vec<String>,
        main_window: WeakEntity<MainWindow>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_with_clock(context_names, main_window, Clock::System, cx)
    }

    fn new_with_clock(
        context_names: Vec<String>,
        main_window: WeakEntity<MainWindow>,
        clock: Clock,
        cx: &mut Context<Self>,
    ) -> Self {
        // `ClusterRegistry`'s own writes already notify on every mutating call (see
        // `session.rs`'s `pause_and_resume_each_notify_registry_observers` test), which
        // covers a session appearing, and every pause/resume. `ConnectionState` changes
        // notify their own `ClusterConnection` entity instead (`connection.rs`'s
        // `connect`), not the registry - task 1.3's audit - so each shown context's
        // connection is observed directly too, rather than routing that entity's writes
        // through the registry for no other benefit.
        let registry_observation =
            cx.observe_global::<ClusterRegistry>(|this, cx| this.refresh(cx));
        let connection_observations = Self::observe_connections(&context_names, cx);
        let tunnels_path = crate::util::paths::preference_dir().join("tunnels.toml");
        let (tunnel_choices, tunnel_bindings) = load_tunnels(&tunnels_path);
        let tunnels_observation = cx.observe_global::<TunnelsRevision>(|this, cx| {
            let (choices, bindings) = load_tunnels(&this.tunnels_path);
            this.tunnel_choices = choices;
            this.tunnel_bindings = bindings;
            cx.notify();
        });
        let mut this = Self {
            context_names,
            active: 0,
            main_window,
            tunnels_path,
            tunnel_choices,
            tunnel_bindings,
            _tunnels_observation: tunnels_observation,
            clock,
            _registry_observation: registry_observation,
            _connection_observations: connection_observations,
            tick: None,
            chord: None,
            _pending_input: None,
            extension: None,
            extended: false,
        };
        this.ensure_tick(cx);
        this
    }

    fn observe_connections(context_names: &[String], cx: &mut Context<Self>) -> Vec<Subscription> {
        context_names
            .iter()
            .map(|context_name| {
                let connection = ClusterRegistry::connection(cx, context_name);
                cx.observe(&connection, |this: &mut Self, _connection, cx| {
                    this.refresh(cx)
                })
            })
            .collect()
    }

    /// `window-context-bar` task 4.1: lets the bar's item source track the window's
    /// live `contexts` - adding or disconnecting a context (section 3) - rather than
    /// staying pinned to whatever list [`Self::new`] was built with. Rebuilds the
    /// per-connection observations for the new list, so a context just added starts
    /// being watched and one just dropped stops being read.
    pub fn set_context_names(&mut self, context_names: Vec<String>, cx: &mut Context<Self>) {
        self._connection_observations = Self::observe_connections(&context_names, cx);
        self.context_names = context_names;
        self.refresh(cx);
    }

    /// Marks `active` (an index into the bar's contexts) as the window's active
    /// context. `MainWindow::sync_context_children` calls this beside
    /// [`Self::set_context_names`].
    pub(crate) fn set_active(&mut self, active: usize, cx: &mut Context<Self>) {
        self.active = active;
        cx.notify();
    }

    fn tunnel_label(&self, context_name: &str) -> Option<String> {
        let id = self.tunnel_bindings.get(context_name)?;
        self.tunnel_choices
            .iter()
            .find(|choice| &choice.id == id)
            .map(|choice| choice.name.clone())
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.ensure_tick(cx);
        cx.notify();
    }

    /// Starts the tick if any item needs one and none is running, or stops it once
    /// nothing does. Called on construction and every refresh, so the tick's own
    /// lifetime never drifts from what [`Self::items`] currently reports.
    fn ensure_tick(&mut self, cx: &mut Context<Self>) {
        if self.any_unhealthy(cx) {
            if self.tick.is_some() {
                return;
            }
            self.tick = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(STATUS_TICK_INTERVAL).await;
                    let Ok(still_unhealthy) = this.update(cx, |this, cx| {
                        cx.notify();
                        this.any_unhealthy(cx)
                    }) else {
                        return;
                    };
                    if !still_unhealthy {
                        let _ = this.update(cx, |this, _cx| this.tick = None);
                        return;
                    }
                }
            }));
        } else {
            self.tick = None;
        }
    }

    fn any_unhealthy(&self, cx: &App) -> bool {
        self.items(cx)
            .iter()
            .any(|item| item.health != ContextHealth::Connected)
    }

    /// One item per context this window uses, non-connected items first (the spec's
    /// "Problem items first" scenario), stable otherwise. `self.clock.now()` in
    /// production; tests call this directly after advancing a fake clock, decoupling the
    /// elapsed-time math from whether the tick's own timer has fired yet.
    pub fn items(&self, cx: &App) -> Vec<StatusItem> {
        let now = self.clock.now();
        let mut items: Vec<StatusItem> = self
            .context_names
            .iter()
            .enumerate()
            .map(|(index, context_name)| StatusItem {
                tunnel: self.tunnel_label(context_name),
                active: index == self.active,
                ..Self::item_for(context_name, cx, now)
            })
            .collect();
        items.sort_by_key(|item| item.health == ContextHealth::Connected);
        items
    }

    fn item_for(context_name: &str, cx: &App, now: Instant) -> StatusItem {
        let health = ClusterRegistry::health(cx, context_name);
        let (icon, text) = Self::icon_and_text(&health);
        let elapsed = Self::elapsed(&health, now);
        StatusItem {
            context_name: context_name.to_string(),
            tunnel: None,
            active: false,
            severity: severity(&health, now),
            health,
            icon,
            text,
            elapsed,
        }
    }

    /// Every state's icon and text, per the spec's "Readable without color" scenario -
    /// each pairs with a distinct icon and label, not shared across states.
    fn icon_and_text(health: &ContextHealth) -> (IconName, &'static str) {
        match health {
            ContextHealth::Connected => (IconName::CircleCheck, "Connected"),
            ContextHealth::WaitingForTunnel { .. } => (IconName::Plug, "Waiting for tunnel"),
            ContextHealth::Paused {
                reason: PauseReason::Reconnecting,
                ..
            } => (IconName::RefreshCw, "Reconnecting"),
            ContextHealth::Paused {
                reason: PauseReason::CredentialRefresh,
                ..
            } => (IconName::KeyRound, "Refreshing credentials"),
            ContextHealth::Failed { .. } => (IconName::CircleAlert, "Connection failed"),
        }
    }

    fn elapsed(health: &ContextHealth, now: Instant) -> Option<Duration> {
        match health {
            ContextHealth::Connected => None,
            ContextHealth::WaitingForTunnel { since }
            | ContextHealth::Paused { since, .. }
            | ContextHealth::Failed { since, .. } => Some(now.saturating_duration_since(*since)),
        }
    }
}

impl Render for StatusBarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let items = self.items(cx);
        let space = crate::ui::space::spacing(cx);
        let scale = crate::ui::space::TextScale::current(cx).factor();
        let this = cx.weak_entity();
        div()
            .flex()
            .items_center()
            .gap(space.control_gap)
            .h(px((STATUS_BAR_HEIGHT * scale).round()))
            .flex_shrink_0()
            .px(space.panel_inset)
            .border_t_1()
            .border_color(theme.border)
            // The capsules and add control scroll sideways when a window holds
            // more contexts than fit, so the theme switcher keeps its end.
            .child(
                div()
                    .id("status-capsules")
                    .flex_1()
                    .min_w_0()
                    .overflow_x_scroll()
                    .flex()
                    .items_center()
                    .gap(space.control_gap)
                    .children(
                        items
                            .into_iter()
                            .map(|item| capsule::render_capsule(item, this.clone(), &theme)),
                    )
                    .child(capsule::render_add_button(this)),
            )
            .children(self.chord.as_ref().map(|chord| chord::render(chord, cx)))
            .child(theme_switch::render_theme_switch(cx))
    }
}

// Deliberately not `use super::*;` here or in `tests.rs`: that glob would re-import
// `gpui_kit::*`'s huge re-export surface (all of `gpui`/`gpui-component`) a second time
// through this module, which - combined with `IconName`'s ~2500 variants and a
// `#[cfg(test)]`/`#[gpui_kit::test]`-annotated item in the same module - blows the
// compiler's macro-expansion budget on this toolchain (confirmed by bisection: the exact
// same code compiles with named imports, and crashes/`recursion_limit` errors with a
// glob). Import specific names instead, here and in `tests.rs`.
#[cfg(test)]
mod tests;
#[cfg(test)]
mod timeout_tests;
