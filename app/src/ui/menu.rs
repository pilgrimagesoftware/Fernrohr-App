//! The native application menu bar: seven top-level menus, built once from
//! the [`CommandRegistry`](crate::command::CommandRegistry) so a menu item
//! and its palette/keymap entry are the same registered command, never a
//! second list that can drift from the first.
//!
//! The platform items - `Quit`/`About`/`Hide` under App, `Minimize`/`Zoom`/
//! `Close Window` under Window - keep their native places in those two menus,
//! so the menu builds them directly rather than from a registry slot. They are
//! still registered commands ([`register_commands`], `menu: None`): every
//! user-facing action reaches the command palette and takes a `keymap.toml`
//! override, and its key comes from the registry like any other command's.

use crate::command::{Command, CommandRegistry, MenuSlot};
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

/// Registers the platform items' handlers and installs the menu bar. Called
/// once at startup, after every command is registered and before the
/// registry is moved into its global slot.
pub fn init(registry: &CommandRegistry, cx: &mut App) {
    register_handlers(cx);
    cx.set_menus(menus(registry));
}

/// Re-installs the menu bar from the registry global. The native menu reads
/// each item's shortcut from the live keymap when it's installed, so this is
/// what makes the menu show a key the keybindings editor just changed.
/// Handlers aren't re-registered - they're app-wide and already in place.
pub fn rebuild_menus(cx: &mut App) {
    let menus = menus(cx.global::<CommandRegistry>());
    cx.set_menus(menus);
}

fn register_handlers(cx: &mut App) {
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
    crate::ui::about_window::register_about_action(cx);
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
}

/// The seven-menu bar, from `registry`.
fn menus(registry: &CommandRegistry) -> Vec<Menu> {
    vec![
        Menu::new("App").items({
            let mut items = vec![MenuItem::action("About Fernrohr", About)];
            // Settings… and anything else the registry puts in the App menu,
            // between About and Services as on every Mac app.
            let app_items = registry_items(MenuSlot::App, registry);
            if !app_items.is_empty() {
                items.push(MenuItem::separator());
                items.extend(app_items);
            }
            items.extend([
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide Fernrohr", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit Fernrohr", Quit),
            ]);
            items
        }),
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
    ]
}

/// Each platform's standard shortcut for an item, or none - `Hide` and friends
/// are macOS conventions with nothing to bind elsewhere.
const fn platform_key(macos: &'static str, other: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        macos
    } else {
        other
    }
}

/// The App and Window menus' platform items as registered commands. A menu
/// item shows (and answers to) whatever key its action is bound to, so these
/// bindings are also what gives the native items their shortcuts.
pub fn register_commands(registry: &mut CommandRegistry) {
    let commands: [(&'static str, &'static str, &'static str, Box<dyn Action>); 8] = [
        ("app.about", "About Fernrohr", "", Box::new(About)),
        (
            "app.hide",
            "Hide Fernrohr",
            platform_key("cmd-h", ""),
            Box::new(Hide),
        ),
        (
            "app.hide_others",
            "Hide Others",
            platform_key("alt-cmd-h", ""),
            Box::new(HideOthers),
        ),
        ("app.show_all", "Show All", "", Box::new(ShowAll)),
        (
            "app.quit",
            "Quit Fernrohr",
            platform_key("cmd-q", "ctrl-q"),
            Box::new(Quit),
        ),
        (
            "window.minimize",
            "Minimize",
            platform_key("cmd-m", ""),
            Box::new(Minimize),
        ),
        ("window.zoom", "Zoom", "", Box::new(Zoom)),
        (
            "window.close",
            "Close Window",
            platform_key("cmd-w", "ctrl-w"),
            Box::new(CloseWindow),
        ),
    ];
    for (id, title, default_binding, action) in commands {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: None,
            action,
            menu: None,
        });
    }
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

#[cfg(test)]
mod tests {
    use super::{
        About, CloseWindow, Hide, MenuSlot, Minimize, Quit, register_commands, registry_items,
    };
    use crate::command::{Command, CommandRegistry};
    use gpui_kit::{Action, actions};

    actions!(menu_test, [TestAction]);

