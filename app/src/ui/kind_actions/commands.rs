//! The kind-specific actions as registered commands. Each is gated by its own
//! key context, which a list or detail panel adds only while its row or object
//! is offered that action ([`add_contexts`]) - so the palette shows Resume only
//! for a suspended CronJob, and no key fires on a kind it means nothing for.
//!
//! A Services list keeps its own `shift-f` command, so Port Forward's context is
//! added only by a Service's detail panel ([`add_contexts`]'s `in_list`).

use crate::command::{Command, CommandRegistry};
use crate::k8s::resource::kind_actions::KindAction;
use gpui_kit::{Action, KeyContext, actions};

actions!(
    kind_action,
    [
        ForwardService,
        TriggerNow,
        Suspend,
        Resume,
        PauseRollout,
        ResumeRollout,
        Expand,
        AddTaint,
        RemoveTaint,
        CreateToken,
        OpenHost
    ]
);

/// `action`'s command: id, palette title, default key, gating context (its
/// context, outside a text field), and action.
pub(crate) struct KindCommand {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) key: &'static str,
    /// The key context that gates it.
    pub(crate) gate: &'static str,
    pub(crate) context: &'static str,
}

pub(crate) fn command(action: KindAction) -> KindCommand {
    let (id, title, key, gate, context) = match action {
        KindAction::ForwardService => (
            "kind.port_forward_service",
            "Service: Port Forward…",
            "shift-f",
            "KindForwardService",
            "KindForwardService && !Input",
        ),
        KindAction::TriggerNow => (
            "kind.trigger_now",
            "CronJob: Trigger Now…",
            "t",
            "KindTriggerNow",
            "KindTriggerNow && !Input",
        ),
        KindAction::Suspend => (
            "kind.suspend",
            "Suspend…",
            "p",
            "KindSuspend",
            "KindSuspend && !Input",
        ),
        KindAction::Resume => (
            "kind.resume",
            "Resume…",
            "shift-p",
            "KindResume",
            "KindResume && !Input",
        ),
        KindAction::PauseRollout => (
            "kind.pause_rollout",
            "Deployment: Pause Rollout…",
            "p",
            "KindPauseRollout",
            "KindPauseRollout && !Input",
        ),
        KindAction::ResumeRollout => (
            "kind.resume_rollout",
            "Deployment: Resume Rollout…",
            "shift-p",
            "KindResumeRollout",
            "KindResumeRollout && !Input",
        ),
        KindAction::Expand => (
            "kind.expand",
            "PersistentVolumeClaim: Expand…",
            "shift-e",
            "KindExpand",
            "KindExpand && !Input",
        ),
        KindAction::AddTaint => (
            "kind.add_taint",
            "Node: Add Taint…",
            "shift-t",
            "KindAddTaint",
            "KindAddTaint && !Input",
        ),
        KindAction::RemoveTaint => (
            "kind.remove_taint",
            "Node: Remove Taint…",
            "alt-shift-t",
            "KindRemoveTaint",
            "KindRemoveTaint && !Input",
        ),
        KindAction::CreateToken => (
            "kind.create_token",
            "ServiceAccount: Create Token",
            "shift-k",
            "KindCreateToken",
            "KindCreateToken && !Input",
        ),
        KindAction::OpenHost => (
            "kind.open_host",
            "Ingress: Open Host in Browser",
            "shift-o",
            "KindOpenHost",
            "KindOpenHost && !Input",
        ),
    };
    KindCommand {
        id,
        title,
        key,
        gate,
        context,
    }
}

/// `action` as the action its command dispatches.
pub(crate) fn action(action: KindAction) -> Box<dyn Action> {
    match action {
        KindAction::ForwardService => Box::new(ForwardService),
        KindAction::TriggerNow => Box::new(TriggerNow),
        KindAction::Suspend => Box::new(Suspend),
        KindAction::Resume => Box::new(Resume),
        KindAction::PauseRollout => Box::new(PauseRollout),
        KindAction::ResumeRollout => Box::new(ResumeRollout),
        KindAction::Expand => Box::new(Expand),
        KindAction::AddTaint => Box::new(AddTaint),
        KindAction::RemoveTaint => Box::new(RemoveTaint),
        KindAction::CreateToken => Box::new(CreateToken),
        KindAction::OpenHost => Box::new(OpenHost),
    }
}

pub fn register_commands(registry: &mut CommandRegistry) {
    for kind_action in KindAction::ALL {
        let command = command(kind_action);
        registry.register(Command {
            id: command.id,
            title: command.title,
            default_binding: command.key,
            context: Some(command.context),
            action: action(kind_action),
            menu: None,
        });
    }
}

/// Adds the gate of each of `offered`. In a list, Port Forward is the
/// Services list's own command, so its gate isn't added there.
pub(crate) fn add_contexts(key_context: &mut KeyContext, offered: &[KindAction], in_list: bool) {
    for kind_action in offered {
        if in_list && *kind_action == KindAction::ForwardService {
            continue;
        }
        key_context.add(command(*kind_action).gate);
    }
}

#[cfg(test)]
mod tests {
    use super::{KindAction, add_contexts, command};
    use gpui_kit::{KeyBindingContextPredicate, KeyContext};

    /// Each command's context is its gate's, and it matches only while its
    /// action is offered.
    #[test]
    fn a_command_matches_only_while_its_action_is_offered() {
        for offered in KindAction::ALL {
            let mut context = KeyContext::default();
            add_contexts(&mut context, &[offered], false);
            for kind_action in KindAction::ALL {
                let command = command(kind_action);
                assert_eq!(command.context, format!("{} && !Input", command.gate));
                let predicate = KeyBindingContextPredicate::parse(command.context).unwrap();
                assert_eq!(
                    predicate.eval(&[context.clone()]),
                    kind_action == offered,
                    "{kind_action:?} with {offered:?} offered"
                );
            }
        }
    }

    #[test]
    fn a_list_leaves_port_forward_to_its_own_command() {
        let mut context = KeyContext::default();
        add_contexts(&mut context, &[KindAction::ForwardService], true);
        assert!(!context.contains("KindForwardService"));
    }
}
