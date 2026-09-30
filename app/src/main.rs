use gpui_kit::*;

mod command;
mod config;
mod consts;
mod forward;
mod k8s;
mod keymap;
mod runtime;
mod ssh_path;
mod tunnel;
mod ui;
mod util;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            runtime::init(cx);
            let ui_config: config::ui::UiConfig =
                config::load(&util::paths::preference_dir().join("ui.toml"));
            ui::theme::init(ui_config.theme, cx);
            let workspace_path = util::shell::default_workspace_path();
            let keymap_path = util::paths::preference_dir().join("keymap.toml");
            util::shell::init(cx, workspace_path.clone(), &keymap_path);
            util::shell::open_saved_or_default(cx, &workspace_path);
        });
}
