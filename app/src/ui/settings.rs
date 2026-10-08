//! The Settings window: `settings.open` (Settings…, `cmd-,`, the App menu), a
//! single-instance window with a sections sidebar: Keyboard Shortcuts
//! ([`shortcuts`], with the Shortcut Timeout above the list) and Appearance
//! ([`appearance`]: Text Size and the theme). Later settings join the sidebar
//! rather than each opening a window of its own.
//!
//! The sidebar's entries are buttons - Tab reaches them, Enter or Space shows
//! the section - and Show Keyboard Shortcuts and Show Appearance are palette
//! commands while the window has focus. They have no default key: a plain
//! `cmd-<digit>` would tie with the global show-panel keys. Showing a section
//! moves focus into it, so the next Tab reaches its first control.

use crate::command::{Command, CommandRegistry, MenuSlot};
use crate::consts::{SETTINGS_WINDOW_MIN_SIZE, SETTINGS_WINDOW_SIZE};
use gpui_kit::component::{ActiveTheme as _, Root};
use gpui_kit::*;

mod appearance;
mod layouts;
mod panels;
mod recorder;
mod rows;
mod shortcut_timeout;
pub mod shortcuts;
#[cfg(test)]
mod tests;
mod text_size;

pub use shortcuts::ShortcutsSection;

actions!(
    settings,
    [
        OpenSettings,
        ShowKeyboardShortcuts,
        ShowAppearance,
        ShowPanels,
        ShowLayouts
    ]
);

/// The window's key context: where the section commands are bound.
pub(crate) const KEY_CONTEXT: &str = "SettingsWindow";

/// The sections sidebar, for layout tests.
pub(crate) const SIDEBAR: &str = "settings-sidebar";

/// Which section the window shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    KeyboardShortcuts,
    Appearance,
    /// Where panels open: a pod's logs in its own or the shared Logs panel.
    Panels,
    /// Lists and removes saved panel layouts (`saved-panel-layouts` design.md
    /// D7). Renaming stays in the saved layouts picker.
    Layouts,
}

impl Section {
    fn title(self) -> &'static str {
        match self {
            Section::KeyboardShortcuts => "Keyboard Shortcuts",
            Section::Appearance => "Appearance",
            Section::Panels => "Panels",
            Section::Layouts => "Layouts",
        }
    }

    /// The sidebar button's element id.
    pub(crate) fn button_id(self) -> &'static str {
        match self {
            Section::KeyboardShortcuts => "settings-section-shortcuts",
            Section::Appearance => "settings-section-appearance",
            Section::Panels => "settings-section-panels",
            Section::Layouts => "settings-section-layouts",
        }
    }

    /// The debug selector of the sidebar button's title, for layout tests.
    pub(crate) fn label_selector(self) -> &'static str {
        match self {
            Section::KeyboardShortcuts => "settings-section-shortcuts-label",
            Section::Appearance => "settings-section-appearance-label",
            Section::Panels => "settings-section-panels-label",
            Section::Layouts => "settings-section-layouts-label",
        }
    }
}

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
    for (id, title, default_binding, action) in [
        (
            "settings.show_shortcuts",
            "Settings: Show Keyboard Shortcuts",
            "",
            Box::new(ShowKeyboardShortcuts) as Box<dyn Action>,
        ),
        (
            "settings.show_appearance",
            "Settings: Show Appearance",
            "",
            Box::new(ShowAppearance),
        ),
        (
            "settings.show_panels",
            "Settings: Show Panels",
            "",
            Box::new(ShowPanels),
        ),
        (
            "settings.show_layouts",
            "Settings: Show Layouts",
            "",
            Box::new(ShowLayouts),
        ),
    ] {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(KEY_CONTEXT),
            action,
            menu: None,
        });
    }
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
    let (window, _) = gpui_kit::open_window(
        WindowOptions {
            app_id: Some(crate::consts::APP_ID.into()),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                SETTINGS_WINDOW_SIZE,
                cx,
            ))),
            window_min_size: Some(SETTINGS_WINDOW_MIN_SIZE),
            ..Default::default()
        },
        cx,
        |window, cx| {
            crate::ui::theme::watch_window(window, cx);
            let view = cx.new(|cx| SettingsWindow::new(window, cx));
            let focus = view.read(cx).shortcuts.read(cx).list_focus();
            window.focus(&focus, cx);
            view
        },
    )
    .expect("failed to open the settings window");
    let handle = window
        .downcast::<Root>()
        .expect("gpui_kit::open_window roots every window in a Root");
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
    section: Section,
    /// The Appearance section's own focus, where showing it puts focus.
    appearance_focus: FocusHandle,
    /// The Panels section's, likewise.
    panels_focus: FocusHandle,
    /// The Layouts section's, likewise.
    layouts_focus: FocusHandle,
    /// The saved layouts the Layouts section shows, read off the main thread
    /// (`layouts::LayoutsCache`) - never by rendering.
    layouts: layouts::LayoutsCache,
}

