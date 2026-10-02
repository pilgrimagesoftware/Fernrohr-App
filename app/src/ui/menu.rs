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

use crate::command::{Command, CommandRegistry, TopMenu};
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
            let app_items = registry_items(TopMenu::App, registry);
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
        menu_from_registry("Context", TopMenu::Context, registry),
        menu_from_registry("Edit", TopMenu::Edit, registry),
        menu_from_registry("View", TopMenu::View, registry),
        menu_from_registry("Navigate", TopMenu::Navigate, registry),
        Menu::new("Window").items({
            let mut items = registry_items(TopMenu::Window, registry);
            items.push(MenuItem::separator());
            items.push(MenuItem::action("Minimize", Minimize));
            items.push(MenuItem::action("Zoom", Zoom));
            items.push(MenuItem::separator());
            items.push(MenuItem::action("Close Window", CloseWindow));
            items
        }),
        menu_from_registry("Help", TopMenu::Help, registry),
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

fn menu_from_registry(name: &'static str, menu: TopMenu, registry: &CommandRegistry) -> Menu {
    Menu::new(name).items(registry_items(menu, registry))
}

/// `menu`'s registry items, grouped (`menu-organization`): each group's items
/// in registration order, a separator between groups, and a group that names
/// a submenu drawn as that one submenu.
///
/// Built once, from the registry - not per focus change. A panel-scoped
/// command carries no menu slot, so nothing here depends on which panel has
/// focus; the native menu greys out an item whose action nothing on the focus
/// path answers.
fn registry_items(menu: TopMenu, registry: &CommandRegistry) -> Vec<MenuItem> {
    let mut items = Vec::new();
    let mut group = None;
    let mut submenu: Option<(&'static str, Vec<MenuItem>)> = None;
    for command in registry.for_menu(menu) {
        let Some(slot) = command.menu else {
            continue;
        };
        if group.is_some_and(|group| group != slot.group()) {
            if let Some((title, sub_items)) = submenu.take() {
                items.push(MenuItem::submenu(Menu::new(title).items(sub_items)));
            }
            items.push(MenuItem::separator());
        }
        group = Some(slot.group());
        let item = command_item(command);
        match (slot.submenu(), &mut submenu) {
            (Some(_), Some((_, sub_items))) => sub_items.push(item),
            (Some(title), None) => submenu = Some((title, vec![item])),
            (None, _) => items.push(item),
        }
    }
    if let Some((title, sub_items)) = submenu {
        items.push(MenuItem::submenu(Menu::new(title).items(sub_items)));
    }
    items
}

fn command_item(command: &Command) -> MenuItem {
    MenuItem::Action {
        name: command.title.into(),
        action: command.action.boxed_clone(),
        os_action: None,
        checked: false,
        disabled: false,
    }
}

#[cfg(test)]
mod tests;
