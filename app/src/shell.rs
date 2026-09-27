use crate::command::{Command, CommandRegistry};
use crate::config::{
    self,
    workspace::{PanelDescriptor, WindowLayout, WorkspaceConfig},
};
use crate::keymap;
use crate::nav::{self, NavTarget, ShowLogs, ShowPods};
use crate::paths;
use crate::ui::panel_title::{self, PanelScope};
use gpui_kit::component::Root;
use gpui_kit::component::dock::{DockArea, DockEvent, DockPlacement, DockSkin, PanelId};
use gpui_kit::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

actions!(shell, [NewWindow, ToggleCommandPalette]);

/// Last known geometry of every window that has closed this run, keyed by
/// `WindowId`. Populated from each window's `on_window_should_close` hook,
/// since under `QuitMode::LastWindowClosed` the app-quit callback fires only
/// after every window is already gone - `cx.windows()` is empty by then, so
/// geometry has to be captured on the way out rather than read at quit time.
#[derive(Default)]
struct ClosedWindowLayouts(HashMap<WindowId, WindowLayout>);

impl Global for ClosedWindowLayouts {}

pub const NEW_WINDOW_COMMAND_ID: &str = "shell.new_window";
pub const NEW_WINDOW_DEFAULT_BINDING: &str = "cmd-n";
pub const TOGGLE_PALETTE_COMMAND_ID: &str = "shell.toggle_command_palette";
pub const TOGGLE_PALETTE_DEFAULT_BINDING: &str = "cmd-shift-p";

pub fn default_workspace_path() -> PathBuf {
    paths::state_dir().join("workspace.toml")
}

/// The commands this module contributes to the app-wide [`CommandRegistry`].
/// Built here (next to the actions they wrap) rather than centrally, so a
/// command's metadata lives beside the action it dispatches.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: NEW_WINDOW_COMMAND_ID,
        title: "New Window",
        default_binding: NEW_WINDOW_DEFAULT_BINDING,
        context: None,
        action: Box::new(NewWindow),
    });
    registry.register(Command {
        id: TOGGLE_PALETTE_COMMAND_ID,
        title: "Command Palette",
        default_binding: TOGGLE_PALETTE_DEFAULT_BINDING,
        context: None,
        action: Box::new(ToggleCommandPalette),
    });
    nav::register_commands(registry);
}

/// Builds the command registry, binds its commands' actions - each to
/// `keymap_path`'s override if it has one, otherwise its default - stores
/// the registry as a global so [`MainWindow`] can build palette items from
/// it, and arranges for the workspace to be persisted at `workspace_path`
/// when the app is about to quit (see [`save`]).
pub fn init(cx: &mut App, workspace_path: PathBuf, keymap_path: &Path) {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let keymap = keymap::load(keymap_path, &registry);

    let new_window_binding =
        keymap::resolve(NEW_WINDOW_COMMAND_ID, NEW_WINDOW_DEFAULT_BINDING, &keymap);
    let palette_binding = keymap::resolve(
        TOGGLE_PALETTE_COMMAND_ID,
        TOGGLE_PALETTE_DEFAULT_BINDING,
        &keymap,
    );
    let show_pods_binding = keymap::resolve(
        nav::SHOW_PODS_COMMAND_ID,
        nav::SHOW_PODS_DEFAULT_BINDING,
        &keymap,
    );
    let show_logs_binding = keymap::resolve(
        nav::SHOW_LOGS_COMMAND_ID,
        nav::SHOW_LOGS_DEFAULT_BINDING,
        &keymap,
    );
    cx.bind_keys([
        KeyBinding::new(&new_window_binding, NewWindow, None),
        KeyBinding::new(&palette_binding, ToggleCommandPalette, None),
        KeyBinding::new(&show_pods_binding, ShowPods, None),
        KeyBinding::new(&show_logs_binding, ShowLogs, None),
    ]);
    cx.on_action(|_: &NewWindow, cx: &mut App| {
        open_window(cx, WindowLayout::default());
    });

    cx.set_global(registry);

    cx.on_app_quit(move |cx| {
        save(cx, &workspace_path);
        async {}
    })
    .detach();
}

/// Descriptors this build knows how to restore, in persisted order. A
/// descriptor whose `kind` this build doesn't recognize deserializes as
/// [`PanelDescriptor::Unknown`] (see `config::workspace`) and is skipped here
/// with a log line, rather than failing the whole layout.
pub fn restorable_panels(layout: &WindowLayout) -> Vec<&PanelDescriptor> {
    layout
        .panels
        .iter()
        .filter(|descriptor| match descriptor {
            PanelDescriptor::Unknown => {
                log::warn!("skipping unknown panel kind on workspace restore");
                false
            }
            _ => true,
        })
        .collect()
}

fn window_bounds(layout: &WindowLayout, cx: &mut App) -> Bounds<Pixels> {
    let size = size(px(layout.width), px(layout.height));
    match (layout.x, layout.y) {
        (Some(x), Some(y)) => Bounds::new(Point::new(px(x), px(y)), size),
        _ => Bounds::centered(None, size, cx),
    }
}

