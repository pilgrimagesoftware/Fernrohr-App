//! Unit tests for `ui::menu`: the platform items, and the menu bar's structure
//! built from the registry.

use super::{About, CloseWindow, Hide, Minimize, Quit, register_commands, registry_items};
use crate::command::{Command, CommandRegistry, MenuSlot, NavigateGroup, TopMenu, ViewGroup};
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
        menu: Some(MenuSlot::View(ViewGroup::Appearance)),
    });
    registry.register(Command {
        id: "test.two",
        title: "Two",
        default_binding: "cmd-2",
        context: None,
        action: Box::new(TestAction),
        menu: Some(MenuSlot::Navigate(NavigateGroup::Tabs)),
    });
    registry.register(Command {
        id: "test.three",
        title: "Three",
        default_binding: "cmd-3",
        context: None,
        action: Box::new(TestAction),
        menu: None,
    });

    let view_items = registry_items(TopMenu::View, &registry);
    assert_eq!(view_items.len(), 1);
}

#[test]
fn quit_and_about_are_distinct_actions() {
    assert!(!Quit.partial_eq(&About));
}

/// The items of `menu`, as the full app registers its commands, flattened to
/// text: an item's title, `---` for a separator, and `Name > [..]` for a
/// submenu.
fn structure(menu: TopMenu) -> Vec<String> {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    describe(&registry_items(menu, &registry))
}

fn describe(items: &[gpui_kit::MenuItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            gpui_kit::MenuItem::Separator => "---".to_string(),
            gpui_kit::MenuItem::Action { name, .. } => name.to_string(),
            gpui_kit::MenuItem::Submenu(menu) => {
                format!("{} > [{}]", menu.name, describe(&menu.items).join(", "))
            }
            gpui_kit::MenuItem::SystemMenu(_) => "(system)".to_string(),
        })
        .collect()
}

/// `menu-organization` 1.1: context actions, a separator, then every tunnel
/// action at the bottom.
#[test]
fn the_context_menu_puts_tunnels_last() {
    assert_eq!(
        structure(TopMenu::Context),
        [
            "Add Context to Window…",
            "Disconnect Active Context…",
            "---",
            "Set Tunnel for Context",
            "Manage Tunnels…",
        ]
    );
}

/// 1.2: the View menu by purpose - the palette, appearance, the Resource
/// panel, panel layout, table columns - with a separator between groups.
#[test]
fn the_view_menu_is_grouped() {
    assert_eq!(
        structure(TopMenu::View),
        [
            "Command Palette",
            "---",
            "Theme: Follow System",
            "Theme: Light",
            "Theme: Dark",
            "Increase Text Size",
            "Decrease Text Size",
            "Reset Text Size",
            "---",
            "Make Resource Panel's Side the Default",
            "Collapse/Expand Resource Panel",
            "Move Resource Panel to Other Side",
            "---",
            "Maximize/Restore Panel",
            "---",
            "Fit Columns to Contents",
        ]
    );
}

/// 1.3: Navigate holds exactly the global navigation - panel focus, then the
/// tab commands, with Select Tab 1-9 in one submenu.
#[test]
fn the_navigate_menu_holds_only_global_navigation() {
    assert_eq!(
        structure(TopMenu::Navigate),
        [
            "Focus Resources",
            "Focus Next Panel",
            "Focus Previous Panel",
            "---",
            "Next Tab",
            "Previous Tab",
            "---",
            "Select Tab > [Select Tab 1, Select Tab 2, Select Tab 3, Select Tab 4, \
             Select Tab 5, Select Tab 6, Select Tab 7, Select Tab 8, Select Last Tab]",
        ]
    );
}

/// 1.3: no panel-scoped command is in the menu bar. Every menu item is
/// global, so the bar never depends on which panel has focus - it is built
/// once at startup and rebuilt only when the keymap changes
/// (`settings::shortcuts`), never on a focus change.
#[test]
fn no_menu_item_is_scoped_to_a_panel() {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    let scoped: Vec<&str> = registry
        .iter()
        .filter(|command| command.menu.is_some() && command.context.is_some())
        .map(|command| command.id)
        .collect();
    assert!(
        scoped.is_empty(),
        "panel-scoped commands in the menu: {scoped:?}"
    );
}

/// 1.3: the commands that left the menu are still in the palette, offered
/// while their panel has focus.
#[test]
fn panel_commands_left_the_menu_but_not_the_palette() {
    use crate::k8s::resource::object_list::LIST_KEY_CONTEXT;
    use crate::k8s::resource::pods::PANEL_KEY_CONTEXT as PODS;
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    let cases: [(&str, &str); 10] = [
        (PODS, "pods.describe"),
        (PODS, "pods.logs"),
        (PODS, "pods.yaml"),
        (PODS, "pods.warp_namespace"),
        (PODS, "pods.fit_columns"),
        (LIST_KEY_CONTEXT, "object_list.open"),
        (LIST_KEY_CONTEXT, "object_list.describe"),
        (LIST_KEY_CONTEXT, "object_list.yaml"),
        (LIST_KEY_CONTEXT, "object_list.warp_namespace"),
        (LIST_KEY_CONTEXT, "object_list.fit_columns"),
    ];
    for (context, id) in cases {
        let command = registry
            .get(id)
            .unwrap_or_else(|| panic!("{id} registered"));
        assert_eq!(command.menu, None, "{id} is out of the menu bar");
        // The palette lists exactly the commands available in the focused
        // contexts (`command::build_items`).
        assert!(
            registry.available(&[context]).iter().any(|c| c.id == id),
            "{id} is still a palette item in its panel"
        );
    }
}
