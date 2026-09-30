//! `window-context-bar` section 3: the context bar along the top of every workspace
//! window, below the title bar - one chip per context the window uses.
//!
//! Design.md decision 4 places this between the title bar and the workspace body: a
//! chip names its context, its bound tunnel (if any), and a health dot matching the
//! status bar's own severity; clicking it sets the window's `active` context, kept in
//! sync both ways with the Resource panel's cluster dropdown. The trailing "+" opens a
//! popover wrapping the cluster picker, filtered to contexts this window doesn't use
//! yet; each chip's own menu offers Disconnect.
//!
//! Tunnel names are cached and refreshed only on [`TunnelsRevision`], the same rule
//! `ui/picker.rs` follows - `render` must never read `tunnels.toml` itself.

use crate::k8s::cluster::context_health::{Severity, severity};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::picker::{ClusterPicker, PickerEvent, load_tunnels};
use crate::ui::picker_tunnel::TunnelChoice;
use crate::ui::tunnels::TunnelsRevision;
use crate::util::context_lifecycle;
use crate::util::shell::MainWindow;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Root;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dialog::DialogButtonProps;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

/// The bar's fixed height - layout-only, so it stays local rather than in
/// `consts.rs` (see `.claude/rules/rust-structure.md`).
const CONTEXT_BAR_HEIGHT: f32 = 36.;

/// One chip's rendered content, computed from the window's state and the caches
/// below - a plain struct rather than an `AnyElement`, mirroring
/// `ui/status_bar.rs`'s `StatusItem` so the same "testable without rendering" rule
/// applies here.
#[derive(Debug, Clone, PartialEq)]
struct ChipData {
    context_name: String,
    active: bool,
    tunnel: Option<String>,
    severity: Severity,
}

/// The context bar. See the module doc comment.
pub struct ContextBarView {
    contexts: Vec<String>,
    active: usize,
    /// The window this bar belongs to - every action (a chip click, "+", Disconnect)
    /// only *asks* `MainWindow` to change its state, the same one-writer rule
    /// `ui/panel/resource.rs`'s cluster dropdown follows.
    main_window: WeakEntity<MainWindow>,
    tunnels_path: PathBuf,
    /// Every configured tunnel and every context's binding, for each chip's tunnel
    /// label - loaded once at construction and refreshed only on
    /// [`TunnelsRevision`], never on render (same caching rule as `ui/picker.rs`).
    tunnel_choices: Vec<TunnelChoice>,
    tunnel_bindings: BTreeMap<String, String>,
    _tunnels_observation: Subscription,
    /// `ClusterRegistry`'s own pause/resume writes notify this global on every
    /// mutating call - the same signal `ui/status_bar.rs` reads for the same reason
    /// (see that module's own note on `ConnectionState` needing its own
    /// observation too).
    _registry_observation: Subscription,
    /// One per context in `contexts`, rebuilt by [`Self::set_state`] whenever that
    /// list changes - a `ConnectionState` transition (e.g. failed, waiting for a
    /// tunnel) notifies its own entity rather than the registry, so each shown
    /// context's connection needs its own observation too.
    _connection_observations: Vec<Subscription>,
}

impl ContextBarView {
    pub(crate) fn new(
        contexts: Vec<String>,
        active: usize,
        main_window: WeakEntity<MainWindow>,
        cx: &mut Context<Self>,
    ) -> Self {
        let tunnels_path = crate::util::paths::preference_dir().join("tunnels.toml");
        let (tunnel_choices, tunnel_bindings) = load_tunnels(&tunnels_path);
        let tunnels_observation = cx.observe_global::<TunnelsRevision>(|this, cx| {
            let (choices, bindings) = load_tunnels(&this.tunnels_path);
            this.tunnel_choices = choices;
            this.tunnel_bindings = bindings;
            cx.notify();
        });
        let registry_observation = cx.observe_global::<ClusterRegistry>(|_this, cx| cx.notify());
        let connection_observations = Self::observe_connections(&contexts, cx);
        Self {
            contexts,
            active,
            main_window,
            tunnels_path,
            tunnel_choices,
            tunnel_bindings,
            _tunnels_observation: tunnels_observation,
            _registry_observation: registry_observation,
            _connection_observations: connection_observations,
        }
    }

