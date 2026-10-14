//! The bulk selection's keyboard routes as registered commands: checking rows
//! (`x`, Shift+`x`), clearing them, and one command per bulk action.
//!
//! Each bulk action is gated by its own key context, which a list panel adds
//! only while its checked set is offered that action ([`add_contexts`]). So the
//! palette offers Scale only while scalable rows are checked, and none of these
//! fires with nothing checked. The bulk actions have no default key: they are
//! reached from the bar and the palette, and never share the single-row
//! `ctrl-d`/`ctrl-k`, which keep acting on the cursor's row alone.

use crate::command::{Command, CommandRegistry};
use crate::k8s::resource::bulk::BulkAction;
use gpui_kit::{Action, Div, KeyContext, Window, actions};

actions!(
    bulk,
    [
        ToggleChecked,
        CheckRange,
        ClearChecked,
        BulkViewLogs,
        BulkRestartRollout,
        BulkRollbackRollout,
        BulkScale,
        BulkCordon,
        BulkUncordon,
        BulkDrain,
        BulkLabel,
        BulkAnnotate,
        BulkCopyNames,
        BulkCopyYaml,
        BulkDelete
    ]
);

/// Added by every list panel whose rows can be checked.
pub(crate) const CHECKABLE_CONTEXT: &str = "CheckableList";
/// Added while at least one row is checked.
pub(crate) const CHECKED_CONTEXT: &str = "CheckedRows";

pub(crate) const TOGGLE_KEY: &str = "x";
pub(crate) const RANGE_KEY: &str = "shift-x";

/// The key context that gates `action`'s command.
fn context(action: BulkAction) -> &'static str {
    match action {
        BulkAction::ViewLogs => "BulkViewLogs",
        BulkAction::RestartRollout => "BulkRestartRollout",
        BulkAction::RollbackRollout => "BulkRollbackRollout",
        BulkAction::Scale => "BulkScale",
        BulkAction::Cordon => "BulkCordon",
        BulkAction::Uncordon => "BulkUncordon",
        BulkAction::Drain => "BulkDrain",
        BulkAction::Label => "BulkLabel",
        BulkAction::Annotate => "BulkAnnotate",
        BulkAction::CopyNames => "BulkCopyNames",
        BulkAction::CopyYaml => "BulkCopyYaml",
        BulkAction::Delete => "BulkDelete",
    }
}

/// `action`'s command id, palette title, and action.
pub(crate) fn command(action: BulkAction) -> (&'static str, &'static str, Box<dyn Action>) {
    match action {
        BulkAction::ViewLogs => (
            "bulk.view_logs",
            "Checked: View Logs",
            Box::new(BulkViewLogs),
        ),
        BulkAction::RestartRollout => (
            "bulk.restart_rollout",
            "Checked: Restart Rollout…",
            Box::new(BulkRestartRollout),
        ),
        BulkAction::RollbackRollout => (
            "bulk.rollback_rollout",
            "Checked: Rollback Rollout…",
            Box::new(BulkRollbackRollout),
        ),
        BulkAction::Scale => ("bulk.scale", "Checked: Scale…", Box::new(BulkScale)),
        BulkAction::Cordon => ("bulk.cordon", "Checked: Cordon…", Box::new(BulkCordon)),
        BulkAction::Uncordon => (
            "bulk.uncordon",
            "Checked: Uncordon…",
            Box::new(BulkUncordon),
        ),
        BulkAction::Drain => ("bulk.drain", "Checked: Drain…", Box::new(BulkDrain)),
        BulkAction::Label => ("bulk.label", "Checked: Label…", Box::new(BulkLabel)),
        BulkAction::Annotate => (
            "bulk.annotate",
            "Checked: Annotate…",
            Box::new(BulkAnnotate),
        ),
        BulkAction::CopyNames => (
            "bulk.copy_names",
            "Checked: Copy Names",
            Box::new(BulkCopyNames),
        ),
        BulkAction::CopyYaml => (
            "bulk.copy_yaml",
            "Checked: Copy YAML",
            Box::new(BulkCopyYaml),
        ),
        BulkAction::Delete => ("bulk.delete", "Checked: Delete…", Box::new(BulkDelete)),
    }
}