fn layout_from_bounds(bounds: Bounds<Pixels>) -> WindowLayout {
    WindowLayout {
        width: f32::from(bounds.size.width),
        height: f32::from(bounds.size.height),
        x: Some(f32::from(bounds.origin.x)),
        y: Some(f32::from(bounds.origin.y)),
        panels: Vec::new(),
    }
}

/// Builds the workspace dock for `context_name` and opens its first panel:
/// Pods, the default view for a freshly connected cluster. Every later panel
/// goes through `MainWindow::open_target`, which reuses this dock - and the
/// window's `ClusterSession` - rather than rebuilding either.
fn build_workspace(
    context_name: String,
    connection_count: usize,
    window: &mut Window,
    cx: &mut App,
) -> (
    Entity<DockArea>,
    Rc<DockSkin>,
    PanelScope,
    (PanelId, nav::OpenedPanel),
) {
    let (dock_area, dock_skin) = DockSkin::dock_area("main", Some(1), window, cx);
    let scope = PanelScope {
        connection_count,
        ..PanelScope::new(NavTarget::pods(), context_name)
    };
    // Returned so the window records it in `open_panels` like any other: a
    // window that opened Pods at startup must still recognise Pods as open,
    // or the first click on the Pod row would duplicate it (spec 9.3). The
    // scope comes back too, so the key is derived from the very scope the panel
    // was built with rather than restated beside it.
    let first = dock_area.update(cx, |area, cx| nav::add_panel(area, &scope, window, cx));
    (dock_area, dock_skin, scope, first)
}

/// The first restorable panel's `cluster_context`, if any - used to pick which context
/// a restored (non-empty) layout's workspace connects to, since full per-panel
/// reconstruction from `PanelDescriptor` is future work.
fn first_restored_context(layout: &WindowLayout) -> Option<String> {
    restorable_panels(layout)
        .into_iter()
        .find_map(|descriptor| match descriptor {
            PanelDescriptor::Pods {
                cluster_context, ..
            }
            | PanelDescriptor::Logs {
                cluster_context, ..
            } => Some(cluster_context.clone()),
            PanelDescriptor::Unknown => None,
        })
}

/// Enough to recognise a panel the dock already holds: which kind, in which
/// cluster, over which namespace scope. The spec's rule is "the same kind,
/// cluster, and namespace scope" (9.3), so all three are in the key - a window
/// with two clusters open, or two namespace pickers on one kind, needs them.
///
/// `namespace` is `None` for a panel whose title-bar picker still reads "All
/// namespaces", which is every panel until the picker has a narrower scope to
/// offer.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct PanelKey {
    target: NavTarget,
    context_name: String,
    namespaces: Vec<String>,
}

impl From<&PanelScope> for PanelKey {
    /// Derived rather than built alongside, so the key a panel is filed under
    /// and the scope its title bar draws cannot describe different panels -
    /// which is exactly the disagreement 10.2's namespace picker would
    /// otherwise create.
    fn from(scope: &PanelScope) -> Self {
        Self {
            target: scope.target.clone(),
            context_name: scope.context_name.clone(),
            namespaces: scope.namespaces.clone(),
        }
    }
}

/// A panel the window opened, and how to find it again in the dock.
struct OpenPanel {
    key: PanelKey,
    id: PanelId,
}

/// A window's body: the cluster picker (no connected context yet) or a connected
/// workspace. A window opens in `Picker` whenever it has no restored panels, per the
/// `cluster-picker` and `app-shell` specs.
enum WindowMode {
    Picker(Entity<crate::ui::picker::ClusterPicker>),
    Workspace {
        dock_area: Entity<DockArea>,
        /// Keeps the dock's renderer alive. The default skin uses GPUI focus
        /// state for active panel chrome and provides its zoom control.
        _dock_skin: Rc<DockSkin>,
        context_name: String,
        /// The discovered-kind list in the window's left edge. It reads the
        /// same `ClusterSession` as `dock_area`, so picking a kind opens a
        /// panel without reconnecting.
        resource_panel: Entity<crate::ui::resource_panel::ResourcePanel>,
        /// Every panel this window has opened, in the order it opened them.
        /// The dock does not report which panel a `PanelId` belongs to, so
        /// this is also how a closed panel is recognised as closed.
        open_panels: Vec<OpenPanel>,
        /// What the Resource panel's row should mark. The window's most
        /// recently opened or focused kind, which is not the same as the
        /// dock's active panel once the user clicks tabs directly (section 12
        /// tracks that).
        nav: Box<NavTarget>,
        /// How many cluster connections this window holds. One today: adding a
        /// second connection to an already-connected window is an explicit
        /// non-goal of this change (`design.md`), so the count is the window's
        /// to state and pass on rather than something panels assume. Section
        /// 10.1's title bar reads it.
        connection_count: usize,
    },
}