    fn observe_connections(contexts: &[String], cx: &mut Context<Self>) -> Vec<Subscription> {
        contexts
            .iter()
            .map(|context_name| {
                let connection = ClusterRegistry::connection(cx, context_name);
                cx.observe(&connection, |_this: &mut Self, _connection, cx| cx.notify())
            })
            .collect()
    }

    /// `MainWindow` calls this whenever its own `contexts`/`active` change - a chip
    /// click, an add, or a disconnect - so the bar is never a stale copy of state
    /// `MainWindow` already owns.
    pub(crate) fn set_state(
        &mut self,
        contexts: Vec<String>,
        active: usize,
        cx: &mut Context<Self>,
    ) {
        self._connection_observations = Self::observe_connections(&contexts, cx);
        self.contexts = contexts;
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

    /// One [`ChipData`] per context, in the window's own order.
    fn chips(&self, cx: &App) -> Vec<ChipData> {
        let now = Instant::now();
        self.contexts
            .iter()
            .enumerate()
            .map(|(index, context_name)| ChipData {
                context_name: context_name.clone(),
                active: index == self.active,
                tunnel: self.tunnel_label(context_name),
                severity: severity(&ClusterRegistry::health(cx, context_name), now),
            })
            .collect()
    }

    fn severity_color(theme: &gpui_kit::component::Theme, severity: Severity) -> Hsla {
        match severity {
            Severity::Muted => theme.muted_foreground,
            Severity::Info => theme.info,
            Severity::Warning => theme.warning,
            Severity::Danger => theme.danger,
        }
    }

    fn on_chip_clicked(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(context_name) = self.contexts.get(index).cloned() else {
            return;
        };
        let Some(main_window) = self.main_window.upgrade() else {
            return;
        };
        main_window.update(cx, |main_window, cx| {
            main_window.set_active_context(&context_name, window, cx);
        });
    }

    fn on_disconnect_clicked(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(context_name) = self.contexts.get(index).cloned() else {
            return;
        };
        self.open_disconnect_dialog(context_name, window, cx);
    }

    /// Section 3.3: the Disconnect confirmation, naming how many panels will close
    /// and, if another window still uses the context, that it stays connected
    /// there. `on_ok` calls back into `MainWindow::disconnect_context`, which does
    /// the actual work.
    fn open_disconnect_dialog(
        &mut self,
        context_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(main_window) = self.main_window.upgrade() else {
            return;
        };
        let panel_count = main_window.read(cx).context_panel_count(&context_name);
        let other_windows = ClusterRegistry::holder_count(cx, &context_name).saturating_sub(1);
        let body = context_lifecycle::disconnect_confirmation_body(
            &context_name,
            panel_count,
            other_windows,
        );
        let title = format!("Disconnect {context_name}?");
        let target = self.main_window.clone();

        Root::update(window, cx, |root, window, cx| {
            root.open_dialog(
                move |dialog, _window, _cx| {
                    let target = target.clone();
                    let context_name = context_name.clone();
                    dialog
                        .title(title.clone())
                        .child(body.clone())
                        .button_props(
                            DialogButtonProps::default()
                                .ok_text("Disconnect")
                                .ok_variant(ButtonVariant::Danger)
                                .show_cancel(true)
                                .on_ok(move |_event, window, cx| {
                                    let context_name = context_name.clone();
                                    let _ = target.update(cx, |main_window, cx| {
                                        main_window.disconnect_context(context_name, window, cx);
                                    });
                                    true
                                }),
                        )
                },
                window,
                cx,
            );
        });
    }

    /// Section 3.2: the "+" popover, a thin wrapper around the picker filtered to
    /// contexts this window doesn't already use - so tunnel labels and connect
    /// progress come for free (design.md decision 4) rather than a second
    /// implementation. `PickerEvent::Connected` only fires once the chosen context
    /// actually connects, which is what makes a failed connect open no panel
    /// (`MainWindow::add_context` defers opening Pods until then).
    fn open_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let used = self.contexts.clone();
        let picker = cx.new(|cx| {
            let mut picker = ClusterPicker::new(window, cx);
            picker.exclude(&used);
            picker
        });
        let target = self.main_window.clone();
        cx.subscribe_in(&picker, window, {
            move |_this: &mut Self, _picker, event, window, cx| {
                let PickerEvent::Connected { context_name } = event;
                let context_name = context_name.clone();
                let _ = target.update(cx, |main_window, cx| {
                    main_window.add_context(context_name, window, cx);
                });
                Root::update(window, cx, |root, window, cx| {
                    root.close_dialog(window, cx);
                });
            }
        })
        .detach();

        Root::update(window, cx, |root, window, cx| {
            root.open_dialog(
                move |dialog, _window, _cx| {
                    dialog
                        .title("Add a context")
                        .w(px(480.))
                        .child(picker.clone())
                },
                window,
                cx,
            );
        });
    }

