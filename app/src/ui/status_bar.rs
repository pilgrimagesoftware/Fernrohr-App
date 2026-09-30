//! `connection-status-bar`: the workspace window's bottom status bar, one item per
//! cluster context the window uses - context name, state icon and text, and, for any
//! state other than connected, elapsed time - colored by [`Severity`]. `util/shell.rs`
//! renders this under the workspace body in `WindowMode::Workspace`, not in picker mode.
//!
//! The bar takes the window's contexts as a `Vec<String>` (design.md decision 3): today
//! that is always `MainWindow`'s single `context_name`, wrapped in a one-element vector,
//! so the later `window-context-bar` change that makes it a real list is a one-line
//! switch at the call site rather than a signature change here.

use crate::consts::STATUS_TICK_INTERVAL;
use crate::k8s::cluster::context_health::{ContextHealth, Severity, severity};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::cluster::watch_registry::PauseReason;
use crate::k8s::resource::pods::format_age;
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::*;
use std::time::{Duration, Instant};

/// The bar's fixed height - layout-only, so it stays local rather than in `consts.rs`
/// (see `.claude/rules/rust-structure.md`).
const STATUS_BAR_HEIGHT: f32 = 28.;

/// One status bar item's content, computed from [`ClusterRegistry::health`] - a plain
/// struct rather than an `AnyElement`, so tests can assert on it directly instead of
/// inspecting rendered output (design.md decision 2's "testable without rendering",
/// extended from `severity` to the whole item).
#[derive(Debug, Clone, PartialEq)]
pub struct StatusItem {
    pub context_name: String,
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
    clock: Clock,
    // Kept alive for the view's lifetime; never read again once subscribed.
    _registry_observation: Subscription,
    _connection_observations: Vec<Subscription>,
    /// A one-second refresh loop, running only while [`Self::items`] has anything other
    /// than `Connected` (design.md decision 4) - `None` once everything is healthy, so an
    /// idle, fully connected window never wakes on a timer.
    tick: Option<Task<()>>,
}

impl StatusBarView {
    pub fn new(context_names: Vec<String>, cx: &mut Context<Self>) -> Self {
        Self::new_with_clock(context_names, Clock::System, cx)
    }

    fn new_with_clock(context_names: Vec<String>, clock: Clock, cx: &mut Context<Self>) -> Self {
        // `ClusterRegistry`'s own writes already notify on every mutating call (see
        // `session.rs`'s `pause_and_resume_each_notify_registry_observers` test), which
        // covers a session appearing, and every pause/resume. `ConnectionState` changes
        // notify their own `ClusterConnection` entity instead (`connection.rs`'s
        // `connect`), not the registry - task 1.3's audit - so each shown context's
        // connection is observed directly too, rather than routing that entity's writes
        // through the registry for no other benefit.
        let registry_observation =
            cx.observe_global::<ClusterRegistry>(|this, cx| this.refresh(cx));
        let connection_observations = context_names
            .iter()
            .map(|context_name| {
                let connection = ClusterRegistry::connection(cx, context_name);
                cx.observe(&connection, |this: &mut Self, _connection, cx| {
                    this.refresh(cx)
                })
            })
            .collect();
        let mut this = Self {
            context_names,
            clock,
            _registry_observation: registry_observation,
            _connection_observations: connection_observations,
            tick: None,
        };
        this.ensure_tick(cx);
        this
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
            .map(|context_name| Self::item_for(context_name, cx, now))
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

    fn color(theme: &gpui_kit::component::Theme, severity: Severity) -> Hsla {
        match severity {
            Severity::Muted => theme.muted_foreground,
            Severity::Info => theme.info,
            Severity::Warning => theme.warning,
            Severity::Danger => theme.danger,
        }
    }

    fn render_item(item: &StatusItem, theme: &gpui_kit::component::Theme) -> impl IntoElement {
        let color = Self::color(theme, item.severity);
        div()
            .flex()
            .items_center()
            .gap_1p5()
            .text_sm()
            .text_color(color)
            .child(Icon::new(item.icon).size(px(12.)).text_color(color))
            .child(item.context_name.clone())
            .child(item.text)
            .children(
                item.elapsed
                    .map(|elapsed| format!("({} ago)", format_age(elapsed.as_secs() as i64))),
            )
    }
}

impl Render for StatusBarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let items = self.items(cx);
        div()
            .flex()
            .items_center()
            .gap_4()
            .h(px(STATUS_BAR_HEIGHT))
            .flex_shrink_0()
            .px_3()
            .border_t_1()
            .border_color(theme.border)
            .children(items.iter().map(|item| Self::render_item(item, &theme)))
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
