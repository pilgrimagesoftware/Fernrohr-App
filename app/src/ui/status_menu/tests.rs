//! #236: the status menu's entries, as a pure function of the contexts, and
//! each on the existing command it stands for.

use super::{CONTEXTS, Choice, Item, items};
use crate::command::CommandRegistry;

#[test]
fn the_menu_lists_windows_contexts_tools_help_then_quit() {
    let contexts = vec!["dev-cluster".to_string(), "prod-cluster".to_string()];
    let items = items(&contexts);
    assert_eq!(
        items[1],
        Item::Contexts(contexts),
        "the {CONTEXTS} submenu after New Window"
    );
    let titles: Vec<&str> = items
        .iter()
        .map(|item| match item {
            Item::Contexts(_) => CONTEXTS,
            Item::Command { title, .. } => title,
            Item::Separator => "-",
        })
        .collect();
    assert_eq!(
        titles,
        [
            "New Window",
            CONTEXTS,
            "Settings\u{2026}",
            "MCP\u{2026}",
            "Tunnel Manager\u{2026}",
            "-",
            "About Fernrohr",
            "Fernrohr Documentation",
            "Report Issue",
            "-",
            "Quit Fernrohr"
        ]
    );
}

#[test]
fn no_contexts_still_lists_the_submenu() {
    assert_eq!(items(&[])[1], Item::Contexts(Vec::new()));
}

/// Every fixed entry dispatches a registered, app-wide command's action.
#[test]
fn each_entry_is_an_existing_command() {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    for (choice, id) in [
        (Choice::Settings, "settings.open"),
        (
            Choice::Mcp,
            crate::ui::settings::OPEN_AGENT_ACCESS_COMMAND_ID,
        ),
        (Choice::TunnelManager, "tunnels.manage"),
        (Choice::NewWindow, "shell.new_window"),
        (Choice::About, "app.about"),
        (
            Choice::Documentation,
            crate::ui::help::OPEN_DOCUMENTATION_COMMAND_ID,
        ),
        (
            Choice::ReportIssue,
            crate::ui::report_issue::REPORT_ISSUE_COMMAND_ID,
        ),
        (Choice::Quit, "app.quit"),
    ] {
        let command = registry
            .get(id)
            .unwrap_or_else(|| panic!("{id} is registered"));
        assert!(
            command.action.partial_eq(choice.action().as_ref()),
            "{choice:?} dispatches {id}"
        );
        assert_eq!(command.context, None, "{id} works with no window");
    }
}

#[test]
fn a_context_entry_opens_that_context() {
    let action = Choice::Context("dev-cluster".into()).action();
    assert!(action.partial_eq(&crate::util::shell::OpenContextWindow {
        context: "dev-cluster".into()
    }));
}

/// #362: an entry that shows a window brings the app forward with it; the
/// documentation opens in the browser and Quit shows nothing.
#[test]
fn window_entries_bring_the_app_forward() {
    for choice in [
        Choice::NewWindow,
        Choice::About,
        Choice::ReportIssue,
        Choice::Settings,
        Choice::Context("dev-cluster".into()),
    ] {
        assert!(choice.shows_a_window(), "{choice:?}");
    }
    assert!(!Choice::Documentation.shows_a_window());
    assert!(!Choice::Quit.shows_a_window());
}
