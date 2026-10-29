//! What a list panel provides for bulk actions ([`BulkHost`]), and the action
//! handlers every such panel gets from it ([`on_bulk_actions`]), so the Pods
//! table and the generic list run the same flow over their own rows.

use super::commands::*;
use super::outcome::BatchOutcome;
use super::{CheckedRows, flow};
use crate::k8s::cluster::connection::ClusterConnection;
use crate::k8s::resource::bulk::{BulkAction, BulkTarget};
use crate::k8s::resource::pods::PodSelection;
use gpui_kit::*;

/// A panel's checked rows, ready to act on.
#[derive(Clone)]
pub(crate) struct BulkSelection {
    pub(crate) context_name: String,
    pub(crate) connection: Entity<ClusterConnection>,
    /// The checked objects, top to bottom.
    pub(crate) targets: Vec<BulkTarget>,
    /// The checked pods as the Pods table selects them, for View Logs; empty
    /// in any other list.
    pub(crate) pods: Vec<PodSelection>,
}

impl BulkSelection {
    /// The actions the checked set is offered.
    pub(crate) fn offered(&self) -> Vec<BulkAction> {
        BulkAction::offered(self.targets.iter().map(|target| &target.kind))
    }
}

/// A list panel whose rows can be checked and acted on together.
pub(crate) trait BulkHost: Sized + 'static {
    /// The checked rows, or `None` with none checked or no table yet.
    fn bulk_selection(&self, cx: &App) -> Option<BulkSelection>;

    /// Changes the checked set, and redraws.
    fn edit_checked(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut CheckedRows));

    /// `x` (`range` false) or Shift+`x` on the cursor's row.
    fn check_cursor_row(&mut self, range: bool, cx: &mut Context<Self>);

    /// Cmd/Ctrl+`a`: checks every row the table shows.
    fn check_all_rows(&mut self, cx: &mut Context<Self>);

    /// Where the last batch's outcome is shown from.
    fn bulk_outcome(&mut self) -> &mut Option<BatchOutcome>;
}

/// `div` with every bulk action's handler, each acting on `T`'s checked rows.
pub(crate) fn on_bulk_actions<T: BulkHost>(div: Div, cx: &mut Context<T>) -> Div {
    fn start<T: BulkHost>(
        action: BulkAction,
    ) -> impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static {
        move |this, window, cx| flow::start(action, this, window, cx)
    }
    macro_rules! bulk {
        ($div:expr, $($action:ident => $bulk:ident),* $(,)?) => {
            $div$(.on_action(cx.listener({
                let run = start::<T>(BulkAction::$bulk);
                move |this, _: &$action, window, cx| run(this, window, cx)
            })))*
        };
    }
    let div = div
        .on_action(
            cx.listener(|this: &mut T, _: &ToggleChecked, _, cx| this.check_cursor_row(false, cx)),
        )
        .on_action(
            cx.listener(|this: &mut T, _: &CheckRange, _, cx| this.check_cursor_row(true, cx)),
        )
        .on_action(cx.listener(|this: &mut T, _: &CheckAll, _, cx| this.check_all_rows(cx)))
        .on_action(cx.listener(|this: &mut T, _: &ClearChecked, _, cx| {
            this.edit_checked(cx, CheckedRows::clear)
        }));
    bulk!(div,
        BulkViewLogs => ViewLogs,
        BulkRestartRollout => RestartRollout,
        BulkRollbackRollout => RollbackRollout,
        BulkScale => Scale,
        BulkCordon => Cordon,
        BulkUncordon => Uncordon,
        BulkDrain => Drain,
        BulkLabel => Label,
        BulkAnnotate => Annotate,
        BulkCopyNames => CopyNames,
        BulkCopyYaml => CopyYaml,
        BulkDelete => Delete,
    )
}

/// Adds `this` panel's bulk key contexts (see [`super::commands::add_contexts`]).
pub(crate) fn add_key_contexts<T: BulkHost>(this: &T, key_context: &mut KeyContext, cx: &App) {
    let selection = this.bulk_selection(cx);
    let offered = selection
        .as_ref()
        .map(BulkSelection::offered)
        .unwrap_or_default();
    add_contexts(key_context, selection.is_some(), &offered);
}

/// What `this` panel draws above its table for bulk actions: the last batch's
/// outcome, then - while rows are checked - the bar.
pub(crate) fn render_bulk<T: BulkHost>(this: &mut T, cx: &mut Context<T>) -> Vec<AnyElement> {
    let mut elements = Vec::new();
    if let Some(outcome) = this.bulk_outcome().clone() {
        let panel = cx.weak_entity();
        elements.push(super::outcome::render(
            &outcome,
            move |_, cx| {
                let _ = panel.update(cx, |this, cx| {
                    *this.bulk_outcome() = None;
                    cx.notify();
                });
            },
            cx,
        ));
    }
    if let Some(selection) = this.bulk_selection(cx) {
        elements.push(super::bar::render(
            selection.targets.len(),
            &selection.offered(),
            cx,
        ));
    }
    elements
}
