use gpui_kit::*;

mod cluster;
mod config;
mod logs;
mod panel;
mod paths;
mod pods;
mod resource_index;
mod runtime;
mod shell;

fn main() {
    gpui_kit::application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            runtime::init(cx);
            let workspace_path = shell::default_workspace_path();
            shell::init(cx, workspace_path.clone());
            shell::open_saved_or_default(cx, &workspace_path);
        });
}
