use gpui_kit::component::Root;
use gpui_kit::component::dock::{DockArea, DockLayout, Panel, PanelEvent, panel_handle};
use gpui_kit::*;

mod config;
mod paths;
mod resource_index;
mod runtime;

struct WelcomePanel {
    focus_handle: FocusHandle,
}

impl WelcomePanel {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Focusable for WelcomePanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for WelcomePanel {}

impl Render for WelcomePanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child("Fernrohr")
    }
}

impl gpui_kit::component::dock::BasePanel for WelcomePanel {
    fn panel_name(&self) -> &'static str {
        "Welcome"
    }
}

impl Panel for WelcomePanel {}

struct MainWindow {
    dock_area: Entity<DockArea>,
}

impl MainWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let welcome = cx.new(WelcomePanel::new);
        let dock_area = cx.new(|cx| DockArea::new("main", Some(1), window, cx));
        dock_area.update(cx, |area, cx| {
            area.set_center(
                DockLayout::tabs().panel_view(panel_handle(welcome), cx),
                window,
                cx,
            );
        });
        Self { dock_area }
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.dock_area.clone())
    }
}

fn main() {
    gpui_kit::application().run(|cx: &mut App| {
        gpui_kit::init(cx);

        let bounds = Bounds::centered(None, size(px(1024.0), px(768.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| MainWindow::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .unwrap();
    });
}
