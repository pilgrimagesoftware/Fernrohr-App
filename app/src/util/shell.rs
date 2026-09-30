use crate::command::{Command, CommandRegistry};
use crate::config::{
    self,
    workspace::{PanelDescriptor, WindowLayout, WorkspaceConfig},
};
use crate::consts::{RESOURCE_PANEL_MAX_WIDTH, RESOURCE_PANEL_MIN_WIDTH, RESOURCE_PANEL_WIDTH};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pod_detail::DetailView;
use crate::k8s::resource::pods::SelectedPod;
use crate::keymap;
use crate::tunnel::store::TunnelStore;
use crate::ui::context_bar::ContextBarView;
use crate::ui::nav::{
    self, NavTarget, OpenedPanel, ShowLogs, ShowPodDetail, ShowPodDetailYaml, ShowPods,
};
use crate::ui::panel_title::{self, PanelScope};
use crate::ui::picker_tunnel;
use crate::ui::tunnels;
use crate::util::context_lifecycle;
use crate::util::paths;
use gpui_kit::component::Root;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    DockArea, DockEvent, DockPlacement, DockSkin, PanelId, PanelInfo, PanelState,
};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
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
        menu: Some(crate::command::MenuSlot::Window),
    });
    registry.register(Command {
        id: TOGGLE_PALETTE_COMMAND_ID,
        title: "Command Palette",
        default_binding: TOGGLE_PALETTE_DEFAULT_BINDING,
        context: None,
        action: Box::new(ToggleCommandPalette),
        menu: Some(crate::command::MenuSlot::View),
    });
    registry.register(Command {
        id: SET_CONTEXT_TUNNEL_COMMAND_ID,
        title: "Set Tunnel for Context",
        default_binding: SET_CONTEXT_TUNNEL_DEFAULT_BINDING,
        context: None,
        action: Box::new(SetContextTunnel),
        menu: Some(crate::command::MenuSlot::Context),
    });
    crate::ui::menu::register_commands(registry);
    nav::register_commands(registry);
    tunnels::register_commands(registry);
    crate::k8s::resource::pods::register_commands(registry);
    crate::k8s::resource::pod_detail::register_commands(registry);
    crate::ui::link::register_commands(registry);
    crate::k8s::resource::object_detail::register_commands(registry);
    crate::ui::resource_panel::register_commands(registry);
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

    // Every registered command, from the registry itself - not a hand-kept list,
    // which is how `tunnels.manage` ended up with a menu item but no key.
    let bindings = keymap::bindings(&registry, &keymap, cx.keyboard_mapper().as_ref());
    cx.bind_keys(bindings);
    cx.bind_keys(crate::ui::resource_panel::panel_bindings());
    cx.on_action(|_: &tunnels::TunnelsManage, cx: &mut App| {
        tunnels::open_or_focus(cx);
    });
    cx.on_action(|_: &NewWindow, cx: &mut App| {
        open_window(cx, WindowLayout::default());
    });
    // `window-context-bar` design.md decision 2: closing a window releases every
    // hold it took, wherever it took them - the registry already knows which
    // contexts a closed window used, so this needs no window-specific state.
    cx.on_window_closed(|cx, window_id| {
        ClusterRegistry::release_window(cx, window_id);
    })
    .detach();

    crate::ui::menu::init(&registry, cx);
    cx.set_global(registry);
    crate::k8s::resource::pods::register_restore(cx);
    crate::k8s::resource::pod_detail::register_restore(cx);
    crate::k8s::resource::object_detail::register_restore(cx);
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
        // GPUI's quit path tears down windows but never runs the `Drop` glue on
        // app-scoped globals - `ClusterRegistry`, and so every live `RegistryHandle`/
        // `SshTunnel`/`SshTransport` it holds - so nothing would otherwise kill this
        // run's own `ssh` forwards before the process exits. Runs synchronously here,
        // not inside the returned future, so it's done before this observer even
        // returns rather than racing GPUI's shutdown timeout. See `util::pidfile`.
        crate::util::pidfile::kill_live_forwards();
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

