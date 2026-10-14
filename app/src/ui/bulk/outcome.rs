//! How a bulk action went, shown above the list until dismissed: how many
//! checked objects it succeeded for, and each failure by name with the server's
//! reason - never one pass or fail for the whole batch.
//!
//! Dismiss is an icon button with a tooltip (`icon-buttons.md`), like a single
//! action's refusal banner (`delete_flow::refusal`).

use crate::k8s::resource::bulk::ObjectOutcome;
use crate::k8s::resource::resource_actions::ActionFailure;
use crate::ui::bulk::RowKey;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::IconName;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The banner's selector and its Dismiss button's id.
pub(crate) const OUTCOME_SELECTOR: &str = "bulk-outcome";
pub(crate) const OUTCOME_DISMISS_ID: &str = "bulk-outcome-dismiss";

/// One object, or one part of one (a drained node's pod), that failed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FailedObject {
    /// `shop/web-1`, or `node-a: evict shop/web-1`.
    pub(crate) name: String,
    pub(crate) failure: ActionFailure,
}

/// A finished batch.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BatchOutcome {
    /// The action's label: "Delete".
    pub(crate) action: &'static str,
    /// The checked objects it fully succeeded for.
    pub(crate) succeeded: Vec<RowKey>,
    pub(crate) failed: Vec<FailedObject>,
    /// How many objects it was sent for, while it is still running.
    pub(crate) running: Option<usize>,
}

impl BatchOutcome {
    /// `outcomes` summed up per checked object: an object succeeded when every
    /// one of its outcomes did.
    pub(crate) fn of(action: &'static str, outcomes: &[ObjectOutcome]) -> Self {
        let mut succeeded: Vec<RowKey> = Vec::new();
        let mut failed_keys: Vec<RowKey> = Vec::new();
        let mut failed = Vec::new();
        for outcome in outcomes {
            let key = RowKey::new(outcome.target.namespace.as_deref(), &outcome.target.name);
            match &outcome.result {
                Ok(()) => {
                    if !succeeded.contains(&key) {
                        succeeded.push(key);
                    }
                }
                Err(failure) => {
                    let name = match &outcome.part {
                        Some(part) => format!("{}: {part}", outcome.target.display_name()),
                        None => outcome.target.display_name(),
                    };
                    failed.push(FailedObject {
                        name,
                        failure: failure.clone(),
                    });
                    failed_keys.push(key);
                }
            }
        }
        succeeded.retain(|key| !failed_keys.contains(key));
        Self {
            action,
            succeeded,
            failed,
            running: None,
        }
    }

    /// A batch of `total` objects, sent and not yet answered.
    pub(crate) fn running(action: &'static str, total: usize) -> Self {
        Self {
            action,
            succeeded: Vec::new(),
            failed: Vec::new(),
            running: Some(total),
        }
    }

    /// "Delete: 4 succeeded, 1 failed", "Delete: 4 succeeded", or while it
    /// runs "Delete: running for 5…".
    pub(crate) fn summary(&self) -> String {
        if let Some(total) = self.running {
            return format!("{}: running for {total}…", self.action);
        }
        let succeeded = self.succeeded.len();
        if self.failed.is_empty() {
            format!("{}: {succeeded} succeeded", self.action)
        } else {
            format!(
                "{}: {succeeded} succeeded, {} failed",
                self.action,
                self.failed.len()
            )
        }
    }
}

/// `outcome` as a banner; Dismiss runs `on_dismiss`.
pub(crate) fn render(
    outcome: &BatchOutcome,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    use crate::ui::style::{Tone, status};
    let space = crate::ui::space::spacing(cx);
    let tone = if outcome.running.is_some() {
        Tone::Info
    } else if outcome.failed.is_empty() {
        Tone::Good
    } else {
        Tone::Bad
    };
    let color = status(tone, cx);
    let muted = cx.theme().muted_foreground;
    let failures = outcome.failed.iter().map(|failed| {
        div()
            .flex()
            .flex_col()
            .child(format!("{}: {}", failed.name, failed.failure.message))
            .when(failed.failure.detail != failed.failure.message, |line| {
                line.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(failed.failure.detail.clone()),
                )
            })
    });
    div()
        .id(OUTCOME_SELECTOR)
        .debug_selector(|| OUTCOME_SELECTOR.into())
        .flex()
        .items_start()
        .gap(space.control_gap)
        .mb(space.control_gap)
        .px(space.panel_inset)
        .py(space.control_gap)
        .rounded_md()
        .border_1()
        .border_color(color)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_color(color).child(outcome.summary()))
                .children(failures),
        )
        .child(
            Button::new(OUTCOME_DISMISS_ID)
                .icon(IconName::Close)
                .xsmall()
                .ghost()
                .tooltip(crate::k8s::resource::delete_flow::refusal::DISMISS_TOOLTIP)
                .on_click(move |_event, window, cx| on_dismiss(window, cx)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests;
