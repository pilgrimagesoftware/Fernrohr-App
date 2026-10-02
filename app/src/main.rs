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
    // Crash recovery: reap any `ssh -N -L` forward a previous run left running after
    // being killed or crashing before its own `Drop` could tear it down. Runs before
    // anything else in `main` so it always happens before any tunnel could possibly
    // be acquired. See `util::pidfile`'s module docs for the quit-time half of this.
    util::pidfile::sweep_stale();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            runtime::init(cx);
            let ui_path = util::paths::preference_dir().join("ui.toml");
            let ui_config: config::ui::UiConfig = config::load(&ui_path);
            ui::theme::init(ui_config.theme, cx);
            ui::text_size::init(ui_config.text_size, ui_path, cx);
            let workspace_path = util::shell::default_workspace_path();
            let keymap_path = util::paths::preference_dir().join("keymap.toml");
            util::shell::init(cx, workspace_path.clone(), &keymap_path);
            util::shell::open_saved_or_default(cx, &workspace_path);
        });
}