/// Task 2.2's save side: `contexts` is the live window's own `WindowMode::Workspace`
/// list (empty for a `Picker`-mode window, which has none yet), read by
/// [`workspace_contexts`] just before this is called. `panels` stays empty - turning
/// a window's actual open panels into `PanelDescriptor`s is a later change's job; on
/// restore, `contexts` alone is enough to reconnect every context a window used
/// (the spec's "Multi-context window restored" scenario), even before that job lands.
fn layout_from_bounds(bounds: Bounds<Pixels>, live: LiveWorkspace) -> WindowLayout {
    WindowLayout {
        width: f32::from(bounds.size.width),
        height: f32::from(bounds.size.height),
        x: Some(f32::from(bounds.origin.x)),
        y: Some(f32::from(bounds.origin.y)),
        contexts: live.contexts,
        panels: Vec::new(),
        resource_panel_width: live.resource_width.map(f32::from),
    }
}

/// What a save reads off a live window beyond its geometry.
#[derive(Default)]
struct LiveWorkspace {
    contexts: Vec<String>,
    resource_width: Option<Pixels>,
}

/// The live contexts a window uses, read back through its `Root` - empty for a
/// `Picker`-mode window (nothing to save yet) or one whose `Root`/`MainWindow` can't
/// be found (shouldn't happen for a window this module opened, but geometry alone is
/// still worth saving over failing the whole snapshot).
fn workspace_contexts(window: &mut Window, cx: &mut App) -> LiveWorkspace {
    let Some(Some(root)) = window.root::<Root>() else {
        return LiveWorkspace::default();
    };
    let Ok(main_window) = root.read(cx).view().clone().downcast::<MainWindow>() else {
        return LiveWorkspace::default();
    };
    let main_window = main_window.read(cx);
    LiveWorkspace {
        contexts: main_window.contexts(),
        resource_width: main_window.resource_width(),
    }
}

/// Remembers a closing main window's layout so `save` still writes it after the window
/// is gone (see [`ClosedWindowLayouts`]).
///
/// Only the *last* main window is recorded. A window closed while others stay open
/// is one the user is done with, so it's dropped rather than restored at the next
/// launch. The last one is recorded because closing it quits the app
/// (`QuitMode::LastWindowClosed`) before `save` can read any open window.
fn record_closing_layout(window_id: WindowId, window: &mut Window, cx: &mut App) {
    if other_open_main_windows(window_id, cx) > 0 {
        if cx.has_global::<ClosedWindowLayouts>() {
            cx.global_mut::<ClosedWindowLayouts>().0.remove(&window_id);
        }
        return;
    }
    let bounds = window.bounds();
    let live = workspace_contexts(window, cx);
    let layout = layout_from_bounds(bounds, live);
    if !cx.has_global::<ClosedWindowLayouts>() {
        cx.set_global(ClosedWindowLayouts::default());
    }
    cx.global_mut::<ClosedWindowLayouts>()
        .0
        .insert(window_id, layout);
}

/// How many main (workspace or picker) windows other than `except` are open.
fn other_open_main_windows(except: WindowId, cx: &App) -> usize {
    cx.windows()
        .into_iter()
        .filter(|handle| handle.window_id() != except)
        .filter(|handle| {
            handle
                .downcast::<Root>()
                .and_then(|root| root.read(cx).ok())
                .is_some_and(|root| root.view().clone().downcast::<MainWindow>().is_ok())
        })
        .count()
}

/// Closes `window` the way its close button does. `remove_window` skips the
/// platform's should-close hook, so a main window's layout is recorded here first;
/// other windows (Tunnels, About) have no layout to keep.
pub(crate) fn close_window(window: &mut Window, cx: &mut App) {
    let is_main = window.root::<Root>().flatten().is_some_and(|root| {
        root.read(cx)
            .view()
            .clone()
            .downcast::<MainWindow>()
            .is_ok()
    });
    if is_main {
        record_closing_layout(window.window_handle().window_id(), window, cx);
    }
    window.remove_window();
}

/// How many contexts the window currently uses, read live from its `MainWindow` - 1
/// for a picker-mode window or one this module didn't open. Read, not cached: the
/// count changes on add and disconnect, and dock panels are drawn after
/// `MainWindow`'s own render has returned, so reading it here never re-enters it.
pub(crate) fn window_context_count(window: &mut Window, cx: &App) -> usize {
    let Some(Some(root)) = window.root::<Root>() else {
        return 1;
    };
    let Ok(main_window) = root.read(cx).view().clone().downcast::<MainWindow>() else {
        return 1;
    };
    main_window.read(cx).contexts().len().max(1)
}

