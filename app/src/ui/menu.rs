//! The native application menu bar: seven top-level menus, built once from
//! the [`CommandRegistry`](crate::command::CommandRegistry) so a menu item
//! and its palette/keymap entry are the same registered command, never a
//! second list that can drift from the first.
//!
//! Two kinds of item that have no `CommandRegistry` entry and never will -
//! `Quit`/`About` under App, `Minimize`/`Zoom` under Window - are platform
//! affordances, not app actions, and are built directly against GPUI's menu
//! API rather than routed through the registry.

use crate::command::{CommandRegistry, MenuSlot};
use gpui_kit::*;

actions!(
    app_menu,
    [
        Quit,
        About,
        Minimize,
        Zoom,
        Hide,
        HideOthers,
        ShowAll,
        CloseWindow
    ]
);

/// Builds the seven-menu bar from `registry` and installs it via
/// `cx.set_menus`. Called once at startup, after every command is
/// registered and before the registry is moved into its global slot.
pub fn init(registry: &CommandRegistry, cx: &mut App) {
    cx.bind_keys(platform_bindings());
    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
    cx.on_action(|_: &Hide, cx: &mut App| cx.hide());
    cx.on_action(|_: &HideOthers, cx: &mut App| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx: &mut App| cx.unhide_other_apps());
    cx.on_action(|_: &CloseWindow, cx: &mut App| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, cx| {
                crate::util::shell::close_window(window, cx)
            });
        }
    });
    cx.on_action(|_: &About, cx: &mut App| about_window(cx));
    cx.on_action(|_: &Minimize, cx: &mut App| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.minimize_window());
        }
    });
    cx.on_action(|_: &Zoom, cx: &mut App| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.zoom_window());
        }
    });

    cx.set_menus(vec![
        Menu::new("App").items(vec![
            MenuItem::action("About Fernrohr", About),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide Fernrohr", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit Fernrohr", Quit),
        ]),
        menu_from_registry("Context", MenuSlot::Context, registry),
        menu_from_registry("Edit", MenuSlot::Edit, registry),
        menu_from_registry("View", MenuSlot::View, registry),
        menu_from_registry("Navigate", MenuSlot::Navigate, registry),
        Menu::new("Window").items({
            let mut items = registry_items(MenuSlot::Window, registry);
            items.push(MenuItem::separator());
            items.push(MenuItem::action("Minimize", Minimize));
            items.push(MenuItem::action("Zoom", Zoom));
            items.push(MenuItem::separator());
            items.push(MenuItem::action("Close Window", CloseWindow));
            items
        }),
        menu_from_registry("Help", MenuSlot::Help, registry),
    ]);
}

/// The platform's standard shortcuts for the App and Window menu items above. A menu
/// item shows (and answers to) whatever key its action is bound to, so without these
/// the items had no shortcuts and ⌘Q did nothing.
fn platform_bindings() -> Vec<KeyBinding> {
    #[cfg(target_os = "macos")]
    return vec![
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
    ];
    #[cfg(not(target_os = "macos"))]
    return vec![
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-w", CloseWindow, None),
    ];
}

fn menu_from_registry(name: &'static str, slot: MenuSlot, registry: &CommandRegistry) -> Menu {
    Menu::new(name).items(registry_items(slot, registry))
}

fn registry_items(slot: MenuSlot, registry: &CommandRegistry) -> Vec<MenuItem> {
    registry
        .for_menu(slot)
        .into_iter()
        .map(|command| MenuItem::Action {
            name: command.title.into(),
            action: command.action.boxed_clone(),
            os_action: None,
            checked: false,
            disabled: false,
        })
        .collect()
}

/// A small, fixed-size window naming the app and its build version - real
/// `CARGO_PKG_*` values, not placeholder text. Not a modal: this app has no
/// modal/dialog system yet, and building one just for this would be its own
/// change; a plain window is the smallest honest thing that works.
fn about_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(320.), px(180.)), cx);
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(320.), px(180.))),
            titlebar: None,
            ..Default::default()
        },
        |window, cx| {
            crate::ui::theme::watch_window(window, cx);
            cx.new(|cx| AboutView {
                _cx: cx.entity_id(),
            })
        },
    );
}

struct AboutView {
    _cx: EntityId,
}

impl Render for AboutView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::ActiveTheme as _;
        let theme = cx.theme();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(theme.background)
            .child(div().text_lg().child(env!("CARGO_PKG_NAME").to_string()))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        About, CloseWindow, Hide, MenuSlot, Minimize, Quit, platform_bindings, registry_items,
    };
    use crate::command::{Command, CommandRegistry};
    use gpui_kit::{Action, actions};

    actions!(menu_test, [TestAction]);

    /// The App and Window menu items answer to the platform's standard shortcuts -
    /// without these bindings the items showed no keys and ⌘Q did nothing.
    #[cfg(target_os = "macos")]
    #[test]
    fn platform_items_have_their_standard_shortcuts() {
        let bindings = platform_bindings();
        let key_for = |action: &dyn Action| {
            bindings
                .iter()
                .find(|binding| binding.action().partial_eq(action))
                .map(|binding| {
                    binding
                        .keystrokes()
                        .iter()
                        .map(|keystroke| keystroke.unparse())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
        };
        assert_eq!(key_for(&Quit).as_deref(), Some("cmd-q"));
        assert_eq!(key_for(&Hide).as_deref(), Some("cmd-h"));
        assert_eq!(key_for(&Minimize).as_deref(), Some("cmd-m"));
        assert_eq!(key_for(&CloseWindow).as_deref(), Some("cmd-w"));
    }

    #[test]
    fn registry_items_only_returns_the_requested_slot() {
        let mut registry = CommandRegistry::new();
        registry.register(Command {
            id: "test.one",
            title: "One",
            default_binding: "cmd-1",
            context: None,
            action: Box::new(TestAction),
            menu: Some(MenuSlot::View),
        });
        registry.register(Command {
            id: "test.two",
            title: "Two",
            default_binding: "cmd-2",
            context: None,
            action: Box::new(TestAction),
            menu: Some(MenuSlot::Navigate),
        });
        registry.register(Command {
            id: "test.three",
            title: "Three",
            default_binding: "cmd-3",
            context: None,
            action: Box::new(TestAction),
            menu: None,
        });

        let view_items = registry_items(MenuSlot::View, &registry);
        assert_eq!(view_items.len(), 1);
    }

    #[test]
    fn quit_and_about_are_distinct_actions() {
        assert!(!Quit.partial_eq(&About));
    }
}
