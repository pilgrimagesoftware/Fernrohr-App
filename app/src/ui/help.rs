//! Help > Fernrohr Documentation (#361): a registered command, so it is in
//! the Help menu, the command palette, the status menu and `keymap.toml`,
//! that opens the user documentation site in the browser.

use crate::command::{Command, CommandRegistry, MenuSlot};
use gpui_kit::*;

actions!(help, [OpenDocumentation]);

pub const OPEN_DOCUMENTATION_COMMAND_ID: &str = "help.documentation";

/// Registers the command. Called before Report Issue's, so it leads the Help
/// menu as a Mac app's help item does.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: OPEN_DOCUMENTATION_COMMAND_ID,
        title: "Fernrohr Documentation",
        // Unbound by default: the Help shortcut (`cmd-?`) is Report Issue's.
        default_binding: "",
        context: None,
        action: Box::new(OpenDocumentation),
        menu: Some(MenuSlot::Help),
    });
}

/// App-wide, like the other Help items: it needs no window.
pub fn register_handler(cx: &mut App) {
    cx.on_action(|_: &OpenDocumentation, cx: &mut App| {
        cx.open_url(crate::consts::DOCS_URL);
    });
}

#[cfg(test)]
mod tests {
    // Named imports: a glob of `super::*` brings `gpui_kit::*`'s `test`
    // attribute macro, which shadows the built-in `#[test]`.
    use super::OPEN_DOCUMENTATION_COMMAND_ID;
    use crate::command::{CommandRegistry, MenuSlot};

    #[test]
    fn documentation_is_a_help_menu_command_that_works_with_no_window() {
        let mut registry = CommandRegistry::new();
        crate::util::shell::register_commands(&mut registry);
        let command = registry
            .get(OPEN_DOCUMENTATION_COMMAND_ID)
            .expect("help.documentation is registered");
        assert_eq!(command.menu, Some(MenuSlot::Help));
        assert_eq!(command.context, None);
        let help: Vec<&str> = registry
            .iter()
            .filter(|command| command.menu == Some(MenuSlot::Help))
            .map(|command| command.id)
            .collect();
        assert_eq!(
            help.first(),
            Some(&OPEN_DOCUMENTATION_COMMAND_ID),
            "it leads the Help menu: {help:?}"
        );
    }

    #[test]
    fn the_docs_url_is_the_sites_current_version() {
        assert!(crate::consts::DOCS_URL.starts_with("https://"));
        assert!(crate::consts::DOCS_URL.ends_with("/latest/"));
    }
}
