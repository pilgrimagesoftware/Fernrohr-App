//! The mouse routes to a kind's actions: icon buttons with tooltips in a detail
//! panel's header (`icon-buttons.md`), and items in a list row's context menu.
//! Both dispatch the action's registered command, so they run exactly what its
//! key does.

use super::commands::{action, command};
use crate::k8s::resource::kind_actions::KindAction;
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;
use std::rc::Rc;

use super::host::{KindHost, KindTarget};

fn icon(kind_action: KindAction) -> IconName {
    match kind_action {
        KindAction::ForwardService => IconName::ArrowLeftRight,
        KindAction::TriggerNow => IconName::Play,
        KindAction::Suspend | KindAction::PauseRollout => IconName::Pause,
        KindAction::Resume | KindAction::ResumeRollout => IconName::CirclePlay,
        KindAction::Expand => IconName::ArrowUpFromLine,
        KindAction::AddTaint => IconName::ShieldPlus,
        KindAction::RemoveTaint => IconName::ShieldMinus,
        KindAction::CreateToken => IconName::KeyRound,
        KindAction::OpenHost => IconName::ExternalLink,
    }
}

/// `kind_action`'s button id: `kind-trigger_now`.
pub(crate) fn button_id(kind_action: KindAction) -> SharedString {
    format!(
        "kind-{}",
        command(kind_action).id.trim_start_matches("kind.")
    )
    .into()
}

/// One icon button per offered action, its tooltip naming the action and its
/// key from the live keymap.
pub(crate) fn buttons(offered: &[KindAction]) -> Vec<AnyElement> {
    offered
        .iter()
        .map(|kind_action| {
            let dispatched = action(*kind_action);
            Button::new(button_id(*kind_action))
                .icon(icon(*kind_action))
                .xsmall()
                .ghost()
                .tooltip_with_action(kind_action.label(), &*action(*kind_action), None)
                .on_click(move |_, window, cx| window.dispatch_action(dispatched.boxed_clone(), cx))
                .into_any_element()
        })
        .collect()
}

/// What a row menu item runs.
pub(crate) type RunEntry = Rc<dyn Fn(&mut Window, &mut App)>;

/// One row menu item: its label, the action whose key it shows, and what it
/// runs - on the row the menu opened on, whatever is selected by the click.
pub(crate) struct MenuEntry {
    pub(crate) label: &'static str,
    pub(crate) action: Box<dyn Action>,
    pub(crate) run: RunEntry,
}

/// A row menu's items for what `target` is offered, each acting on `target`
/// through `panel`.
pub(crate) fn menu_entries<T: KindHost>(
    target: KindTarget,
    panel: WeakEntity<T>,
) -> Vec<MenuEntry> {
    target
        .offered()
        .into_iter()
        .map(|kind_action| {
            let (target, panel) = (target.clone(), panel.clone());
            MenuEntry {
                label: kind_action.label(),
                action: action(kind_action),
                run: Rc::new(move |window, cx| {
                    let _ = panel.update(cx, |this, cx| {
                        super::flow::run_on(kind_action, target.clone(), this, window, cx)
                    });
                }),
            }
        })
        .collect()
}

/// The hint row's entry for each of `offered`: its key from the live keymap,
/// and its label.
pub(crate) fn hints(
    offered: &[KindAction],
    window: &Window,
) -> Vec<crate::ui::key_hint_row::KeyHint> {
    offered
        .iter()
        .map(|kind_action| {
            let command = command(*kind_action);
            crate::ui::key_hint_row::KeyHint::for_action(
                &*action(*kind_action),
                Some(command.gate),
                command.key,
                kind_action.label(),
                window,
            )
        })
        .collect()
}
