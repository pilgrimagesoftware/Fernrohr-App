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

/// The app's first (and, today, only) menu bar entry - just enough to reach the
/// Tunnels window without a connected cluster window open, per design.md decision 5.
/// Grows as later changes give the app more to put in a menu.
fn app_menus() -> Vec<Menu> {
    vec![Menu::new("Fernrohr").items(vec![MenuItem::action(
        "Manage Tunnels…",
        ui::tunnels::TunnelsManage,
    )])]
}

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
            cx.set_menus(app_menus());
            util::shell::open_saved_or_default(cx, &workspace_path);
        });
}