/// A saved Resource panel width, clamped to the range its divider allows; the
/// default when the layout has none.
fn restored_resource_width(layout: &WindowLayout) -> Pixels {
    layout
        .resource_panel_width
        .map(px)
        .map(|width| width.clamp(RESOURCE_PANEL_MIN_WIDTH, RESOURCE_PANEL_MAX_WIDTH))
        .unwrap_or(RESOURCE_PANEL_WIDTH)
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

/// Every context a restored (non-empty) layout's workspace should reconnect, in the
/// order the window used them: `layout.contexts` when it says anything, otherwise
/// (`window-context-bar` design.md decision 3, an older file with no `contexts` at
/// all) the distinct `cluster_context`s named by its saved panels, first-seen order.
/// Empty for a layout with no restorable panels - that window opens in `Picker` mode.
fn restored_contexts(layout: &WindowLayout) -> Vec<String> {
    if !layout.contexts.is_empty() {
        return layout.contexts.clone();
    }
    let mut contexts = Vec::new();
    for descriptor in restorable_panels(layout) {
        let context_name = match descriptor {
            PanelDescriptor::Pods {
                cluster_context, ..
            }
            | PanelDescriptor::Logs {
                cluster_context, ..
            } => cluster_context,
            PanelDescriptor::Unknown => continue,
        };
        if !contexts.contains(context_name) {
            contexts.push(context_name.clone());
        }
    }
    contexts
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
        "ObjectDetail" => match crate::k8s::resource::object_detail::target_from_state(data) {
            Some(object) => NavTarget::Object(object),
            None => return keys,
        },
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

/// The cluster context a Logs or Pod-detail panel should scope itself to: the
/// context that published the currently selected pod ([`SelectedPod`]), when
/// `contexts` - this window's own - includes it; [`contexts[active]`](usize)
/// while nothing is selected yet (a bare `nav.show_logs` before any pod has
/// been clicked, or a restored panel with no live selection at all). Every
/// other target is not pod-scoped and always reads `contexts[active]`.
///
/// A selection published by a context this window does not hold is refused
/// rather than opened against `active` instead - that silent substitution
/// (`1-window-context-bar` bug 1) is what streamed a pod selected in one
/// context's Pods panel against a *different* context, turning a real pod
/// into a 404. [`MainWindow::open_target_with_view`] no-ops on `None`, after
/// this has logged why.
fn pod_scoped_context(
    target: &NavTarget,
    contexts: &[String],
    active: usize,
    cx: &App,
) -> Option<String> {
    if !matches!(target, NavTarget::Logs | NavTarget::Pod(_)) {
        return Some(contexts[active].clone());
    }
    match cx
        .try_global::<SelectedPod>()
        .and_then(|selected| selected.0.as_ref())
    {
        None => Some(contexts[active].clone()),
        Some(selection) if contexts.iter().any(|held| held == &selection.context_name) => {
            Some(selection.context_name.clone())
        }
        Some(selection) => {
            log::warn!(
                "selected pod {}/{} belongs to context {:?}, which this window does not hold \
                 ({contexts:?}); not opening {target:?}",
                selection.namespace,
                selection.name,
                selection.context_name,
            );
            None
        }
    }
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
        /// Every context this window uses, in the order they were added
        /// (`window-context-bar` design.md decision 1). A restored window can
        /// already carry more than one; adding a second interactively (the "+"
        /// control, section 3) is still that change's own later sections' job.
        /// `connection_count` (how many cluster connections the window holds,
        /// which section 10.1's title bar reads) is always `contexts.len()`
        /// rather than a field of its own, so the two can never disagree.
        contexts: Vec<String>,
        /// Index into `contexts` naming which one the Resource panel's cluster
        /// dropdown currently shows, and which one a window-wide action (like
        /// `SetContextTunnel`) applies to. Always `0` while `contexts` has one
        /// entry.
        active: usize,
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
        /// `connection-status-bar`: one item per context this window uses, shown along
        /// the workspace's bottom edge. Absent in `Picker` mode - the picker already
        /// shows its own connect progress (proposal.md's non-goals).
        status_bar: Entity<crate::ui::status_bar::StatusBarView>,
        /// `window-context-bar` section 3: one chip per context this window uses,
        /// shown along the workspace's top edge, below the title bar. `contexts` and
        /// `active` above are this bar's source of truth - every edit to either goes
        /// through [`MainWindow::sync_context_children`], which is what keeps the bar,
        /// the status bar, and the Resource panel's dropdown from disagreeing.
        context_bar: Entity<ContextBarView>,
        /// The Resource panel's current width: seeded from the saved layout, updated
        /// on every divider drag, and written back by [`save`].
        resource_width: Pixels,
    },
}

/// Opens one window, in `Picker` mode if `layout` has no restorable contexts, or
/// directly into a connected workspace (seeded from the restored layout's contexts)
/// otherwise.
pub fn open_window(cx: &mut App, layout: WindowLayout) {
    let bounds = window_bounds(&layout, cx);
    let contexts = restored_contexts(&layout);
    let resource_width = restored_resource_width(&layout);
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
                record_closing_layout(window_id, window, cx);
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
                if !contexts.is_empty() {
                    view.enter_workspace(contexts, window, cx);
                    view.set_resource_width(resource_width);
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
            this.enter_workspace(vec![context_name.clone()], window, cx);
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
            // Task 2.2: keyed by every context the window uses (`context_lifecycle::
            // dock_layout_key`), not just the first - a single-context window's key is
            // still its bare context name, so this is the same save it always was for
            // that case, and a new one for a multi-context window (design.md decision 3
            // predates task 2.2's fuller persistence; see that task's own note on why).
            if let WindowMode::Workspace { contexts, .. } = &this.mode
                && cx.has_global::<SavedDockLayouts>()
            {
                let key = context_lifecycle::dock_layout_key(contexts);
                let state = dock_area.read(cx).dump(cx);
                cx.global_mut::<SavedDockLayouts>().0.insert(key, state);
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

/// The "Set tunnel for <context>" chooser: Direct plus every tunnel, the current
/// binding checked. Shared by `context.set_tunnel` in a workspace and in the picker.
fn open_tunnel_dialog(context_name: String, window: &mut Window, cx: &mut App) {
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
    match write_context_tunnel(tunnels_path, context_name, tunnel_id.as_deref()) {
        Ok(()) => crate::ui::tunnels::notify_tunnels_changed(cx),
        Err(error) => log::warn!("failed to set {context_name}'s tunnel binding: {error:?}"),
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

    /// Swaps this window to a connected workspace on `contexts`: the dock (defaulting
    /// to a Pods panel on `contexts[0]`), the Resource panel listing that cluster's
    /// discovered kinds, and the subscription that opens whatever is picked. Both
    /// windows-enter-workspace sites go through here so the Resource panel cannot be
    /// wired up in one of them and forgotten in the other.
    ///
    /// Takes this window's hold on every one of `contexts` (`window-context-bar`
    /// design.md decision 2) before building anything else, so a session already
    /// exists (or is connected here) for every step below to read - including a
    /// context past `contexts[0]`, which a restored multi-context window holds but,
    /// absent the "+" control and its panel (section 3), opens no panel for yet.
    ///
    /// `contexts` must not be empty.
    fn enter_workspace(
        &mut self,
        contexts: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(
            !contexts.is_empty(),
            "a workspace always has at least one context"
        );
        let window_id = window.window_handle().window_id();
        for context_name in &contexts {
            ClusterRegistry::hold(cx, context_name, window_id);
        }
        let context_name = contexts[0].clone();
        // Task 2.2: keyed by every context this window uses - see `watch_workspace`'s
        // matching save-side key.
        let saved_layout = if cx.has_global::<SavedDockLayouts>() {
            cx.global::<SavedDockLayouts>()
                .0
                .get(&context_lifecycle::dock_layout_key(&contexts))
                .cloned()
        } else {
            None
        };
        // One connection per context in `contexts`; see `WindowMode::Workspace::contexts`.
        let connection_count = contexts.len();
        let (dock_area, dock_skin, scope, (first_id, first)) =
            build_workspace(context_name.clone(), connection_count, window, cx);
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
        let resource_panel = cx.new(|cx| {
            crate::ui::resource_panel::ResourcePanel::new(
                context_name.clone(),
                contexts.clone(),
                window,
                cx,
            )
        });
        cx.subscribe_in(
            &resource_panel,
            window,
            |this: &mut MainWindow, _panel, event, window, cx| match event {
                crate::ui::resource_panel::ResourceEvent::Open(target) => {
                    this.open_target(target.clone(), window, cx);
                }
                crate::ui::resource_panel::ResourceEvent::SwitchContext(context_name) => {
                    this.set_active_context(context_name, window, cx);
                }
            },
        )
        .detach();
        let status_bar =
            cx.new(|cx| crate::ui::status_bar::StatusBarView::new(contexts.clone(), cx));
        let main_window_handle = cx.weak_entity();
        let context_bar =
            cx.new(|cx| ContextBarView::new(contexts.clone(), 0, main_window_handle, cx));
        self.mode = WindowMode::Workspace {
            dock_area,
            _dock_skin: dock_skin,
            contexts,
            active: 0,
            resource_panel,
            open_panels,
            nav: Box::new(NavTarget::pods()),
            status_bar,
            context_bar,
            resource_width: RESOURCE_PANEL_WIDTH,
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
            nav::OpenedPanel::PodDetail(_) | nav::OpenedPanel::ObjectDetail(_) => {}
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

    /// Every context this window uses - empty in `Picker` mode. What `save`
    /// (`util/shell.rs`'s module doc comment) persists per window.
    fn resource_width(&self) -> Option<Pixels> {
        match &self.mode {
            WindowMode::Picker(_) => None,
            WindowMode::Workspace { resource_width, .. } => Some(*resource_width),
        }
    }

    fn set_resource_width(&mut self, width: Pixels) {
        if let WindowMode::Workspace { resource_width, .. } = &mut self.mode {
            *resource_width = width;
        }
    }

    fn contexts(&self) -> Vec<String> {
        match &self.mode {
            WindowMode::Picker(_) => Vec::new(),
            WindowMode::Workspace { contexts, .. } => contexts.clone(),
        }
    }

    /// How many of this window's open panels use `context_name` - the count the
    /// Disconnect confirmation (`ui/context_bar.rs`) states before closing them.
    /// `0` in `Picker` mode, or for a context this window doesn't use.
    pub(crate) fn context_panel_count(&self, context_name: &str) -> usize {
        let WindowMode::Workspace { open_panels, .. } = &self.mode else {
            return 0;
        };
        open_panels
            .iter()
            .filter(|open| open.key.context_name == context_name)
            .count()
    }

    /// Pushes `contexts`/`active` to the Resource panel, the status bar, and the
    /// context bar - the one place that updates all three, so `add_context`,
    /// `disconnect_context`, and `set_active_context` cannot update one and forget
    /// another (see `WindowMode::Workspace::context_bar`'s doc comment). A no-op in
    /// `Picker` mode.
    fn sync_context_children(&mut self, cx: &mut Context<Self>) {
        let WindowMode::Workspace {
            contexts,
            active,
            resource_panel,
            status_bar,
            context_bar,
            ..
        } = &self.mode
        else {
            return;
        };
        let contexts_snapshot = contexts.clone();
        let active_index = *active;
        let Some(active_context) = contexts_snapshot.get(active_index).cloned() else {
            return;
        };
        let resource_panel = resource_panel.clone();
        let status_bar = status_bar.clone();
        let context_bar = context_bar.clone();
        // Deferred: a chip click (`ui/context_bar.rs::ContextBarView::
        // on_chip_clicked`) and the Resource panel's own cluster dropdown
        // (`ResourcePanel::cluster_dropdown`'s `cx.emit`) both reach this
        // synchronously from within that very entity's own update - updating it
        // again here, before that update returns, panics ("cannot update T while
        // it is already being updated"). `cx.defer` runs this closure once the
        // current effect cycle finishes and every lease along the way to here has
        // released, which resolves before any `Entity::update`/`WindowHandle::
        // update` call that reached `set_active_context` returns - so callers
        // still observe the synced state immediately afterward, same as before
        // this was deferred.
        cx.defer(move |cx| {
            resource_panel.update(cx, |panel, cx| {
                panel.set_active_context(active_context, contexts_snapshot.clone(), cx);
            });
            status_bar.update(cx, |bar, cx| {
                bar.set_context_names(contexts_snapshot.clone(), cx);
            });
            context_bar.update(cx, |bar, cx| {
                bar.set_state(contexts_snapshot, active_index, cx);
            });
        });
        cx.notify();
    }

    /// `window-context-bar` design.md decision 4: the one place `active` is
    /// written - a chip click and the Resource panel's own cluster dropdown both
    /// land here, which is what keeps the two in sync. A no-op if `context_name`
    /// isn't one this window uses (a stale request racing a disconnect).
    pub(crate) fn set_active_context(
        &mut self,
        context_name: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            contexts, active, ..
        } = &mut self.mode
        else {
            return;
        };
        let Some(index) = contexts.iter().position(|name| name == context_name) else {
            return;
        };
        *active = index;
        self.sync_context_children(cx);
    }

    /// Section 3.2: adds `context_name` to this window, makes it the active
    /// context, and opens its Pods panel. Called only after
    /// `ui/context_bar.rs`'s "+" popover already reports
    /// `PickerEvent::Connected` for it, so by the time this runs the connection
    /// has already succeeded - a context that fails to connect never reaches
    /// this at all, which is what keeps a failed add from opening a panel.
    pub(crate) fn add_context(
        &mut self,
        context_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace { contexts, .. } = &self.mode else {
            return;
        };
        if contexts.contains(&context_name) {
            return;
        }

        let window_id = window.window_handle().window_id();
        ClusterRegistry::hold(cx, &context_name, window_id);

        let WindowMode::Workspace {
            contexts, active, ..
        } = &mut self.mode
        else {
            return;
        };
        contexts.push(context_name);
        *active = contexts.len() - 1;
        self.sync_context_children(cx);
        self.open_target(NavTarget::pods(), window, cx);
    }

    /// Section 3.3: closes every panel in this window that uses `context_name`,
    /// releases this window's hold on it, and removes its chip. Falls back to
    /// the cluster picker when that was the window's last context.
    pub(crate) fn disconnect_context(
        &mut self,
        context_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            dock_area,
            open_panels,
            ..
        } = &self.mode
        else {
            return;
        };
        let dock_area = dock_area.clone();
        let closing: Vec<(PanelId, Option<OpenedPanel>)> = open_panels
            .iter()
            .filter(|open| open.key.context_name == context_name)
            .map(|open| (open.id, open.panel.clone()))
            .collect();

        for (id, opened) in closing {
            let opened = opened.or_else(|| nav::opened_panel_for(dock_area.read(cx), id, cx));
            let Some(opened) = opened else {
                continue;
            };
            dock_area.update(cx, |area, cx| match opened {
                OpenedPanel::Pods(panel) => area.remove_panel(panel, window, cx),
                OpenedPanel::Placeholder(panel) => area.remove_panel(panel, window, cx),
                OpenedPanel::Logs(panel) => area.remove_panel(panel, window, cx),
                OpenedPanel::PodDetail(panel) => area.remove_panel(panel, window, cx),
                OpenedPanel::ObjectDetail(panel) => area.remove_panel(panel, window, cx),
            });
        }

        let WindowMode::Workspace { open_panels, .. } = &mut self.mode else {
            return;
        };
        open_panels.retain(|open| open.key.context_name != context_name);

        let window_id = window.window_handle().window_id();
        ClusterRegistry::release(cx, &context_name, window_id);

        let WindowMode::Workspace {
            contexts, active, ..
        } = &self.mode
        else {
            return;
        };
        let outcome =
            context_lifecycle::contexts_after_disconnect(contexts, *active, &context_name);

        match outcome {
            Some((remaining, new_active)) => {
                let WindowMode::Workspace {
                    contexts, active, ..
                } = &mut self.mode
                else {
                    return;
                };
                *contexts = remaining;
                *active = new_active;
                self.sync_context_children(cx);
            }
            None => self.enter_picker(window, cx),
        }
    }

    fn enter_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx));
        watch_picker(&picker, window, cx);
        self.mode = WindowMode::Picker(picker);
        cx.notify();
    }

    /// `resource.focus`: puts keyboard focus on this window's Resource panel.
    fn on_action_focus_resources(
        &mut self,
        _: &crate::ui::resource_panel::FocusResources,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let WindowMode::Workspace { resource_panel, .. } = &self.mode {
            resource_panel.update(cx, |panel, cx| panel.focus_list(window, cx));
        }
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
        // The window's active context in a workspace; in the picker, the context the
        // user selected there - so the binding can be set from the keyboard before
        // connecting, not only from a row's dropdown.
        let context_name = match &self.mode {
            WindowMode::Workspace {
                contexts, active, ..
            } => contexts[*active].clone(),
            WindowMode::Picker(picker) => match picker.read(cx).selected_context() {
                Some(context_name) => context_name,
                None => return,
            },
        };
        open_tunnel_dialog(context_name, window, cx);
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
        self.open_target_in(target, initial_view, None, Vec::new(), window, cx);
    }

    /// `open_target_with_view` with the scope spelled out: `context_name` is
    /// the cluster context to open the panel against (`None` for the window's
    /// own choice, as every menu, palette and row request makes), and
    /// `namespaces` the namespace scope to give it.
    ///
    /// A followed reference (`resource-links` 3.1) names its context
    /// explicitly - the context of the panel it was shown in, which in a
    /// multi-context window need not be the active one. A context this window
    /// doesn't hold is refused rather than substituted, for the same reason
    /// `pod_scoped_context` refuses a foreign selection.
    pub(crate) fn open_target_in(
        &mut self,
        target: NavTarget,
        initial_view: Option<DetailView>,
        context_name: Option<String>,
        namespaces: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let WindowMode::Workspace {
            dock_area,
            contexts,
            active,
            resource_panel,
            open_panels,
            nav,
            ..
        } = &mut self.mode
        else {
            return;
        };
        let connection_count = contexts.len();
        let context_name = match context_name {
            Some(named) if contexts.contains(&named) => named,
            Some(named) => {
                log::warn!(
                    "not opening {target:?}: context {named:?} is not held by this window \
                     ({contexts:?})"
                );
                return;
            }
            None => match pod_scoped_context(&target, contexts.as_slice(), *active, cx) {
                Some(context_name) => context_name,
                None => return,
            },
        };
        // Set only when a panel was actually built, so the subscription below
        // is not made for a panel the dock already had.
        let mut watch_scope = None;
        let scope = PanelScope {
            connection_count,
            ..PanelScope::new(target.clone(), context_name)
        }
        .scoped_to(namespaces);
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

    /// Test-only: a bare `Picker`-mode window, for tests elsewhere in the crate
    /// that only need a real `WeakEntity<MainWindow>` to satisfy a constructor
    /// (`ui/context_bar.rs::ContextBarView::new`, which stores one but never reads
    /// it outside a click handler) - `mode` and `focus_handle` above have no
    /// visibility modifier, so nothing outside this module can build a
    /// `MainWindow` literal directly.
    #[cfg(test)]
    pub(crate) fn test_picker_window(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Test-only: a window already in `Workspace` mode on `contexts`, for tests
    /// that need a chip click or a Resource panel dropdown pick to actually reach
    /// [`Self::set_active_context`] and its downstream `sync_context_children`.
    /// Not `util/shell/tests.rs`'s own `connected_window`: that helper drives a
    /// real `ClusterConnection::connect`, which this one's callers don't need.
    /// Callers must pre-seed every context's `ClusterRegistry` session first
    /// (`insert_test_session`), so `enter_workspace`'s `hold` reuses it instead of
    /// starting a real connect.
    #[cfg(test)]
    pub(crate) fn test_workspace(
        contexts: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::test_picker_window(window, cx);
        this.enter_workspace(contexts, window, cx);
        this
    }

    /// Test-only readback of which context is active - `active` itself has no
    /// getter since production code only ever needs to write it (through
    /// [`Self::set_active_context`]).
    #[cfg(test)]
    pub(crate) fn test_active_context_name(&self) -> Option<String> {
        match &self.mode {
            WindowMode::Workspace {
                contexts, active, ..
            } => contexts.get(*active).cloned(),
            WindowMode::Picker(_) => None,
        }
    }

    /// Test-only access to the Resource panel, so a test can drive its cluster
    /// dropdown's `SwitchContext` exactly as a click does.
    #[cfg(test)]
    pub(crate) fn test_resource_panel(
        &self,
    ) -> Option<Entity<crate::ui::resource_panel::ResourcePanel>> {
        match &self.mode {
            WindowMode::Workspace { resource_panel, .. } => Some(resource_panel.clone()),
            WindowMode::Picker(_) => None,
        }
    }

    /// Test-only access to the embedded context bar, so a test can assert the
    /// *real* bar [`Self::sync_context_children`] pushes into - not a second,
    /// disconnected `ContextBarView` built only for the test - reflects an
    /// active-context change.
    #[cfg(test)]
    pub(crate) fn test_context_bar(&self) -> Option<Entity<ContextBarView>> {
        match &self.mode {
            WindowMode::Workspace { context_bar, .. } => Some(context_bar.clone()),
            WindowMode::Picker(_) => None,
        }
    }
}

/// Opens the command palette in a dialog on `window`'s `Root`. A fresh
/// `CommandState` is created per open (not reused across opens) since the
/// palette's own query/selection state should reset each time it's summoned.
fn open_command_palette(window: &mut Window, cx: &mut App) {
    crate::util::palette::open(window, cx);
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body: AnyElement = match &self.mode {
            WindowMode::Picker(picker) => picker.clone().into_any_element(),
            // The Resource panel is the window's left edge. It used to be a
            // fixed Pods/Logs list built here; the kinds now come from the
            // cluster's own discovery (spec 8.1), so it owns its own chrome.
            // `connection-status-bar`/`window-context-bar`: the workspace is a
            // column - the context bar fixed to its own height at the top, the
            // existing panel row at `flex_1`, then the status bar fixed to its
            // own height below it. The picker has neither bar (it shows its own
            // connect progress instead, and has no context list to chip yet).
            WindowMode::Workspace {
                dock_area,
                resource_panel,
                status_bar,
                context_bar,
                resource_width,
                ..
            } => div()
                .size_full()
                .flex()
                .flex_col()
                .child(context_bar.clone())
                .child(
                    // The Resource panel is a resizable split, not a fixed-width
                    // column: drag the divider to trade list width for dock space.
                    div().flex_1().min_h_0().child(
                        h_resizable("workspace-split")
                            .on_resize({
                                let this = cx.weak_entity();
                                move |state, _window, cx| {
                                    let Some(width) = state.read(cx).sizes().first().copied()
                                    else {
                                        return;
                                    };
                                    let _ = this.update(cx, |this, _cx| {
                                        this.set_resource_width(width);
                                    });
                                }
                            })
                            .child(
                                resizable_panel()
                                    .size(*resource_width)
                                    .size_range(RESOURCE_PANEL_MIN_WIDTH..RESOURCE_PANEL_MAX_WIDTH)
                                    .child(resource_panel.clone()),
                            )
                            .child(resizable_panel().child(dock_area.clone().into_any_element())),
                    ),
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
            .on_action(cx.listener(Self::on_action_focus_resources))
            .on_action(cx.listener(Self::on_action_show_logs))
            .on_action(cx.listener(Self::on_action_show_pod_detail))
            .on_action(cx.listener(Self::on_action_show_pod_detail_yaml))
            .on_action(cx.listener(Self::on_action_set_tunnel))
            .on_action(cx.listener(Self::on_action_follow_reference))
            .child(body)
            // gpui-component's `Root` only records open dialogs, sheets and
            // notifications; the window's own view has to draw them. Without these
            // layers `Root::open_dialog` (the palette, "+", Disconnect,
            // `context.set_tunnel`) opens nothing visible.
            .children(gpui_kit::component::Root::render_sheet_layer(window, cx))
            .children(gpui_kit::component::Root::render_dialog_layer(window, cx))
            .children(gpui_kit::component::Root::render_notification_layer(
                window, cx,
            ))
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
        if let Ok(layout) = handle.update(cx, |_, window, cx| {
            let bounds = window.bounds();
            let live = workspace_contexts(window, cx);
            layout_from_bounds(bounds, live)
        }) {
            layouts.insert(handle.window_id(), layout);
        }
    }
    let windows: Vec<WindowLayout> = layouts.into_values().collect();
    let _ = config::save(workspace_path, &WorkspaceConfig { windows });
}

// A sibling `tests.rs` rather than an inline module, per rust-structure.md's
// file-size cap: this module's own production code is already well past 700
// lines on its own (a pre-existing condition `window-context-bar`'s own brief
// says not to grow further "more than wiring"), and inlining the test module on
// top of it made the file harder to navigate than the split costs. `tests.rs`
// uses named imports rather than `use super::*` for the same reason `ui/status_
// bar.rs`'s sibling does: that glob re-imports `gpui_kit::*`'s huge surface a
// second time and blows this toolchain's macro-expansion budget alongside a
// `#[gpui_kit::test]` item.
mod follow;
#[cfg(test)]
mod tests;