pub fn register_commands(registry: &mut CommandRegistry) {
    let checkable = "CheckableList && !Input";
    registry.register(Command {
        id: "bulk.toggle_checked",
        title: "List: Check or Uncheck Selected Row",
        default_binding: TOGGLE_KEY,
        context: Some(checkable),
        action: Box::new(ToggleChecked),
        menu: None,
    });
    registry.register(Command {
        id: "bulk.check_range",
        title: "List: Check Rows Up to Selected",
        default_binding: RANGE_KEY,
        context: Some(checkable),
        action: Box::new(CheckRange),
        menu: None,
    });
    registry.register(Command {
        id: "bulk.clear_checked",
        title: "Checked: Uncheck All",
        default_binding: "",
        context: Some("CheckedRows && !Input"),
        action: Box::new(ClearChecked),
        menu: None,
    });
    for action in BulkAction::ALL {
        let (id, title, boxed) = command(action);
        registry.register(Command {
            id,
            title,
            default_binding: "",
            context: Some(gated(action)),
            action: boxed,
            menu: None,
        });
    }
}

/// `action`'s command context: its gate, outside a text field.
fn gated(action: BulkAction) -> &'static str {
    match action {
        BulkAction::ViewLogs => "BulkViewLogs && !Input",
        BulkAction::RestartRollout => "BulkRestartRollout && !Input",
        BulkAction::RollbackRollout => "BulkRollbackRollout && !Input",
        BulkAction::Scale => "BulkScale && !Input",
        BulkAction::Cordon => "BulkCordon && !Input",
        BulkAction::Uncordon => "BulkUncordon && !Input",
        BulkAction::Drain => "BulkDrain && !Input",
        BulkAction::Label => "BulkLabel && !Input",
        BulkAction::Annotate => "BulkAnnotate && !Input",
        BulkAction::CopyNames => "BulkCopyNames && !Input",
        BulkAction::CopyYaml => "BulkCopyYaml && !Input",
        BulkAction::Delete => "BulkDelete && !Input",
    }
}

/// The hint row's Check entry (`x`), read from the live keymap.
pub(crate) fn check_hint(window: &mut Window) -> Div {
    crate::ui::list_search::ListSearch::hint(
        &ToggleChecked,
        CHECKABLE_CONTEXT,
        TOGGLE_KEY,
        "Check",
        window,
    )
}

/// Adds a checkable list's contexts: always [`CHECKABLE_CONTEXT`], and while
/// rows are checked [`CHECKED_CONTEXT`] and each of `offered`'s gate.
pub(crate) fn add_contexts(key_context: &mut KeyContext, checked: bool, offered: &[BulkAction]) {
    key_context.add(CHECKABLE_CONTEXT);
    if checked {
        key_context.add(CHECKED_CONTEXT);
        for action in offered {
            key_context.add(context(*action));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BulkAction, add_contexts, context, gated};
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use gpui_kit::{KeyBindingContextPredicate, KeyContext};

    /// What the palette and the keymap test: a command matches the panel's
    /// contexts exactly when its action is offered for what's checked.
    #[test]
    fn a_command_matches_only_while_its_action_is_offered() {
        let offered = BulkAction::offered([&DiscoveredKind::pods()]);
        let mut checked = KeyContext::default();
        add_contexts(&mut checked, true, &offered);
        let mut unchecked = KeyContext::default();
        add_contexts(&mut unchecked, false, &offered);
        for action in BulkAction::ALL {
            let predicate = KeyBindingContextPredicate::parse(gated(action)).expect("parses");
            assert_eq!(
                predicate.eval(&[checked.clone()]),
                offered.contains(&action),
                "{action:?} with pods checked"
            );
            assert!(
                !predicate.eval(&[unchecked.clone()]),
                "{action:?} with nothing checked"
            );
        }
    }

    /// Each command's context is its gate's - the two lists can't drift.
    #[test]
    fn every_command_is_gated_by_the_context_its_panel_adds() {
        for action in BulkAction::ALL {
            assert_eq!(gated(action), format!("{} && !Input", context(action)));
        }
    }
}