    /// After a keymap edit and `rebuild_menus`, the menu bar is re-installed
    /// with the command still in it, and the key the native menu shows - the
    /// action's earliest binding that no `Unbind` cancelled - is the new one.
    /// Rebuilding doesn't register handlers again: About still opens exactly
    /// one window.
    #[gpui_kit::test]
    fn rebuilt_menus_show_the_new_key_and_fire_once(cx: &mut gpui_kit::TestAppContext) {
        use crate::keymap::{Edit, apply};
        use crate::ui::panel::focus::FocusNextPanel;

        cx.executor().allow_parking();
        let dir = std::env::temp_dir();
        let n = std::process::id();
        let (workspace, keymap) = (
            dir.join(format!("fernrohr-menu-workspace-{n}.toml")),
            dir.join(format!("fernrohr-menu-keymap-{n}.toml")),
        );
        let _ = std::fs::remove_file(&keymap);
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            crate::util::shell::init(cx, workspace.clone(), &keymap);

            apply(cx, "panel.focus_next", Edit::Set("cmd-shift-j".into())).expect("saved");
            super::rebuild_menus(cx);
            super::rebuild_menus(cx);

            let menus = cx.get_menus().expect("a menu bar is installed");
            let navigate = menus
                .iter()
                .find(|menu| menu.name.as_ref() == "Navigate")
                .expect("a Navigate menu");
            assert!(
                navigate.items.iter().any(|item| matches!(
                    item,
                    gpui_kit::OwnedMenuItem::Action { name, .. } if name == "Focus Next Panel"
                )),
                "the command is still in its menu"
            );
            let shown = cx
                .key_bindings()
                .borrow()
                .bindings_for_action(&FocusNextPanel)
                .next()
                .map(|binding| {
                    binding
                        .keystrokes()
                        .iter()
                        .map(|key| gpui_kit::AsKeystroke::as_keystroke(key).unparse())
                        .collect::<Vec<_>>()
                });
            let expected = gpui_kit::Keystroke::parse("cmd-shift-j").unwrap().unparse();
            assert_eq!(shown, Some(vec![expected]), "the menu shows the new key");
        });

        let before = cx.update(|cx| cx.windows().len());
        cx.update(|cx| cx.dispatch_action(&About));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|cx| cx.windows().len()),
            before + 1,
            "About's handler ran once, not once per rebuild"
        );
        let _ = std::fs::remove_file(&workspace);
        let _ = std::fs::remove_file(&keymap);
    }

    /// Settings… sits in the App menu, between About and Services, once the
    /// registry has it - `MenuSlot::App`'s first command.
    #[test]
    fn the_app_menu_carries_settings() {
        let mut registry = CommandRegistry::new();
        crate::ui::settings::register_commands(&mut registry);
        let menus = super::menus(&registry);
        let names: Vec<String> = menus[0]
            .items
            .iter()
            .filter_map(|item| match item {
                gpui_kit::MenuItem::Action { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(names.first().map(String::as_str), Some("About Fernrohr"));
        assert_eq!(names.get(1).map(String::as_str), Some("Settings…"));
    }

    /// The App and Window menu items answer to each platform's standard
    /// shortcuts - through their registered commands, the path every command's
    /// key takes - and every platform item reaches the palette. Hide and
    /// Minimize are macOS conventions, unbound elsewhere.
    #[test]
    fn platform_items_have_their_standard_shortcuts() {
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            &gpui_kit::DummyKeyboardMapper,
        );
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
        let macos = cfg!(target_os = "macos");
        assert_eq!(
            key_for(&Quit).as_deref(),
            Some(if macos { "cmd-q" } else { "ctrl-q" })
        );
        assert_eq!(key_for(&Hide).as_deref(), macos.then_some("cmd-h"));
        assert_eq!(key_for(&Minimize).as_deref(), macos.then_some("cmd-m"));
        assert_eq!(
            key_for(&CloseWindow).as_deref(),
            Some(if macos { "cmd-w" } else { "ctrl-w" })
        );
        assert_eq!(
            key_for(&About),
            None,
            "About has no shortcut, and no warning"
        );
        assert_eq!(
            registry.available(&[]).len(),
            8,
            "every platform item is a palette entry"
        );
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
