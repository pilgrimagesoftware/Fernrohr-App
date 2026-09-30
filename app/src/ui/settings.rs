//! The Settings window: `settings.open` (Settings…, `cmd-,`, the App menu), a
//! single-instance window with a sections sidebar. Keyboard Shortcuts is its
//! only section today ([`shortcuts`]); later settings join the sidebar rather
//! than each opening a window of its own.

use crate::command::{Command, CommandRegistry, MenuSlot};
use crate::consts::{SETTINGS_WINDOW_MIN_SIZE, SETTINGS_WINDOW_SIZE};
use gpui_kit::component::{ActiveTheme as _, Root};
use gpui_kit::*;

mod recorder;
mod rows;
pub mod shortcuts;
#[cfg(test)]
mod tests;

pub use shortcuts::ShortcutsSection;

actions!(settings, [OpenSettings]);

/// Registers Settings… and the Keyboard Shortcuts section's own commands.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: "settings.open",
        title: "Settings…",
        default_binding: "cmd-,",
        context: None,
        action: Box::new(OpenSettings),
        menu: Some(MenuSlot::App),
    });
    shortcuts::register_commands(registry);
}

/// The app-wide handler and the section's raw ↑/↓ bindings. Called once at
/// startup, next to the other windows' handlers.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &OpenSettings, cx: &mut App| open_or_focus(cx));
    cx.bind_keys(shortcuts::list_bindings());
}

/// The one Settings window, if open.
struct SettingsWindowHandle(Option<WindowHandle<Root>>);

impl Global for SettingsWindowHandle {}

/// Opens the Settings window, or brings the open one to the front - the
/// Tunnels window's single-instance pattern.
pub fn open_or_focus(cx: &mut App) {
    if let Some(handle) = cx
        .try_global::<SettingsWindowHandle>()
        .and_then(|handle| handle.0)
        && cx.windows().contains(&handle.into())
    {
        // Deferred: `cmd-,` pressed in the Settings window itself runs while that
        // window is mid-update, where updating it again would fail - and falling
        // through would open a second one.
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        });
        return;
    }
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    SETTINGS_WINDOW_SIZE,
                    cx,
                ))),
                window_min_size: Some(SETTINGS_WINDOW_MIN_SIZE),
                ..Default::default()
            },
            |window, cx| {
                crate::ui::theme::watch_window(window, cx);
                let view = cx.new(|cx| SettingsWindow::new(window, cx));
                let focus = view.read(cx).shortcuts.read(cx).list_focus();
                window.focus(&focus, cx);
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("failed to open the settings window");
    let _ = handle.update(cx, |_, window, cx| {
        window.on_window_should_close(cx, |_, cx| {
            cx.set_global(SettingsWindowHandle(None));
            true
        });
    });
    cx.set_global(SettingsWindowHandle(Some(handle)));
}

/// Test-only: the open Settings window, if any.
#[cfg(test)]
fn test_window(cx: &App) -> Option<WindowHandle<Root>> {
    cx.try_global::<SettingsWindowHandle>()?.0
}

pub struct SettingsWindow {
    shortcuts: Entity<ShortcutsSection>,
}

impl SettingsWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            shortcuts: cx.new(|cx| ShortcutsSection::new(window, cx)),
        }
    }

    /// Test-only: the Keyboard Shortcuts section.
    #[cfg(test)]
    fn shortcuts(&self) -> Entity<ShortcutsSection> {
        self.shortcuts.clone()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let sidebar = div()
            .w(px(180.))
            .h_full()
            .p_2()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(theme.accent)
                    .child("Keyboard Shortcuts"),
            );
        div()
            .size_full()
            .flex()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(sidebar)
            .child(div().flex_1().min_w_0().child(self.shortcuts.clone()))
            .children(Root::render_dialog_layer(window, cx))
    }
}
