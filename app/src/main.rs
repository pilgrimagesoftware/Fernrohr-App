use gpui_kit::*;

mod cluster;
mod command;
mod config;
mod consts;
mod forward_registry;
mod forward_supervisor;
mod k8s_port_forward;
mod keychain;
mod keymap;
mod logs;
mod managed_forward;
mod nav;
mod paths;
mod pidfile;
mod placeholder;
mod pods;
mod port_allocator;
mod resource_index;
mod runtime;
mod shell;
mod ssh_path;
mod ssh_tunnel;
mod theme;
mod tunnel_secrets;
mod tunnel_store;
mod ui;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            runtime::init(cx);
            let ui_config: config::ui::UiConfig =
                config::load(&paths::preference_dir().join("ui.toml"));
            theme::init(ui_config.theme, cx);
            let workspace_path = shell::default_workspace_path();
            let keymap_path = paths::preference_dir().join("keymap.toml");
            shell::init(cx, workspace_path.clone(), &keymap_path);
            shell::open_saved_or_default(cx, &workspace_path);
        });
}