/// Opens one window, in `Picker` mode if `layout` has no restorable panels, or directly
/// into a connected workspace (seeded from the restored layout's context) otherwise.
pub fn open_window(cx: &mut App, layout: WindowLayout) {
    let bounds = window_bounds(&layout, cx);
    let restored_context = first_restored_context(&layout);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        |window, cx| {
            crate::theme::watch_window(window, cx);

            let window_id = window.window_handle().window_id();
            window.on_window_should_close(cx, move |window, cx| {
                let layout = layout_from_bounds(window.bounds());
                if !cx.has_global::<ClosedWindowLayouts>() {
                    cx.set_global(ClosedWindowLayouts::default());
                }
                cx.global_mut::<ClosedWindowLayouts>()
                    .0
                    .insert(window_id, layout);
                true
            });

            let view = cx.new(|cx| {
                let mut view = MainWindow {
                    mode: WindowMode::Picker(
                        cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
                    ),
                    focus_handle: cx.focus_handle(),
                };
                let WindowMode::Picker(picker) = &view.mode else {
                    unreachable!("just constructed a picker-mode window")
                };
                watch_picker(picker, window, cx);
                if let Some(context_name) = restored_context {
                    view.enter_workspace(context_name, window, cx);
                }
                view
            });
            view.update(cx, |view, cx| view.focus_initial(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
    .expect("failed to open window");
}

/// Subscribes so a successful connect on `picker` swaps this window into
/// `Workspace` mode and arms [`watch_workspace`] on the new dock.
fn watch_picker(
    picker: &Entity<crate::ui::picker::ClusterPicker>,
    window: &mut Window,
    cx: &mut Context<MainWindow>,
) {
    cx.subscribe_in(
        picker,
        window,
        |this: &mut MainWindow, _picker, event, window, cx| {
            let crate::ui::picker::PickerEvent::Connected { context_name, .. } = event;
            this.enter_workspace(context_name.clone(), window, cx);
        },
    )
    .detach();
}

/// Subscribes so closing `dock_area`'s last panel swaps this window back into
/// `Picker` mode, per the `app-shell` spec's "closing the last panel returns
/// to the picker" scenario.
fn watch_workspace(
    dock_area: &Entity<DockArea>,
    window: &mut Window,
    cx: &mut Context<MainWindow>,
) {
    cx.subscribe_in(
        dock_area,
        window,
        |this: &mut MainWindow, dock_area, event, window, cx| {
            if !matches!(event, DockEvent::LayoutChanged) {
                return;
            }
            this.forget_closed_panels(dock_area, cx);
            if !dock_area.read(cx).is_empty(DockPlacement::Center, cx) {
                return;
            }
            this.enter_picker(window, cx);
        },
    )
    .detach();
}

pub struct MainWindow {
    mode: WindowMode,
    focus_handle: FocusHandle,
}

impl MainWindow {
    fn focus_initial(&self, window: &mut Window, cx: &mut App) {
        if let WindowMode::Picker(picker) = &self.mode {
            let picker = picker.clone();
            let focus_handle = picker.read(cx).command_focus_handle(cx);
            focus_handle.focus(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
    }

    /// Swaps this window to a connected workspace on `context_name`: the dock
    /// (defaulting to Pods), the Resource panel listing that cluster's
    /// discovered kinds, and the subscription that opens whatever is picked.
    /// Both windows-enter-workspace sites go through here so the Resource
    /// panel cannot be wired up in one of them and forgotten in the other.
    fn enter_workspace(
        &mut self,
        context_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // One connection per connected window in this change; see
        // `WindowMode::Workspace::connection_count`.
        const CONNECTIONS: usize = 1;
        let (dock_area, dock_skin, scope, (first_id, first)) =
            build_workspace(context_name.clone(), CONNECTIONS, window, cx);
        watch_workspace(&dock_area, window, cx);
        let resource_panel =
            cx.new(|cx| crate::ui::resource_panel::ResourcePanel::new(context_name.clone(), cx));
        cx.subscribe_in(
            &resource_panel,
            window,
            |this: &mut MainWindow, _panel, event, window, cx| {
                let crate::ui::resource_panel::ResourceEvent::Open(target) = event;
                this.open_target(target.clone(), window, cx);
            },
        )
        .detach();
        self.mode = WindowMode::Workspace {
            dock_area,
            _dock_skin: dock_skin,
            context_name,
            resource_panel,
            open_panels: vec![OpenPanel {
                key: PanelKey::from(&scope),
                id: first_id,
            }],
            nav: Box::new(NavTarget::pods()),
            connection_count: CONNECTIONS,
        };
        self.watch_scope_changes(first, window, cx);
        cx.notify();
    }

    /// Re-files an open panel under the scope it now shows.
    ///
    /// A panel's title-bar namespace picker re-scopes the panel in place
    /// (10.2). Without this the window would keep the panel under its old key,
    /// so asking for that old scope again would focus a panel no longer showing
    /// it, and the panel's current scope would be unreachable.
    fn watch_scope_changes(
        &mut self,
        opened: nav::OpenedPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // `PanelId` rather than the entity: the subscription closure has to be
        // `'static`, and an id says everything `rescope` needs.
        let id = opened.panel_id();
        let rekey = move |this: &mut MainWindow,
                          event: &panel_title::ScopeEvent,
                          cx: &mut Context<MainWindow>| {
            let panel_title::ScopeEvent::NamespacesChanged(namespaces) = event;
            this.rescope(id, namespaces.clone(), cx);
        };
        match opened {
            nav::OpenedPanel::Pods(panel) => {
                cx.subscribe_in(
                    &panel,
                    window,
                    move |this: &mut MainWindow, _panel, event, _window, cx| rekey(this, event, cx),
                )
                .detach();
            }
            nav::OpenedPanel::Placeholder(panel) => {
                cx.subscribe_in(
                    &panel,
                    window,
                    move |this: &mut MainWindow, _panel, event, _window, cx| rekey(this, event, cx),
                )
                .detach();
            }
            nav::OpenedPanel::Logs(panel) => {
                cx.subscribe_in(
                    &panel,
                    window,
                    move |this: &mut MainWindow, _panel, event, _window, cx| rekey(this, event, cx),
                )
                .detach();
            }
        }
    }

    /// Moves the panel with `id` to a new namespace, whichever panel type it is.
    fn rescope(&mut self, id: PanelId, namespaces: Vec<String>, cx: &mut Context<Self>) {
        let WindowMode::Workspace { open_panels, .. } = &mut self.mode else {
            return;
        };
        if let Some(open) = open_panels.iter_mut().find(|open| open.id == id) {
            open.key.namespaces = namespaces;
        }
        cx.notify();
    }

    fn enter_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx));
        watch_picker(&picker, window, cx);
        self.mode = WindowMode::Picker(picker);
        cx.notify();
    }

    fn on_action_show_pods(&mut self, _: &ShowPods, window: &mut Window, cx: &mut Context<Self>) {
        self.open_target(NavTarget::pods(), window, cx);
    }

    fn on_action_show_logs(&mut self, _: &ShowLogs, window: &mut Window, cx: &mut Context<Self>) {
        self.open_target(NavTarget::Logs, window, cx);
    }

    /// Shows `target`: a new panel in the dock if that kind is not already
    /// open in this window, otherwise a focus of the panel that is (spec 9.3).
    /// A no-op if the window has no workspace yet.
    ///
    /// Every route to a panel - double-click, the row's context menu, and the
    /// `nav.show_pods` / `nav.show_logs` commands - lands here, which is what
    /// makes 8.3's "same panel" and 9.2's "equivalent to double-click" hold by
    /// construction rather than by three call sites agreeing.
    ///
    /// Panels are built against the window's existing `context_name`, so
    /// opening one never reconnects.
    fn open_target(&mut self, target: NavTarget, window: &mut Window, cx: &mut Context<Self>) {
        let WindowMode::Workspace {
            dock_area,
            context_name,
            resource_panel,
            open_panels,
            nav,
            connection_count,
            ..
        } = &mut self.mode
        else {
            return;
        };
        let connection_count = *connection_count;
        // Set only when a panel was actually built, so the subscription below
        // is not made for a panel the dock already had.
        let mut watch_scope = None;
        let scope = PanelScope {
            connection_count,
            ..PanelScope::new(target.clone(), context_name.clone())
        };
        let key = PanelKey::from(&scope);
        match open_panels.iter().find(|open| open.key == key) {
            Some(open) => {
                let id = open.id;
                dock_area.update(cx, |area, cx| area.select_panel(id, window, cx));
            }
            None => {
                let (id, opened) =
                    dock_area.update(cx, |area, cx| nav::add_panel(area, &scope, window, cx));
                open_panels.push(OpenPanel { key, id });
                watch_scope = Some(opened);
            }
        }
        **nav = target;
        let showing = (**nav).clone();
        resource_panel.update(cx, |panel, cx| panel.set_selected(Some(showing), cx));
        if let Some(opened) = watch_scope {
            self.watch_scope_changes(opened, window, cx);
        }
        cx.notify();
    }

    /// Drops bookkeeping for panels the user closed, so re-selecting one later
    /// opens a fresh panel instead of focusing a dock id that no longer exists.
    fn forget_closed_panels(&mut self, dock_area: &Entity<DockArea>, cx: &mut App) {
        let WindowMode::Workspace { open_panels, .. } = &mut self.mode else {
            return;
        };
        let Some(tree) = dock_area.read(cx).layout(DockPlacement::Center) else {
            return;
        };
        let held: Vec<PanelId> = tree.panels().collect();
        open_panels.retain(|open| held.contains(&open.id));
    }
}

/// Opens the command palette in a dialog on `window`'s `Root`. A fresh
/// `CommandState` is created per open (not reused across opens) since the
/// palette's own query/selection state should reset each time it's summoned.
fn open_command_palette(window: &mut Window, cx: &mut App) {
    let Some(Some(root)) = window.root::<gpui_kit::component::Root>() else {
        return;
    };
    let items = cx.global::<CommandRegistry>();
    let items = crate::command::build_items(items, &[]);
    let state = cx.new(|cx| gpui_kit::component::command::CommandState::new(window, cx));

    root.update(cx, |root, cx| {
        root.open_dialog(
            move |dialog, _window, _cx| {
                let state = state.clone();
                let items = items.clone();
                dialog.content(move |content, _window, _cx| {
                    content.child(
                        gpui_kit::component::command::Command::new(&state)
                            .items(items.clone())
                            .placeholder("Type a command..."),
                    )
                })
            },
            window,
            cx,
        );
    });
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: AnyElement = match &self.mode {
            WindowMode::Picker(picker) => picker.clone().into_any_element(),
            // The Resource panel is the window's left edge. It used to be a
            // fixed Pods/Logs list built here; the kinds now come from the
            // cluster's own discovery (spec 8.1), so it owns its own chrome.
            WindowMode::Workspace {
                dock_area,
                resource_panel,
                ..
            } => div()
                .flex()
                .size_full()
                .child(resource_panel.clone())
                .child(dock_area.clone().into_any_element())
                .into_any_element(),
        };
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(|_: &ToggleCommandPalette, window, cx| {
                open_command_palette(window, cx);
            })
            .on_action(cx.listener(Self::on_action_show_pods))
            .on_action(cx.listener(Self::on_action_show_logs))
            .child(body)
    }
}

/// Opens the windows recorded at `workspace_path`, or one default window if
/// the file is missing, empty, or failed to parse (`config::load` already
/// guarantees defaults-without-touching-the-file in that last case).
pub fn open_saved_or_default(cx: &mut App, workspace_path: &Path) {
    let workspace: WorkspaceConfig = config::load(workspace_path);
    if workspace.windows.is_empty() {
        open_window(cx, WindowLayout::default());
    } else {
        for layout in workspace.windows {
            open_window(cx, layout);
        }
    }
}

/// Snapshots every window's geometry into `workspace_path`: still-open
/// windows read fresh from `cx.windows()`, plus any already closed this run
/// (see [`ClosedWindowLayouts`]) - under `QuitMode::LastWindowClosed` that's
/// every window, since the app-quit callback fires after the last one
/// closes. Panel descriptors are left empty until a later change adds panel
/// kinds worth restoring (see [`open_window`]'s placeholder split).
pub fn save(cx: &mut App, workspace_path: &Path) {
    let mut layouts = if cx.has_global::<ClosedWindowLayouts>() {
        cx.global::<ClosedWindowLayouts>().0.clone()
    } else {
        HashMap::new()
    };
    for handle in cx.windows() {
        if let Ok(layout) = handle.update(cx, |_, window, _cx| layout_from_bounds(window.bounds()))
        {
            layouts.insert(handle.window_id(), layout);
        }
    }
    let windows: Vec<WindowLayout> = layouts.into_values().collect();
    let _ = config::save(workspace_path, &WorkspaceConfig { windows });
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: `gpui_kit::*` re-exports its own `test` attribute
    // macro, which would shadow `core::prelude::v1::test` for the plain
    // synchronous test below.
    use super::{
        ClosedWindowLayouts, MainWindow, NavTarget, PanelDescriptor, PanelKey,
        ToggleCommandPalette, WindowLayout, WindowMode, WorkspaceConfig, config, init,
        open_saved_or_default, open_window, register_commands, restorable_panels, save,
    };
    use crate::cluster::discovery::DiscoveredKind;
    use crate::cluster::session::ClusterRegistry;
    use crate::command::CommandRegistry;
    use crate::nav;
    use crate::ui::panel_title::PanelScope;
    use gpui_kit::component::dock::{self, DockLayout, DockPlacement, PanelView as _};
    use gpui_kit::{App, AppContext as _, Entity, SharedString, TestAppContext, Window, WindowId};
    use kube::core::GroupVersionKind;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_workspace_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("fernrohr-shell-test-{n}.toml"))
    }

    /// A connected window on `context_name`, for the panel-opening tests.
    async fn connected_window(
        cx: &mut TestAppContext,
        context_name: &str,
    ) -> gpui_kit::WindowHandle<MainWindow> {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        cx.add_window(|window, cx| {
            let mut main_window = MainWindow {
                mode: WindowMode::Picker(
                    cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
                ),
                focus_handle: cx.focus_handle(),
            };
            main_window.enter_workspace(context_name.to_string(), window, cx);
            main_window
        })
    }

    /// A kind with no concrete panel, as discovery would report a CRD's.
    fn crd_kind() -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk("ferns.example.com", "v1", "Fern"),
            plural: "ferns".to_string(),
            namespaced: true,
        }
    }

    /// A cluster-scoped CRD, the kind 10.2 says must not grow a namespace picker.
    fn cluster_scoped_kind() -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk("widgets.example.com", "v1", "Widget"),
            plural: "widgets".to_string(),
            namespaced: false,
        }
    }

    /// The title bar the dock builds for `panel`, read back the way the dock
    /// reads it: through `PanelView`, which is the object-safe face of `Panel`
    /// and takes no panel context.
    ///
    /// Returns the tab name, whether a namespace picker is on the bar, and how
    /// many controls sit at its trailing end.
    fn title_bar_of<T: dock::Panel>(
        panel: &Entity<T>,
        window: &mut Window,
        cx: &mut App,
    ) -> (Option<SharedString>, bool, usize) {
        let name = panel.tab_name(cx);
        let picker = panel.title_suffix(window, cx).is_some();
        let controls = panel
            .toolbar_buttons(window, cx)
            .map_or(0, |buttons| buttons.len());
        (name, picker, controls)
    }

    /// Section 9.1: selecting a kind adds a panel to the dock rather than
    /// replacing what was there, and it reuses the window's connection instead
    /// of opening a new one - so the kinds listed and the panels opened stay on
    /// one `ClusterSession`.
    #[gpui_kit::test]
    async fn opening_a_kind_adds_a_panel_and_keeps_the_session(cx: &mut TestAppContext) {
        let window = connected_window(cx, "kind-dev").await;
        cx.run_until_parked();

        let session_before =
            cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());

        window
            .update(cx, |main_window, window, cx| {
                let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                let before = dock_area
                    .read(cx)
                    .layout(DockPlacement::Center)
                    .expect("a workspace dock has a centre")
                    .panels()
                    .count();

                main_window.open_target(NavTarget::Kind(crd_kind()), window, cx);

                let dock_area = match &main_window.mode {
                    WindowMode::Workspace { dock_area, .. } => dock_area,
                    _ => unreachable!("open_target did not leave workspace mode"),
                };
                let after = dock_area
                    .read(cx)
                    .layout(DockPlacement::Center)
                    .expect("a workspace dock has a centre")
                    .panels()
                    .count();
                assert_eq!(after, before + 1, "the kind got its own panel");
            })
            .unwrap();
        cx.run_until_parked();

        let session_after = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());
        assert_eq!(
            session_before, session_after,
            "opening a panel must not reconnect the window"
        );
    }

    /// Section 9.3: a kind that already has a panel open is focused rather than
    /// opened a second time. The workspace starts with Pods open, so the first
    /// selection of Pods is already the "already open" case.
    #[gpui_kit::test]
    async fn reopening_a_kind_focuses_it_instead_of_duplicating(cx: &mut TestAppContext) {
        let window = connected_window(cx, "kind-dev").await;
        cx.run_until_parked();

        window
            .update(cx, |main_window, window, cx| {
                let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                assert_eq!(
                    open_panels.len(),
                    1,
                    "the workspace opens Pods and records it"
                );
                assert_eq!(
                    open_panels[0].key,
                    PanelKey {
                        target: NavTarget::pods(),
                        context_name: "kind-dev".to_string(),
                        namespaces: Vec::new(),
                    }
                );

                // A second, different kind is a genuinely new panel...
                main_window.open_target(NavTarget::Kind(crd_kind()), window, cx);
                // ...and the first one again, which must not add a third.
                main_window.open_target(NavTarget::pods(), window, cx);
                main_window.open_target(NavTarget::pods(), window, cx);

                let WindowMode::Workspace {
                    open_panels,
                    dock_area,
                    ..
                } = &main_window.mode
                else {
                    unreachable!("open_target did not leave workspace mode")
                };
                assert_eq!(
                    open_panels.len(),
                    2,
                    "two distinct kinds, however many times each is selected"
                );
                let distinct: std::collections::HashSet<_> =
                    open_panels.iter().map(|open| open.key.clone()).collect();
                assert_eq!(distinct.len(), 2, "the recorded keys are distinct");
                assert_eq!(
                    dock_area
                        .read(cx)
                        .layout(DockPlacement::Center)
                        .expect("a workspace dock has a centre")
                        .panels()
                        .count(),
                    2,
                    "the dock holds one panel per distinct kind"
                );
            })
            .unwrap();
        cx.run_until_parked();
    }

    /// Closing a panel frees its key, so re-selecting the kind afterwards opens
    /// a fresh panel instead of focusing a dock id the area no longer holds.
    /// The dock has no id-keyed removal, so the centre is emptied the way the
    /// app's own "last panel closed" path empties it.
    #[gpui_kit::test]
    async fn a_closed_kind_is_opened_again_rather_than_focused(cx: &mut TestAppContext) {
        let window = connected_window(cx, "kind-dev").await;
        cx.run_until_parked();

        window
            .update(cx, |main_window, window, cx| {
                let dock_area = match &main_window.mode {
                    WindowMode::Workspace { dock_area, .. } => dock_area.clone(),
                    _ => panic!("a connected window is in workspace mode"),
                };
                dock_area.update(cx, |area, cx| {
                    area.set_center(DockLayout::tabs(), window, cx);
                });
                main_window.forget_closed_panels(&dock_area, cx);

                let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                    unreachable!()
                };
                assert!(open_panels.is_empty(), "the closed panel was forgotten");

                main_window.open_target(NavTarget::pods(), window, cx);
                let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                    unreachable!()
                };
                assert_eq!(open_panels.len(), 1, "so the kind opens again");
            })
            .unwrap();
        cx.run_until_parked();
    }

    #[test]
    fn restorable_panels_skips_unknown_kinds() {
        let layout = WindowLayout {
            panels: vec![
                PanelDescriptor::Unknown,
                PanelDescriptor::Pods {
                    cluster_context: "kind-dev".into(),
                    namespace: crate::config::workspace::NamespaceScope::All,
                    filter: String::new(),
                    sort: crate::config::workspace::SortState {
                        column: "name".into(),
                        ascending: true,
                    },
                },
                PanelDescriptor::Unknown,
            ],
            ..Default::default()
        };

        let kept = restorable_panels(&layout);

        assert_eq!(kept.len(), 1);
        assert!(matches!(kept[0], PanelDescriptor::Pods { .. }));
    }

    /// Section 10.1-10.3, checked on every panel type the dock holds rather than
    /// on the title-bar helpers alone: each panel's tab names its kind, a
    /// namespace picker is on the bar exactly when the kind is namespaced, and
    /// the close control the dock needs is on every one of them.
    ///
    /// A cluster-scoped kind is in the list on purpose - it is the case where
    /// the picker must be *absent*, which a test over namespaced kinds alone
    /// could not catch.
    #[gpui_kit::test]
    async fn every_resource_panel_carries_its_title_bar(cx: &mut TestAppContext) {
        let window = connected_window(cx, "kind-dev").await;
        cx.run_until_parked();

        let expected = 4;
        let cases: [(NavTarget, bool); 4] = [
            (NavTarget::pods(), true),
            (NavTarget::Logs, true),
            (NavTarget::Kind(crd_kind()), true),
            (NavTarget::Kind(cluster_scoped_kind()), false),
        ];

        let mut checked: Vec<String> = Vec::new();
        window
            .update(cx, |main_window, window, cx| {
                let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                for (target, namespaced) in cases {
                    let scope = PanelScope::new(target.clone(), "kind-dev".to_string());
                    let (_id, opened) =
                        dock_area.update(cx, |area, cx| nav::add_panel(area, &scope, window, cx));
                    let (name, picker, controls) = match opened {
                        nav::OpenedPanel::Pods(panel) => title_bar_of(&panel, window, cx),
                        nav::OpenedPanel::Placeholder(panel) => title_bar_of(&panel, window, cx),
                        nav::OpenedPanel::Logs(panel) => title_bar_of(&panel, window, cx),
                    };
                    assert_eq!(
                        name.as_deref(),
                        Some(target.label()).as_deref(),
                        "the title bar names the kind, and adds the cluster only \
                         when the window holds more than one connection"
                    );
                    assert_eq!(
                        picker, namespaced,
                        "a namespace picker belongs on a namespaced kind and on \
                         nothing else"
                    );
                    assert!(
                        controls > 0,
                        "every resource panel needs its close control, found \
                         none on {}",
                        target.label()
                    );
                    checked.push(target.label());
                }
            })
            .unwrap();

        assert_eq!(
            checked.len(),
            expected,
            "every panel type was checked: {checked:?}"
        );
    }

    /// Section 10.2's second half: a panel that reports a narrower namespace is
    /// re-filed under it.
    ///
    /// This is the body the title-bar subscription runs, driven directly: the
    /// dock hands out panel ids rather than panel entities, so there is no way
    /// from a test to pick a namespace in the rendered menu and watch the
    /// event arrive. What it pins down is the rule the subscription exists for
    /// - the panel stops being filed under the scope it no longer shows.
    #[gpui_kit::test]
    async fn a_narrowed_namespace_rekeys_the_open_panel(cx: &mut TestAppContext) {
        let window = connected_window(cx, "kind-dev").await;
        cx.run_until_parked();

        window
            .update(cx, |main_window, _window, cx| {
                let (id, before) = match &main_window.mode {
                    WindowMode::Workspace { open_panels, .. } => {
                        let open = &open_panels[0];
                        (open.id, open.key.clone())
                    }
                    _ => panic!("a connected window is in workspace mode"),
                };
                assert!(before.namespaces.is_empty(), "it starts on all namespaces");

                main_window.rescope(id, vec!["staging".to_string(), "default".to_string()], cx);

                let after = match &main_window.mode {
                    WindowMode::Workspace { open_panels, .. } => open_panels[0].key.clone(),
                    _ => unreachable!("rescope did not leave workspace mode"),
                };
                assert_ne!(
                    before, after,
                    "the panel must stop being filed under the scope it dropped"
                );
                assert_eq!(
                    after.namespaces,
                    ["staging", "default"],
                    "and be filed under the namespaces it now shows"
                );
                assert_eq!(after.target, before.target, "only the namespace moved");
                assert_eq!(
                    after.context_name, before.context_name,
                    "the cluster is still the cluster"
                );
            })
            .unwrap();
    }

    /// Section 4.4: emptying a workspace's center dock (what closing its last panel
    /// leaves behind) flips the window back to `Picker` mode - `watch_workspace`'s
    /// `DockEvent::LayoutChanged` subscription, driven directly here via `set_center`
    /// with an empty layout rather than a real interactive panel close.
    #[gpui_kit::test]
    async fn closing_the_last_panel_returns_to_the_picker(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });

        let window = cx.add_window(|window, cx| {
            // Goes through the same transition a real connect does, so this
            // covers the Resource panel being wired up as well as the dock.
            let mut main_window = MainWindow {
                mode: WindowMode::Picker(
                    cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
                ),
                focus_handle: cx.focus_handle(),
            };
            main_window.enter_workspace("kind-dev".to_string(), window, cx);
            main_window
        });

        window
            .update(cx, |main_window, _window, _cx| {
                assert!(matches!(main_window.mode, WindowMode::Workspace { .. }));
            })
            .unwrap();

        window
            .update(cx, |main_window, window, cx| {
                let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                    unreachable!("just asserted Workspace mode above");
                };
                dock_area.update(cx, |area, cx| {
                    area.set_center(DockLayout::tabs(), window, cx);
                });
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |main_window, _window, _cx| {
                assert!(matches!(main_window.mode, WindowMode::Picker(_)));
            })
            .unwrap();
    }

    #[gpui_kit::test]
    async fn quitting_persists_open_window_geometry(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let path = temp_workspace_path();
        let keymap_path = temp_workspace_path();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            init(cx, path.clone(), &keymap_path);
            open_window(
                cx,
                WindowLayout {
                    width: 900.0,
                    height: 700.0,
                    x: Some(10.0),
                    y: Some(20.0),
                    panels: Vec::new(),
                },
            );
        });
        cx.run_until_parked();

        cx.update(|cx| save(cx, &path));

        let saved: WorkspaceConfig = config::load(&path);
        assert_eq!(saved.windows.len(), 1);
        assert_eq!(saved.windows[0].width, 900.0);
        assert_eq!(saved.windows[0].height, 700.0);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&keymap_path);
    }

    #[gpui_kit::test]
    async fn save_persists_geometry_of_a_window_already_closed(cx: &mut TestAppContext) {
        // Regression test: under `QuitMode::LastWindowClosed`, `on_app_quit`
        // fires after every window is already gone, so `cx.windows()` alone
        // (the pre-fix implementation) sees nothing and silently saves an
        // empty layout. `save` must also pick up geometry captured by
        // `open_window`'s `on_window_should_close` hook and stashed in
        // `ClosedWindowLayouts` before the window disappeared.
        let path = temp_workspace_path();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(ClosedWindowLayouts(HashMap::from([(
                WindowId::from(1),
                WindowLayout {
                    width: 900.0,
                    height: 700.0,
                    x: Some(10.0),
                    y: Some(20.0),
                    panels: Vec::new(),
                },
            )])));
            save(cx, &path);
        });

        let saved: WorkspaceConfig = config::load(&path);
        assert_eq!(saved.windows.len(), 1);
        assert_eq!(saved.windows[0].width, 900.0);
        assert_eq!(saved.windows[0].height, 700.0);

        let _ = std::fs::remove_file(&path);
    }

    #[gpui_kit::test]
    async fn corrupt_workspace_file_yields_one_default_window(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let path = temp_workspace_path();
        std::fs::write(&path, "not valid toml {{{").unwrap();

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            open_saved_or_default(cx, &path);
        });
        cx.run_until_parked();

        let window_count = cx.update(|cx| cx.windows().len());
        assert_eq!(window_count, 1);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "not valid toml {{{"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[gpui_kit::test]
    async fn toggle_command_palette_action_opens_a_dialog(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            let mut registry = CommandRegistry::new();
            register_commands(&mut registry);
            cx.set_global(registry);
            open_window(cx, WindowLayout::default());
        });
        cx.run_until_parked();

        let window = cx.update(|cx| cx.windows()[0]);

        // Leak-safe: with no dialog open, `render_dialog_layer` returns
        // `None` before touching any dialog state.
        let dialog_open_before = window
            .update(cx, |_, window, cx| {
                gpui_kit::component::Root::render_dialog_layer(window, cx).is_some()
            })
            .unwrap();
        assert!(!dialog_open_before);

        window
            .update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(ToggleCommandPalette), cx);
            })
            .unwrap();
        cx.run_until_parked();

        // Not re-checked via `render_dialog_layer` here: actually rendering
        // gpui-component's `Command` widget installs a model that outlives
        // `close_all_dialogs`/`remove_window` and trips the test harness's
        // leaked-entity check - reproduced directly against gpui-component
        // 0.6.6, not something under our control. `open_command_palette`
        // reaching this point without panicking, immediately after the
        // action dispatch above, is what's covered instead.

        // Close the dialog before the test ends, or the leak detector flags
        // its CommandState entity: the harness asserts every entity created
        // during a test is released by teardown.
        window
            .update(cx, |_, window, cx| {
                let Some(Some(root)) = window.root::<gpui_kit::component::Root>() else {
                    return;
                };
                root.update(cx, |root, cx| root.close_all_dialogs(window, cx));
            })
            .unwrap();
        window
            .update(cx, |_, window, _cx| window.remove_window())
            .unwrap();
        cx.run_until_parked();
    }
}
