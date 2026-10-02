//! The shell's actions and commands, and app start-up: registering every command, binding the keymap, installing the menus and restore hooks.

use super::*;

actions!(shell, [NewWindow, ToggleCommandPalette, SetContextTunnel]);

/// Last known geometry of every window that has closed this run, keyed by
/// `WindowId`. Populated from each window's `on_window_should_close` hook,
/// since under `QuitMode::LastWindowClosed` the app-quit callback fires only
/// after every window is already gone - `cx.windows()` is empty by then, so
/// geometry has to be captured on the way out rather than read at quit time.
#[derive(Default)]
pub(super) struct ClosedWindowLayouts(pub(super) HashMap<WindowId, WindowLayout>);

impl Global for ClosedWindowLayouts {}

/// Where [`init`] was told to persist the workspace, for saves that happen
/// before quit (see [`super::persist::schedule_save`]).
pub(super) struct WorkspacePath(pub(super) PathBuf);

impl Global for WorkspacePath {}

/// The save [`super::persist::schedule_save`] has pending. Replacing it drops,
/// and so cancels, the previous one - that is the debounce.
pub(super) struct PendingSave {
    /// Held only so dropping it cancels the save.
    pub(super) _task: Task<()>,
}

impl Global for PendingSave {}

pub(super) struct SavedDockLayouts(pub(super) crate::config::dock_layouts::DockLayouts);

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
    crate::ui::panel::focus::register_commands(registry);
    crate::ui::panel::tabs::register_commands(registry);
    crate::ui::settings::register_commands(registry);
    crate::ui::text_size::register_commands(registry);
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
    // Kept for the keybindings editor, which edits it live (`keymap::apply`).
    cx.set_global(keymap::LiveKeymap::new(keymap_path.to_path_buf(), keymap));
    cx.bind_keys(crate::ui::resource_panel::panel_bindings());
    crate::ui::settings::init(cx);
    crate::ui::text_size::register_handlers(cx);
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

    cx.set_global(WorkspacePath(workspace_path.clone()));
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

#[cfg(test)]
mod tests;
