use gpui_kit::*;

mod assets;
mod cli;
mod command;
mod config;
mod consts;
mod forward;
mod k8s;
mod keymap;
mod mcp;
mod notify;
mod runtime;
mod ssh_path;
mod tunnel;
mod ui;
mod util;

fn main() {
    // Help, version, unrecognized arguments and `fernrohr mcp` (the stdio MCP
    // adapter an agent's client launches) all end here, before anything below
    // can sweep the running app's tunnels, write the app log or open a window
    // (`cli-help-and-shutdown` D2). Only no arguments start the app.
    match cli::parse(std::env::args_os().skip(1)) {
        cli::Invocation::StartApp => {}
        cli::Invocation::Mcp => std::process::exit(mcp::run_command()),
        cli::Invocation::Print {
            text,
            to_stderr,
            code,
        } => {
            // Write errors (a closed pipe, as in `fernrohr --help | head -1`)
            // don't change the exit code.
            // `process::exit` runs no destructors, so flush before it.
            let mut out: Box<dyn std::io::Write> = if to_stderr {
                Box::new(std::io::stderr())
            } else {
                Box::new(std::io::stdout())
            };
            let _ = out.write_all(text.as_bytes()).and_then(|()| out.flush());
            std::process::exit(code);
        }
    }

    // Crash recovery: reap any `ssh -N -L` forward a previous run left running after
    // being killed or crashing before its own `Drop` could tear it down. Runs before
    // anything else in `main` so it always happens before any tunnel could possibly
    // be acquired. See `util::pidfile`'s module docs for the quit-time half of this.
    util::pidfile::sweep_stale();

    // Not under `cfg(test)`: `main` is never the test entry point. Every other
    // invocation returned above, so only the real app writes the log.
    util::app_log::install();
    log::info!("Fernrohr {} starting", env!("CARGO_PKG_VERSION"));

    let app = gpui_kit::application()
        .with_assets(assets::AppAssets)
        // Explicit on macOS, where closing the last window leaves the app in
        // the menu bar (quiet mode, #236); with the last window elsewhere.
        .with_quit_mode(util::shell::quit_mode());
    // The Dock icon clicked with no window open brings one back - the one that
    // closed last, as launch restores it.
    app.on_reopen(|cx| {
        util::shell::reopen(cx);
    });
    app.run(|cx: &mut App| {
        gpui_kit::init(cx);
        runtime::init(cx);
        let ui_path = util::paths::preference_dir().join("ui.toml");
        let ui_config: config::ui::UiConfig = config::load(&ui_path);
        ui::theme::init(ui_config.theme, cx);
        ui::theme::save_to(ui_path.clone(), cx);
        ui::resource_panel::init_side_preference(ui_config.resource_side, ui_path.clone(), cx);
        k8s::resource::pod_detail::window_preference::init(
            ui_config.pod_events_window,
            ui_path.clone(),
            cx,
        );
        ui::shortcut_timeout::init(ui_config.shortcut_timeout_secs, ui_path.clone(), cx);
        ui::logs_panels::init(ui_config.logs_panels, ui_path.clone(), cx);
        ui::background_mcp::init(ui_config.background_mcp, ui_path.clone(), cx);
        ui::text_size::init(ui_config.text_size, ui_path, cx);
        ui::namespace_sets::store::NamespaceSets::init(
            util::paths::preference_dir().join("namespace-sets.toml"),
            cx,
        );
        ui::exec_library::store::ExecLibraryStore::init(
            util::paths::preference_dir().join(consts::EXEC_LIBRARY_FILE),
            cx,
        );
        let workspace_path = util::shell::default_workspace_path();
        let keymap_path = util::paths::preference_dir().join("keymap.toml");
        util::shell::init(cx, workspace_path.clone(), &keymap_path);
        util::shell::open_saved_or_default(cx, &workspace_path);
        mcp::start(cx);
        util::shell::init_quiet(util::shell::QuietHooks::app(), cx);
        // Shown from launch: quiet mode's way back once the windows are gone.
        #[cfg(target_os = "macos")]
        ui::status_menu::macos::install(cx);
    });
}