    fn render_add_button(this: WeakEntity<Self>) -> impl IntoElement {
        Button::new("context-bar-add")
            .icon(IconName::Plus)
            .xsmall()
            .ghost()
            .tooltip("Add a context")
            .on_click(move |_event, window, cx| {
                let _ = this.update(cx, |bar, cx| bar.open_add_dialog(window, cx));
            })
    }

    fn render_chip(
        index: usize,
        chip: ChipData,
        this: WeakEntity<Self>,
        theme: &gpui_kit::component::Theme,
    ) -> AnyElement {
        let dot_color = Self::severity_color(theme, chip.severity);
        let click_target = this.clone();
        let body = Button::new(SharedString::from(format!(
            "context-chip-{}",
            chip.context_name
        )))
        .ghost()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(dot_color))
                .child(chip.context_name.clone())
                .children(chip.tunnel.map(|tunnel| {
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(tunnel)
                })),
        )
        .on_click(move |_event, window, cx| {
            let _ = click_target.update(cx, |bar, cx| bar.on_chip_clicked(index, window, cx));
        });

        let menu_target = this;
        let menu_button = Button::new(SharedString::from(format!(
            "context-chip-menu-{}",
            chip.context_name
        )))
        .icon(IconName::ChevronDown)
        .xsmall()
        .ghost()
        .dropdown_menu(move |menu, _window, _cx| {
            let menu_target = menu_target.clone();
            menu.item(
                PopupMenuItem::new("Disconnect").on_click(move |_event, window, cx| {
                    let _ = menu_target
                        .update(cx, |bar, cx| bar.on_disconnect_clicked(index, window, cx));
                }),
            )
        });

        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .rounded_md()
            .when(chip.active, |el| el.bg(theme.selection))
            .child(body)
            .child(menu_button)
            .into_any_element()
    }
}

impl Render for ContextBarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let this = cx.weak_entity();
        let chips = self.chips(cx);
        let rendered: Vec<AnyElement> = chips
            .into_iter()
            .enumerate()
            .map(|(index, chip)| Self::render_chip(index, chip, this.clone(), &theme))
            .collect();
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(CONTEXT_BAR_HEIGHT))
            .flex_shrink_0()
            .px_3()
            .border_b_1()
            .border_color(theme.border)
            .children(rendered)
            .child(Self::render_add_button(this))
    }
}

// Not `use super::*;` in a sibling test module here: `gpui_kit::*`'s re-export
// surface combined with `IconName`'s ~2500 variants blows the compiler's
// macro-expansion budget alongside a `#[cfg(test)]`/`#[gpui_kit::test]`-annotated
// item in the same module (see `ui/status_bar.rs`'s note on the same crash).
// Named imports instead, here and in `tests.rs`.
#[cfg(test)]
mod tests;
