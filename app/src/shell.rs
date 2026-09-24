use crate::config::{
    self,
    workspace::{PanelDescriptor, WindowLayout, WorkspaceConfig},
};
use crate::panel::PlaceholderPanel;
use crate::paths;
use gpui_kit::component::Root;
use gpui_kit::component::dock::{DockArea, DockLayout, panel_handle};
use gpui_kit::*;
use std::path::{Path, PathBuf};

actions!(shell, [NewWindow]);

pub fn default_workspace_path() -> PathBuf {
    paths::state_dir().join("workspace.toml")
}

/// Binds the "New Window" action and arranges for the workspace to be
/// persisted at `workspace_path` when the app is about to quit (see
/// [`save`]).
pub fn init(cx: &mut App, workspace_path: PathBuf) {
    cx.bind_keys([KeyBinding::new("cmd-n", NewWindow, None)]);
    cx.on_action(|_: &NewWindow, cx: &mut App| {
        open_window(cx, WindowLayout::default());
    });
    cx.on_app_quit(move |cx| {
        save(cx, &workspace_path);
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

/// Opens one window with a two-panel split workspace. The real (Pods, logs)
/// panel kinds land in later changes; today every window shows the same
/// placeholder split so the dock's add/split/resize/close mechanics and the
/// window persistence path both have something concrete to exercise.
pub fn open_window(cx: &mut App, layout: WindowLayout) {
    let bounds = window_bounds(&layout, cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        |window, cx| {
            let left = cx.new(|cx| PlaceholderPanel::new("Panel 1", cx));
            let right = cx.new(|cx| PlaceholderPanel::new("Panel 2", cx));
            let dock_area = cx.new(|cx| DockArea::new("main", Some(1), window, cx));
            dock_area.update(cx, |area, cx| {
                area.set_center(
                    DockLayout::h_split()
                        .child(DockLayout::tabs().panel_view(panel_handle(left), cx), None)
                        .child(DockLayout::tabs().panel_view(panel_handle(right), cx), None),
                    window,
                    cx,
                );
            });
            let view = cx.new(|_| MainWindow { dock_area });
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
    .expect("failed to open window");
}

pub struct MainWindow {
    dock_area: Entity<DockArea>,
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.dock_area.clone())
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

/// Snapshots every open window's geometry into `workspace_path`. Panel
/// descriptors are left empty until a later change adds panel kinds worth
/// restoring (see [`open_window`]'s placeholder split).
pub fn save(cx: &mut App, workspace_path: &Path) {
    let windows: Vec<WindowLayout> = cx
        .windows()
        .into_iter()
        .filter_map(|handle| {
            handle
                .update(cx, |_, window, _cx| {
                    let bounds = window.bounds();
                    WindowLayout {
                        width: f32::from(bounds.size.width),
                        height: f32::from(bounds.size.height),
                        x: Some(f32::from(bounds.origin.x)),
                        y: Some(f32::from(bounds.origin.y)),
                        panels: Vec::new(),
                    }
                })
                .ok()
        })
        .collect();
    let _ = config::save(workspace_path, &WorkspaceConfig { windows });
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: `gpui_kit::*` re-exports its own `test` attribute
    // macro, which would shadow `core::prelude::v1::test` for the plain
    // synchronous test below.
    use super::{
        PanelDescriptor, WindowLayout, WorkspaceConfig, config, init, open_saved_or_default,
        open_window, restorable_panels, save,
    };
    use gpui_kit::TestAppContext;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_workspace_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("fernrohr-shell-test-{n}.toml"))
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

    #[gpui_kit::test]
    async fn quitting_persists_open_window_geometry(cx: &mut TestAppContext) {
        let path = temp_workspace_path();
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx, path.clone());
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
    }

    #[gpui_kit::test]
    async fn corrupt_workspace_file_yields_one_default_window(cx: &mut TestAppContext) {
        let path = temp_workspace_path();
        std::fs::write(&path, "not valid toml {{{").unwrap();

        cx.update(|cx| {
            gpui_kit::init(cx);
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
}
