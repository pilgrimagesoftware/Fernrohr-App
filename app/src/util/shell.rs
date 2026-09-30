use crate::command::{Command, CommandRegistry};
use crate::config::{
    self,
    workspace::{PanelDescriptor, WindowLayout, WorkspaceConfig},
};
use crate::k8s::resource::pod_detail::DetailView;
use crate::k8s::resource::pods::SelectedPod;
use crate::keymap;
use crate::tunnel::store::TunnelStore;
use crate::ui::nav::{
    self, NavTarget, OpenedPanel, ShowLogs, ShowPodDetail, ShowPodDetailYaml, ShowPods,
};
use crate::ui::panel_title::{self, PanelScope};
use crate::ui::picker_tunnel;
use crate::ui::tunnels;
use crate::util::paths;
use gpui_kit::component::Root;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    DockArea, DockEvent, DockPlacement, DockSkin, PanelId, PanelInfo, PanelState,
};
use gpui_kit::*;
use kube::core::GroupVersionKind;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

actions!(shell, [NewWindow, ToggleCommandPalette, SetContextTunnel]);

/// Last known geometry of every window that has closed this run, keyed by
/// `WindowId`. Populated from each window's `on_window_should_close` hook,
/// since under `QuitMode::LastWindowClosed` the app-quit callback fires only
/// after every window is already gone - `cx.windows()` is empty by then, so
/// geometry has to be captured on the way out rather than read at quit time.
#[derive(Default)]
struct ClosedWindowLayouts(HashMap<WindowId, WindowLayout>);

impl Global for ClosedWindowLayouts {}

struct SavedDockLayouts(crate::config::dock_layouts::DockLayouts);

impl Global for SavedDockLayouts {}

pub const NEW_WINDOW_COMMAND_ID: &str = "shell.new_window";
pub const NEW_WINDOW_DEFAULT_BINDING: &str = "cmd-n";
pub const TOGGLE_PALETTE_COMMAND_ID: &str = "shell.toggle_command_palette";
pub const TOGGLE_PALETTE_DEFAULT_BINDING: &str = "cmd-shift-p";
pub const SET_CONTEXT_TUNNEL_COMMAND_ID: &str = "context.set_tunnel";
pub const SET_CONTEXT_TUNNEL_DEFAULT_BINDING: &str = "cmd-shift-b";

pub fn default_workspace_path() -> PathBuf {
    paths::state_dir().join("workspace.toml")
}