impl SettingsWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A save, rename or removal anywhere re-reads the list while the
        // Layouts section is the one shown; the next show re-reads it anyway.
        cx.observe_global::<crate::util::shell::SavedLayoutsChanged>(|this: &mut Self, cx| {
            if this.section == Section::Layouts {
                this.reload_layouts(cx);
            }
        })
        .detach();
        Self {
            shortcuts: cx.new(|cx| ShortcutsSection::new(window, cx)),
            section: Section::default(),
            appearance_focus: cx.focus_handle(),
            panels_focus: cx.focus_handle(),
            layouts_focus: cx.focus_handle(),
            layouts: layouts::LayoutsCache::default(),
        }
    }

    /// Shows `section` and puts focus in it: the shortcuts list, or the top of
    /// the Appearance section, so Tab goes on to its first control.
    fn show(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.section = section;
        match section {
            Section::KeyboardShortcuts => {
                let focus = self.shortcuts.read(cx).list_focus();
                window.focus(&focus, cx);
            }
            Section::Appearance => window.focus(&self.appearance_focus, cx),
            Section::Panels => window.focus(&self.panels_focus, cx),
            Section::Layouts => {
                window.focus(&self.layouts_focus, cx);
                self.reload_layouts(cx);
            }
        }
        cx.notify();
    }

    /// Test-only: the section shown.
    #[cfg(test)]
    pub(crate) fn section(&self) -> Section {
        self.section
    }

    /// A sidebar entry: a button that shows `section`, marked while shown.
    ///
    /// The button fills the sidebar's width, so its selected background does,
    /// and its title starts at the left like a list's. The title is the
    /// button's own content rather than its `label`: a `Button` centres its
    /// label and ellipsizes it to its natural width.
    fn sidebar_button(&self, section: Section, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::Selectable as _;
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        use gpui_kit::prelude::FluentBuilder as _;
        let shown = self.section == section;
        let label = section.label_selector();
        Button::new(section.button_id())
            .accessibility_label(section.title())
            .child(
                div()
                    .debug_selector(move || label.into())
                    .w_full()
                    .whitespace_nowrap()
                    .child(section.title()),
            )
            .w_full()
            .ghost()
            .when(shown, |button| button.selected(true))
            .on_click(cx.listener(move |this, _event, window, cx| this.show(section, window, cx)))
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
        let space = crate::ui::space::spacing(cx);
        // At least 180px, wider when a title needs it - at a larger text
        // size, say - so a section's name is never cut short.
        let sidebar = div()
            .debug_selector(|| SIDEBAR.into())
            .flex_none()
            .min_w(px(180.))
            .h_full()
            .p(space.control_gap)
            .flex()
            .flex_col()
            .gap_1()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(self.sidebar_button(Section::KeyboardShortcuts, cx))
            .child(self.sidebar_button(Section::Appearance, cx))
            .child(self.sidebar_button(Section::Panels, cx))
            .child(self.sidebar_button(Section::Layouts, cx));
        let content = match self.section {
            Section::KeyboardShortcuts => div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                // Inset like the Shortcuts list below it, whose own top inset
                // is the gap between the two.
                .child(
                    div()
                        .px(space.panel_inset)
                        .pt(space.panel_inset)
                        .child(shortcut_timeout::row(cx)),
                )
                .child(div().flex_1().min_h_0().child(self.shortcuts.clone()))
                .into_any_element(),
            Section::Appearance => div()
                .flex_1()
                .min_w_0()
                .track_focus(&self.appearance_focus)
                .child(appearance::section(window, cx))
                .into_any_element(),
            Section::Panels => div()
                .flex_1()
                .min_w_0()
                .track_focus(&self.panels_focus)
                .child(panels::section(cx))
                .into_any_element(),
            Section::Layouts => div()
                .flex_1()
                .min_w_0()
                .track_focus(&self.layouts_focus)
                .child(layouts::section(&self.layouts, cx))
                .into_any_element(),
        };
        div()
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|this, _: &ShowKeyboardShortcuts, window, cx| {
                this.show(Section::KeyboardShortcuts, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowAppearance, window, cx| {
                this.show(Section::Appearance, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowPanels, window, cx| {
                this.show(Section::Panels, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowLayouts, window, cx| {
                this.show(Section::Layouts, window, cx)
            }))
            .size_full()
            .flex()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(sidebar)
            .child(content)
    }
}
