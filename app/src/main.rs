use gpui_kit::*;

mod cluster;
mod command;
mod config;
mod keychain;
mod keymap;
mod logs;
mod paths;
mod pods;
mod resource_index;
mod runtime;
mod shell;
mod ssh_path;

fn main() {
    gpui_kit::application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            runtime::init(cx);
            let workspace_path = shell::default_workspace_path();
            let keymap_path = paths::preference_dir().join("keymap.toml");
            shell::init(cx, workspace_path.clone(), &keymap_path);
            shell::open_saved_or_default(cx, &workspace_path);
        });
}