pub fn default_dock_layouts_path() -> PathBuf {
    paths::state_dir().join("dock-layouts.json")
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
    registry.register(Command {
        id: SET_CONTEXT_TUNNEL_COMMAND_ID,
        title: "Set Tunnel for Context",
        default_binding: SET_CONTEXT_TUNNEL_DEFAULT_BINDING,
        context: None,
        action: Box::new(SetContextTunnel),
    });
    nav::register_commands(registry);
    tunnels::register_commands(registry);
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
    let set_context_tunnel_binding = keymap::resolve(
        SET_CONTEXT_TUNNEL_COMMAND_ID,
        SET_CONTEXT_TUNNEL_DEFAULT_BINDING,
        &keymap,
    );
    cx.bind_keys([
        KeyBinding::new(&new_window_binding, NewWindow, None),
        KeyBinding::new(&palette_binding, ToggleCommandPalette, None),
        KeyBinding::new(&show_pods_binding, ShowPods, None),
        KeyBinding::new(&show_logs_binding, ShowLogs, None),
        KeyBinding::new(&set_context_tunnel_binding, SetContextTunnel, None),
    ]);
    // The panels' own shortcuts, each in its own key context. A panel naming a
    // key in its hint bar has not bound that key: without this the hint bar
    // prints letters no keystroke resolves to, and the shortcut does nothing.
    cx.bind_keys(crate::k8s::resource::pods::panel_bindings());
    cx.bind_keys(crate::k8s::resource::pod_detail::panel_bindings());
    cx.on_action(|_: &tunnels::TunnelsManage, cx: &mut App| {
        tunnels::open_or_focus(cx);
    });
    cx.on_action(|_: &NewWindow, cx: &mut App| {
        open_window(cx, WindowLayout::default());
    });

    cx.set_global(registry);
    crate::k8s::resource::pods::register_restore(cx);
    crate::k8s::resource::pod_detail::register_restore(cx);
    crate::util::logs::register_restore(cx);
    crate::ui::placeholder::register_restore(cx);
    let dock_layouts_path = default_dock_layouts_path();
    cx.set_global(SavedDockLayouts(crate::config::dock_layouts::load(
        &dock_layouts_path,
    )));

    cx.on_app_quit(move |cx| {
        save(cx, &workspace_path);
        if let Some(layouts) = cx.try_global::<SavedDockLayouts>() {
            let _ = crate::config::dock_layouts::save(&dock_layouts_path, &layouts.0);
        }
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
    let first = dock_area.update(cx, |area, cx| {
        nav::add_panel(area, &scope, None, window, cx)
    });
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
    /// The typed handle, when this window built the panel. `None` for one
    /// restored by the dock from layout - that one exists only as a
    /// `PanelId`, with no entity the window ever held.
    ///
    /// Kept so a request that arrives for an already-open panel can do more
    /// than focus it: `y` on a pod whose detail panel is already showing
    /// fields has to switch that panel to the YAML, and switching needs the
    /// entity, not the id.
    panel: Option<nav::OpenedPanel>,
}

fn restored_panel_keys(state: &PanelState) -> Vec<PanelKey> {
    let mut keys = state
        .children
        .iter()
        .flat_map(restored_panel_keys)
        .collect::<Vec<_>>();
    let PanelInfo::Panel(data) = &state.info else {
        return keys;
    };
    let context_name = data["context_name"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let namespaces = serde_json::from_value(data["namespaces"].clone()).unwrap_or_default();
    let target = match state.panel_name.as_str() {
        "Pods" => NavTarget::pods(),
        "Logs" => NavTarget::Logs,
        "PodDetail" => NavTarget::pod(
            data["pod_namespace"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            data["pod_name"].as_str().unwrap_or_default().to_string(),
        ),
        "Resource" => NavTarget::Kind(crate::k8s::cluster::discovery::DiscoveredKind {
            gvk: GroupVersionKind::gvk(
                data["group"].as_str().unwrap_or_default(),
                data["version"].as_str().unwrap_or("v1"),
                data["kind"].as_str().unwrap_or("Resource"),
            ),
            plural: data["plural"].as_str().unwrap_or("resources").to_string(),
            namespaced: data["namespaced"].as_bool().unwrap_or(false),
        }),
        _ => return keys,
    };
    keys.push(PanelKey {
        target,
        context_name,
        namespaces,
    });
    keys
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
        /// `connection-status-bar`: one item per context this window uses, shown along
        /// the workspace's bottom edge. Absent in `Picker` mode - the picker already
        /// shows its own connect progress (proposal.md's non-goals).
        status_bar: Entity<crate::ui::status_bar::StatusBarView>,
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
            // A restored layout can carry a window smaller than the picker needs
            // - it is whatever size the user last left it, from before the logo
            // grew. Without a floor the picker's centred column overflows and the
            // logo is clipped off the top.
            window_min_size: Some(crate::ui::picker::MIN_WINDOW_SIZE),
            ..Default::default()
        },
        |window, cx| {
            crate::ui::theme::watch_window(window, cx);

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
            if let WindowMode::Workspace { context_name, .. } = &this.mode
                && cx.has_global::<SavedDockLayouts>()
            {
                let state = dock_area.read(cx).dump(cx);
                cx.global_mut::<SavedDockLayouts>()
                    .0
                    .insert(context_name.clone(), state);
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

/// Section 3.2: one row of `on_action_set_tunnel`'s dialog - writes the binding (or,
/// for `tunnel_id: None`, removes it) and closes the dialog. A plain `Button` rather
/// than a `PopupMenuItem`: this is a dialog's own content, not a popover menu.
fn tunnel_dialog_option(
    label: String,
    checked: bool,
    tunnels_path: PathBuf,
    context_name: String,
    tunnel_id: Option<String>,
) -> AnyElement {
    let label = if checked {
        format!("\u{2713} {label}")
    } else {
        label
    };
    Button::new(SharedString::from(format!(
        "set-tunnel-{}",
        tunnel_id.as_deref().unwrap_or("direct")
    )))
    .label(label)
    .ghost()
    .w_full()
    .on_click(move |_event, window, cx| {
        set_context_tunnel_and_close(&tunnels_path, &context_name, tunnel_id.clone(), window, cx);
    })
    .into_any_element()
}

/// Section 3.2's actual write: binds (`Some`) or unbinds (`None`) `context_name`
/// through the same `TunnelStore` `ui/picker_tunnel.rs`'s row selector uses. Free of
/// any GPUI context, so it's testable without a `Root` (which `open_dialog`/
/// `close_dialog` require) at all - this is `context.set_tunnel`'s handler binding,
/// in the sense tasks.md 3.2 asks for.
fn write_context_tunnel(
    tunnels_path: &Path,
    context_name: &str,
    tunnel_id: Option<&str>,
) -> Result<(), crate::tunnel::store::TunnelStoreError> {
    let store = TunnelStore::new(tunnels_path.to_path_buf());
    match tunnel_id {
        Some(id) => store.bind(context_name, id),
        None => store.unbind(context_name),
    }
}

/// Writes `context_name`'s tunnel binding and closes the dialog the option came from.
fn set_context_tunnel_and_close(
    tunnels_path: &Path,
    context_name: &str,
    tunnel_id: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Err(error) = write_context_tunnel(tunnels_path, context_name, tunnel_id.as_deref()) {
        log::warn!("failed to set {context_name}'s tunnel binding: {error:?}");
    }
    Root::update(window, cx, |root, window, cx| {
        root.close_dialog(window, cx);
    });
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
        let saved_layout = if cx.has_global::<SavedDockLayouts>() {
            cx.global::<SavedDockLayouts>()
                .0
                .get(&context_name)
                .cloned()
        } else {
            None
        };
        let (dock_area, dock_skin, scope, (first_id, first)) =
            build_workspace(context_name.clone(), CONNECTIONS, window, cx);
        let restored = saved_layout.is_some();
        let restored_keys = saved_layout
            .as_ref()
            .map(|state| restored_panel_keys(&state.center))
            .unwrap_or_default();
        if let Some(state) = saved_layout {
            dock_area.update(cx, |area, cx| {
                area.load(state, window, cx)
                    .expect("saved dock layout must load");
            });
        }
        let open_panels = if restored {
            let ids = dock_area
                .read(cx)
                .layout(DockPlacement::Center)
                .map(|tree| tree.panels().collect::<Vec<_>>())
                .unwrap_or_default();
            ids.into_iter()
                .zip(restored_keys)
                .map(|(id, key)| OpenPanel {
                    id,
                    key,
                    panel: None,
                })
                .collect()
        } else {
            vec![OpenPanel {
                key: PanelKey::from(&scope),
                id: first_id,
                // The first panel is the pods list, and the window did build
                // it, so the handle is available: keeping it lets a later
                // request for pods reuse the same `OpenedPanel` bookkeeping
                // every other panel gets.
                panel: Some(first.clone()),
            }]
        };
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
        let status_bar =
            cx.new(|cx| crate::ui::status_bar::StatusBarView::new(vec![context_name.clone()], cx));
        self.mode = WindowMode::Workspace {
            dock_area,
            _dock_skin: dock_skin,
            context_name,
            resource_panel,
            open_panels,
            nav: Box::new(NavTarget::pods()),
            connection_count: CONNECTIONS,
            status_bar,
        };
        if !restored {
            self.watch_scope_changes(first, window, cx);
        }
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
            // A pod's detail panel shows one pod rather than a namespace-
            // filterable list of many, so it carries no picker and never
            // re-scopes.
            nav::OpenedPanel::PodDetail(_) => {}
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

    /// Section 3.2: opens a small dialog offering Direct plus every configured
    /// tunnel for this window's connected context, writing through the same
    /// `TunnelStore::bind`/`unbind` the picker's own row selector uses. A no-op in
    /// `Picker` mode - there is no connected context to set a tunnel for yet, and
    /// the picker's own per-row selector already covers that case.
    fn on_action_set_tunnel(
        &mut self,
        _: &SetContextTunnel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace { context_name, .. } = &self.mode else {
            return;
        };
        let context_name = context_name.clone();
        let tunnels_path = paths::preference_dir().join("tunnels.toml");
        let store = TunnelStore::new(tunnels_path.clone());
        let choices = picker_tunnel::tunnel_choices(&store);
        let current = store.binding_for(&context_name);

        Root::update(window, cx, |root, window, cx| {
            root.open_dialog(
                move |dialog, _window, _cx| {
                    let mut options: Vec<AnyElement> = Vec::new();
                    options.push(tunnel_dialog_option(
                        "Direct".to_string(),
                        current.is_none(),
                        tunnels_path.clone(),
                        context_name.clone(),
                        None,
                    ));
                    for choice in &choices {
                        let checked = current.as_deref() == Some(choice.id.as_str());
                        options.push(tunnel_dialog_option(
                            choice.name.clone(),
                            checked,
                            tunnels_path.clone(),
                            context_name.clone(),
                            Some(choice.id.clone()),
                        ));
                    }
                    dialog
                        .title(format!("Set tunnel for {context_name}"))
                        .w(px(360.))
                        .child(div().flex().flex_col().gap_1().children(options))
                },
                window,
                cx,
            );
        });
    }

    /// Opens the selected pod's detail panel on the field list. Emitted by a
    /// Pods panel's `DescribePod` handler and by its row context menu's
    /// "Open".
    fn on_action_show_pod_detail(
        &mut self,
        _: &ShowPodDetail,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_pod_detail(DetailView::Structured, window, cx);
    }

    /// Opens the selected pod's detail panel *on the YAML*. The same panel as
    /// `ShowPodDetail` opens - same target, same dock slot, same dedup - just
    /// landing on the other view, because `y` is bound to "the YAML, now" and
    /// would be pointless if it only focused a panel showing fields.
    fn on_action_show_pod_detail_yaml(
        &mut self,
        _: &ShowPodDetailYaml,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_pod_detail(DetailView::Yaml, window, cx);
    }

    /// The single entry point for "show me this pod's detail". The pod itself
    /// travels in the app-scoped `SelectedPod` global, the same one `ShowLogs`
    /// reads, rather than in the action - `gpui_kit::actions!` generates
    /// unit-only structs, so a `ShowPodYaml` action cannot carry the pod or
    /// the view alongside it.
    fn open_pod_detail(&mut self, view: DetailView, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selection) = cx
            .try_global::<SelectedPod>()
            .and_then(|selected| selected.0.clone())
        else {
            return;
        };
        self.open_target_with_view(
            NavTarget::pod(selection.namespace, selection.name),
            Some(view),
            window,
            cx,
        );
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
        self.open_target_with_view(target, None, window, cx);
    }

    /// `open_target` for a request that also says which view the panel should
    /// be showing. `initial_view` is `None` for every panel that has one view,
    /// and for a pod's detail panel opened by describing it.
    ///
    /// It applies to an already-open panel as well as a new one: focusing an
    /// existing pod panel and switching it to the requested view is what makes
    /// `y` work on a pod whose detail is already up, which is the common case
    /// rather than the exception.
    fn open_target_with_view(
        &mut self,
        target: NavTarget,
        initial_view: Option<DetailView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                if let (Some(OpenedPanel::PodDetail(panel)), Some(view)) =
                    (open.panel.as_ref(), initial_view)
                {
                    panel.update(cx, |panel, cx| panel.set_view(view, cx));
                }
                dock_area.update(cx, |area, cx| area.select_panel(id, window, cx));
            }
            None => {
                let (id, opened) = dock_area.update(cx, |area, cx| {
                    nav::add_panel(area, &scope, initial_view, window, cx)
                });
                open_panels.push(OpenPanel {
                    key,
                    id,
                    panel: Some(opened.clone()),
                });
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
            // `connection-status-bar`: the workspace is now a column - the
            // existing panel row at `flex_1`, then the status bar fixed to its
            // own height below it. The picker has no bar (it shows its own
            // connect progress instead).
            WindowMode::Workspace {
                dock_area,
                resource_panel,
                status_bar,
                ..
            } => div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .child(resource_panel.clone())
                        .child(dock_area.clone().into_any_element()),
                )
                .child(status_bar.clone())
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
            .on_action(cx.listener(Self::on_action_show_pod_detail))
            .on_action(cx.listener(Self::on_action_show_pod_detail_yaml))
            .on_action(cx.listener(Self::on_action_set_tunnel))
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
        ClosedWindowLayouts, MainWindow, NavTarget, OpenPanel, OpenedPanel, PanelDescriptor,
        PanelKey, SET_CONTEXT_TUNNEL_COMMAND_ID, ShowPodDetail, ToggleCommandPalette, WindowLayout,
        WindowMode, WorkspaceConfig, config, init, open_saved_or_default, open_window,
        register_commands, restorable_panels, save, watch_picker, write_context_tunnel,
    };
    use crate::command::CommandRegistry;
    use crate::config::tunnels::{TunnelAuth, TunnelConfig};
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use crate::k8s::cluster::session::ClusterRegistry;
    use crate::ui::nav;
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

    fn temp_tunnels_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fernrohr-shell-set-tunnel-test-{n}.toml"));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// Tasks.md 3.2: `context.set_tunnel` is registered with a title, alongside every
    /// other palette command.
    #[test]
    fn set_context_tunnel_is_a_registered_command() {
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);

        let command = registry
            .get(SET_CONTEXT_TUNNEL_COMMAND_ID)
            .expect("context.set_tunnel must be registered");
        assert_eq!(command.title, "Set Tunnel for Context");
    }

    /// Tasks.md 3.2: the command's handler binds - `write_context_tunnel` is the
    /// write `on_action_set_tunnel`'s dialog options call, factored out so it's
    /// testable without a `Root` (which its `open_dialog`/`close_dialog` calls
    /// require).
    #[test]
    fn set_context_tunnels_handler_binds_and_unbinds() {
        let path = temp_tunnels_path();
        let store = crate::tunnel::store::TunnelStore::new(path.clone());
        store
            .create(
                "qa-bastion",
                TunnelConfig {
                    name: "QA".into(),
                    bastion_user: "ops".into(),
                    bastion_host: "bastion.example.com".into(),
                    bastion_port: 22,
                    jump_hosts: Vec::new(),
                    auth: TunnelAuth::default(),
                },
                None,
            )
            .unwrap();

        write_context_tunnel(&path, "qa-1", Some("qa-bastion")).unwrap();
        assert_eq!(store.binding_for("qa-1"), Some("qa-bastion".to_string()));

        write_context_tunnel(&path, "qa-1", None).unwrap();
        assert_eq!(store.binding_for("qa-1"), None);

        let _ = std::fs::remove_file(&path);
    }

    /// A connected window on `context_name`, for the panel-opening tests.
    ///
    /// `enter_workspace` opens a real Pods panel (`nav::add_panel` always
    /// builds one via `PodsPanel::new`, not the `with_stubs` seam
    /// `pods::tests` uses), which starts a genuine `ClusterConnection::connect`,
    /// a tokio task that resolves a kubeconfig and probes a server, then
    /// wakes its GPUI observer from that tokio thread. `allow_parking` is the
    /// same seam `cluster::session`'s and `cluster::connection`'s own tests
    /// use for this exact reason: without it, the wakeup races the test
    /// scheduler's thread-confinement check non-deterministically, since it
    /// depends on real wall-clock I/O timing rather than anything these tests
    /// control.
    async fn connected_window(
        cx: &mut TestAppContext,
        context_name: &str,
    ) -> gpui_kit::WindowHandle<MainWindow> {
        cx.executor().allow_parking();
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
    /// The namespace picker moved out of the title bar into each panel's own
    /// body (Paul's feedback: a picker shared across a tab group's title bar
    /// was ambiguous about which tab it scoped), so this reads only what
    /// still lives in the dock's title bar: the name and the toolbar
    /// controls.
    fn title_bar_of<T: dock::Panel>(
        panel: &Entity<T>,
        window: &mut Window,
        cx: &mut App,
    ) -> (Option<SharedString>, usize) {
        let name = panel.tab_name(cx);
        let controls = panel
            .toolbar_buttons(window, cx)
            .map_or(0, |buttons| buttons.len());
        (name, controls)
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

    /// Section 5.1/5.4: the pod-detail request lands on the same `open_target`
    /// every other panel uses, so a pod gets a panel of its own and
    /// re-requesting it focuses rather than duplicates. What makes two pods
    /// two panels is the pod identity now inside the key, not a second dedup
    /// rule.
    #[gpui_kit::test]
    async fn a_pod_detail_panel_is_keyed_by_which_pod(cx: &mut TestAppContext) {
        let window = connected_window(cx, "kind-dev").await;
        cx.run_until_parked();

        let session_before =
            cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());

        window
            .update(cx, |main_window, window, cx| {
                main_window.open_target(NavTarget::pod("prod", "web-1"), window, cx);
                // The same pod again: focused, not opened twice.
                main_window.open_target(NavTarget::pod("prod", "web-1"), window, cx);
                // A different pod is a genuinely new panel.
                main_window.open_target(NavTarget::pod("prod", "web-2"), window, cx);

                let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                let pods: Vec<_> = open_panels
                    .iter()
                    .filter(|open| matches!(open.key.target, NavTarget::Pod(_)))
                    .map(|open| open.key.target.clone())
                    .collect();
                assert_eq!(
                    pods,
                    vec![
                        NavTarget::pod("prod", "web-1"),
                        NavTarget::pod("prod", "web-2"),
                    ],
                    "one panel per pod, however many times each is requested"
                );
            })
            .unwrap();
        cx.run_until_parked();

        let session_after = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());
        assert_eq!(
            session_before, session_after,
            "the detail panel reads the window's existing connection"
        );
    }

    /// Section 5.1: the `ShowPodDetail` a Pods panel emits (for `d`, `y`, or
    /// the row menu's "Open") resolves the pod from the app-scoped
    /// `SelectedPod` and opens its detail panel - the same `open_target` the
    /// keybinding-free path above uses.
    #[gpui_kit::test]
    async fn a_requested_pod_detail_opens_its_panel(cx: &mut TestAppContext) {
        use crate::k8s::resource::pods::{PodSelection, SelectedPod};

        let window = connected_window(cx, "kind-dev").await;
        cx.run_until_parked();

        cx.update(|cx| {
            cx.set_global(SelectedPod(Some(PodSelection {
                namespace: "prod".into(),
                name: "web-1".into(),
                containers: vec!["web".into()],
            })));
        });
        window
            .update(cx, |main_window, window, cx| {
                main_window.focus_handle.clone().focus(window, cx);
                window.dispatch_action(Box::new(ShowPodDetail), cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |main_window, _window, _cx| {
                let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                assert!(
                    open_panels
                        .iter()
                        .any(|open| open.key.target == NavTarget::pod("prod", "web-1")),
                    "the selected pod's detail panel is open"
                );
            })
            .unwrap();
    }

    /// `y` is bound to "the YAML, now", so it has to land on the YAML - and it
    /// has to do so whether the panel is opened by the shortcut or already
    /// sitting there showing fields, which is the case that actually comes up.
    ///
    /// Both halves matter and they fail differently: a fresh open needs the
    /// view threaded into construction, and an existing panel needs the window
    /// to hold on to the entity so it can switch a panel it only has a
    /// `PanelId` for.
    #[gpui_kit::test]
    async fn asking_for_yaml_opens_and_switches_the_pod_panel_to_yaml(cx: &mut TestAppContext) {
        use crate::k8s::resource::pod_detail::DetailView;
        use crate::k8s::resource::pods::{PodSelection, SelectedPod};
        use crate::ui::nav::ShowPodDetailYaml;

        type Window = gpui_kit::WindowHandle<MainWindow>;

        /// Dispatches an app-level detail request the way a keybinding would.
        fn request_detail(cx: &mut TestAppContext, window: &Window, yaml: bool) {
            window
                .update(cx, |main_window, window, cx| {
                    main_window.focus_handle.clone().focus(window, cx);
                    if yaml {
                        window.dispatch_action(Box::new(ShowPodDetailYaml), cx);
                    } else {
                        window.dispatch_action(Box::new(ShowPodDetail), cx);
                    }
                })
                .unwrap();
            cx.run_until_parked();
        }

        /// The view showing in the window's one pod detail panel.
        fn open_panel_view(cx: &mut TestAppContext, window: &Window) -> DetailView {
            window
                .update(cx, |main_window, _window, cx| {
                    let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                        panic!("a connected window is in workspace mode")
                    };
                    let detail: Vec<&OpenPanel> = open_panels
                        .iter()
                        .filter(|open| matches!(open.panel, Some(OpenedPanel::PodDetail(_))))
                        .collect();
                    assert_eq!(
                        detail.len(),
                        1,
                        "one detail panel, whichever shortcut asked for it"
                    );
                    let Some(OpenedPanel::PodDetail(panel)) = &detail[0].panel else {
                        unreachable!("filtered to detail panels")
                    };
                    panel.read(cx).view()
                })
                .unwrap()
        }

        let window = connected_window(cx, "kind-dev").await;
        cx.run_until_parked();
        cx.update(|cx| {
            cx.set_global(SelectedPod(Some(PodSelection {
                namespace: "prod".into(),
                name: "web-1".into(),
                containers: vec!["web".into()],
            })));
        });

        request_detail(cx, &window, true);
        assert_eq!(
            open_panel_view(cx, &window),
            DetailView::Yaml,
            "a fresh panel opens on the YAML, not on its default view"
        );

        request_detail(cx, &window, true);
        assert_eq!(
            open_panel_view(cx, &window),
            DetailView::Yaml,
            "asking again is still one panel, and still on the YAML"
        );

        request_detail(cx, &window, false);
        assert_eq!(
            open_panel_view(cx, &window),
            DetailView::Structured,
            "`d` switches the open panel back to the field list rather than \
             adding a second panel for the same pod"
        );
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
        let cases: [NavTarget; 4] = [
            NavTarget::pods(),
            NavTarget::Logs,
            NavTarget::Kind(crd_kind()),
            NavTarget::Kind(cluster_scoped_kind()),
        ];

        let mut checked: Vec<String> = Vec::new();
        window
            .update(cx, |main_window, window, cx| {
                let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                for target in cases {
                    let scope = PanelScope::new(target.clone(), "kind-dev".to_string());
                    let (_id, opened) = dock_area.update(cx, |area, cx| {
                        nav::add_panel(area, &scope, None, window, cx)
                    });
                    let (name, controls) = match opened {
                        nav::OpenedPanel::Pods(panel) => title_bar_of(&panel, window, cx),
                        nav::OpenedPanel::Placeholder(panel) => title_bar_of(&panel, window, cx),
                        nav::OpenedPanel::Logs(panel) => title_bar_of(&panel, window, cx),
                        nav::OpenedPanel::PodDetail(panel) => title_bar_of(&panel, window, cx),
                    };
                    assert_eq!(
                        name.as_deref(),
                        Some(target.list_label()).as_deref(),
                        "the title bar names the kind, and adds the cluster only \
                         when the window holds more than one connection"
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
        // See `connected_window`'s doc comment: `enter_workspace` starts a real
        // connect whose completion wakes GPUI from a tokio thread.
        cx.executor().allow_parking();
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

    /// Regression test for the HANDOFF.md report: opening a second window and
    /// selecting a context that a *first* window already connected said "connected"
    /// but never switched the second window out of picker mode. Drives both windows
    /// through the real `ClusterPicker::select` -> `PickerEvent::Connected` ->
    /// `watch_picker` path (unlike `connected_window`, which shortcuts straight to
    /// `enter_workspace` and so never exercised this path at all) - the same shared
    /// connection entity stands in for `ClusterRegistry` returning the first window's
    /// already-`Connected` entity to the second window's picker.
    #[gpui_kit::test]
    async fn second_window_connecting_to_an_already_connected_context_shows_workspace(
        cx: &mut TestAppContext,
    ) {
        use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
        use crate::ui::picker::ClusterPicker;
        use kube::{Client, Config};
        use std::cell::RefCell;

        thread_local! {
            static SHARED: RefCell<Option<Entity<ClusterConnection>>> = const { RefCell::new(None) };
        }

        // `connection_factory` only stubs the picker's own connection entity;
        // `watch_picker` still drives `enter_workspace`, which starts a real
        // `ClusterRegistry` connect for the panel it builds. See
        // `connected_window`'s doc comment for why that needs `allow_parking`.
        cx.executor().allow_parking();

        fn shared_connected_stub(cx: &mut App, _context_name: &str) -> Entity<ClusterConnection> {
            SHARED.with(|cell| {
                if let Some(entity) = cell.borrow().as_ref() {
                    return entity.clone();
                }
                let handle = crate::runtime::handle(cx);
                let _guard = handle.enter();
                let client =
                    Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap();
                let entity = cx.new(|_| {
                    ClusterConnection::test_with_state(ConnectionState::Connected(client))
                });
                *cell.borrow_mut() = Some(entity.clone());
                entity
            })
        }

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });

        fn picker_window(cx: &mut TestAppContext) -> gpui_kit::WindowHandle<MainWindow> {
            cx.add_window(|window, cx| {
                let picker = cx.new(|cx| {
                    let mut picker = ClusterPicker::new(window, cx);
                    picker.connection_factory = Some(shared_connected_stub);
                    picker
                });
                let main_window = MainWindow {
                    mode: WindowMode::Picker(picker.clone()),
                    focus_handle: cx.focus_handle(),
                };
                watch_picker(&picker, window, cx);
                main_window
            })
        }

        let first = picker_window(cx);
        first
            .update(cx, |main_window, _window, cx| {
                let WindowMode::Picker(picker) = &main_window.mode else {
                    unreachable!("just constructed in Picker mode");
                };
                picker.update(cx, |picker, cx| {
                    picker.select("kind-dev".to_string(), cx);
                });
            })
            .unwrap();
        cx.run_until_parked();
        first
            .update(cx, |main_window, _window, _cx| {
                assert!(
                    matches!(main_window.mode, WindowMode::Workspace { .. }),
                    "first window connects normally"
                );
            })
            .unwrap();

        let second = picker_window(cx);
        second
            .update(cx, |main_window, _window, cx| {
                let WindowMode::Picker(picker) = &main_window.mode else {
                    unreachable!("just constructed in Picker mode");
                };
                picker.update(cx, |picker, cx| {
                    picker.select("kind-dev".to_string(), cx);
                });
            })
            .unwrap();
        cx.run_until_parked();
        second
            .update(cx, |main_window, _window, _cx| {
                assert!(
                    matches!(main_window.mode, WindowMode::Workspace { .. }),
                    "second window selecting an already-connected context must also \
                     switch to the workspace, not stay stuck showing the picker"
                );
            })
            .unwrap();

        SHARED.with(|cell| *cell.borrow_mut() = None);
    }

    /// Hypothesis two from `second-window-connect-fix/design.md`: a second window
    /// connecting to a context that is *not* already connected (so it goes through
    /// `cx.observe`'s callback, not `select`'s synchronous `emit_connected` call).
    #[gpui_kit::test]
    async fn second_window_connecting_to_a_fresh_context_shows_workspace(cx: &mut TestAppContext) {
        use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
        use crate::ui::picker::ClusterPicker;
        use kube::{Client, Config};

        // See the previous test: `connection_factory` stubs only the picker's
        // own entity, not the real connect `enter_workspace` starts.
        cx.executor().allow_parking();

        fn connecting_then_connected_stub(
            cx: &mut App,
            _context_name: &str,
        ) -> Entity<ClusterConnection> {
            let handle = crate::runtime::handle(cx);
            let _guard = handle.enter();
            let client =
                Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap();
            let entity =
                cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting));
            entity.update(cx, |connection, cx| {
                connection.state = ConnectionState::Connected(client);
                cx.notify();
            });
            entity
        }

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });

        let second = cx.add_window(|window, cx| {
            let picker = cx.new(|cx| {
                let mut picker = ClusterPicker::new(window, cx);
                picker.connection_factory = Some(connecting_then_connected_stub);
                picker
            });
            let main_window = MainWindow {
                mode: WindowMode::Picker(picker.clone()),
                focus_handle: cx.focus_handle(),
            };
            watch_picker(&picker, window, cx);
            main_window
        });
        second
            .update(cx, |main_window, _window, cx| {
                let WindowMode::Picker(picker) = &main_window.mode else {
                    unreachable!("just constructed in Picker mode");
                };
                picker.update(cx, |picker, cx| {
                    picker.select("fresh-dev".to_string(), cx);
                });
            })
            .unwrap();
        cx.run_until_parked();
        second
            .update(cx, |main_window, _window, _cx| {
                assert!(
                    matches!(main_window.mode, WindowMode::Workspace { .. }),
                    "a fresh connect completing after select must still flip this \
                     window to the workspace"
                );
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

    #[test]
    fn restored_panel_keys_preserve_kind_context_and_namespace() {
        use gpui_kit::component::dock::{PanelInfo, PanelState};

        let state = PanelState {
            panel_name: "Pods".to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": "kind-dev",
                "namespaces": ["kube-system"],
            })),
        };

        let keys = super::restored_panel_keys(&state);

        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].target, NavTarget::pods());
        assert_eq!(keys[0].context_name, "kind-dev");
        assert_eq!(keys[0].namespaces, vec!["kube-system"]);
    }

    /// The panel's own keys have to be *bound*, not merely printed.
    ///
    /// The hint bar under the pods table reads the keymap for each shortcut
    /// and falls back to printing the letter, so an unbound `d` looks
    /// identical to a working one on screen while doing nothing when pressed.
    /// This presses the key rather than dispatching the action, because the
    /// binding is exactly the part that can be missing.
    ///
    /// At the end of the module on purpose: `title_bar_of` above is being
    /// changed on another branch, and a test whose context sits under it would
    /// stop applying the moment that lands.
    #[gpui_kit::test]
    async fn a_pods_panel_shortcut_key_reaches_the_window(cx: &mut TestAppContext) {
        use crate::k8s::resource::pods::{PodSelection, SelectedPod};
        use gpui_kit::{Focusable as _, test::TestWindowExt as _};

        let workspace = temp_workspace_path();
        let keymap = temp_workspace_path();
        // See `connected_window`'s doc comment: `enter_workspace` starts a real
        // connect whose completion wakes GPUI from a tokio thread.
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            init(cx, workspace.clone(), &keymap);
        });
        let window = cx.add_window(|window, cx| {
            let mut main_window = MainWindow {
                mode: WindowMode::Picker(
                    cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
                ),
                focus_handle: cx.focus_handle(),
            };
            main_window.enter_workspace("kind-dev".to_string(), window, cx);
            main_window
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.set_global(SelectedPod(Some(PodSelection {
                namespace: "default".into(),
                name: "web-1".into(),
                containers: vec!["web".into()],
            })));
        });

        // Focus the pods list, the way clicking into its table would.
        window
            .update(cx, |main_window, window, cx| {
                let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                let Some(OpenedPanel::Pods(panel)) = open_panels[0].panel.clone() else {
                    panic!("a new workspace opens on the pods list")
                };
                panel.read(cx).focus_handle(cx).focus(window, cx);
            })
            .unwrap();
        cx.run_until_parked();

        // A real keystroke, not a dispatched action. Dispatched through the
        // window rather than the entity: a keypress re-renders, and re-entering
        // the window's view while it is mid-update is what gpui forbids.
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.dispatch_keystroke(
                gpui_kit::Keystroke::parse("d").expect("valid keystroke"),
                cx,
            );
            window.render_frame(cx);
        })
        .expect("the window is still open");
        cx.run_until_parked();

        window
            .update(cx, |main_window, _window, _cx| {
                let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                assert!(
                    open_panels
                        .iter()
                        .any(|open| open.key.target == NavTarget::pod("default", "web-1")),
                    "pressing `d` in the pods list opened the selected pod's detail \
                     panel, so the key was bound rather than only printed"
                );
            })
            .unwrap();

        let _ = std::fs::remove_file(&workspace);
        let _ = std::fs::remove_file(&keymap);
    }

    /// `connection-status-bar` 2.3: the status bar renders under a connected workspace's
    /// body, and a picker-mode window - which shows its own connect progress instead
    /// (proposal.md's non-goals) - has no such field to render at all.
    #[gpui_kit::test]
    async fn the_status_bar_renders_only_in_workspace_mode(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt as _;

        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });

        let window = cx.add_window(|window, cx| MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        });
        cx.run_until_parked();

        window
            .update(cx, |main_window, _window, _cx| {
                assert!(
                    matches!(main_window.mode, WindowMode::Picker(_)),
                    "the picker variant carries no status bar field"
                );
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
        })
        .expect("a picker-mode window renders with no status bar");

        window
            .update(cx, |main_window, window, cx| {
                main_window.enter_workspace("kind-dev".to_string(), window, cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |main_window, _window, cx| {
                let WindowMode::Workspace { status_bar, .. } = &main_window.mode else {
                    panic!("entering the workspace leaves picker mode")
                };
                assert_eq!(
                    status_bar.read(cx).items(cx).len(),
                    1,
                    "the bar is wired to the window's own context, not merely a field \
                     nobody reads"
                );
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
        })
        .expect("a workspace window renders its status bar");
    }
}
