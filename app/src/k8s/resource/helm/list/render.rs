//! Drawing the Helm Releases list: the header with its namespace scope, the
//! search, the table, the hint row, and the states that stand in for rows -
//! connecting, loading, empty, and Helm's storage refused.

use super::commands::{
    DescribeRelease, FocusSearch, OPEN_KEY, OpenRelease, PANEL_KEY_CONTEXT, SEARCH_KEY,
    ToggleUninstalled,
};
use super::panel::HelmReleasesPanel;
use super::table::data_table;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::resource::helm::HelmScope;
use crate::k8s::resource::load_phase::LoadPhase;
use crate::ui::key_hint_row::{KeyHint, KeyHintRow};
use crate::ui::list_state::{self, TableArea};
use crate::ui::panel_title;
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The hint row's id, which keeps whether it is collapsed.
const HINTS_ID: &str = "helm-releases-hints";

/// Every identifier the panel's element carries, for `keymap::conflicts`
/// (#331): the panel's own and the sortable list's.
pub(crate) const KEY_CONTEXT_GROUP: &[&str] =
    &[PANEL_KEY_CONTEXT, crate::ui::list_sort::KEY_CONTEXT];

/// What the loading and empty states call this list's rows.
const KIND_LABEL: &str = "Helm releases";

/// The access message's title and detail for a refused `scope` (helm-releases
/// "Access to release storage"): it names the scope, and for every namespace
/// suggests choosing one.
pub(super) fn refusal_text(scope: &HelmScope, message: &str) -> (String, String) {
    match scope {
        HelmScope::All => (
            "Can't read Helm's storage across all namespaces".to_string(),
            format!(
                "Helm keeps releases in Secrets, and listing Secrets in every namespace was \
                 refused. Choose a namespace you may read with the namespace picker.\n\n{message}"
            ),
        ),
        HelmScope::Namespace(namespace) => (
            format!("Can't read Helm's storage in {namespace}"),
            format!(
                "Helm keeps releases in Secrets, and listing Secrets in {namespace} was \
                 refused.\n\n{message}"
            ),
        ),
    }
}

impl HelmReleasesPanel {
    fn hint(action: &dyn Action, fallback: &str, label: &'static str, window: &Window) -> KeyHint {
        KeyHint::for_action(action, Some(PANEL_KEY_CONTEXT), fallback, label, window)
    }

    fn content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let space = crate::ui::space::spacing(cx);
        let status = |text: String| div().size_full().p(space.panel_inset).child(text);
        match &self.connection.read(cx).state {
            ConnectionState::Connecting => {
                return status("Connecting...".into()).into_any_element();
            }
            ConnectionState::WaitingForTunnel => {
                return status("Waiting for tunnel...".into()).into_any_element();
            }
            ConnectionState::Failed(reason) => {
                return panel_title::error_content(
                    "Couldn't connect to the cluster".to_string(),
                    Some(reason.clone()),
                    cx,
                )
                .into_any_element();
            }
            ConnectionState::Connected(_) => {}
        }
        if let Some((scope, message)) = self.refusal(cx) {
            let (title, detail) = refusal_text(&scope, &message);
            return div()
                .debug_selector(|| "helm-releases-refused".into())
                .size_full()
                .child(panel_title::error_content(title, Some(detail), cx))
                .into_any_element();
        }
        let search = self.search_input(window, cx);
        let rows = self.visible_rows(cx);
        let total = self.scoped_rows(cx).len();
        let count = if self.search.query(cx).is_empty() {
            format!("{} releases", rows.len())
        } else {
            format!("{} of {total} releases", rows.len())
        };
        let area = list_state::table_area(
            self.phase(cx),
            total,
            rows.len(),
            KIND_LABEL,
            Some(&self.scope.namespaces),
        );
        let loading_shown = self.indicators.loading.ready(
            matches!(area, TableArea::Loading { .. }),
            |this: &mut Self| &mut this.indicators.loading,
            cx,
        );
        let table = self.sync_table(rows, window, cx);
        table.update(cx, |table, _| {
            table
                .delegate_mut()
                .set_no_match(area == TableArea::NoMatch);
        });
        let uninstalled = if self.show_uninstalled {
            "Hide uninstalled"
        } else {
            "Show uninstalled"
        };
        let hints = KeyHintRow::new(HINTS_ID)
            .hint_gap(space.control_gap)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .hint(Self::hint(&OpenRelease, OPEN_KEY, "Open", window).essential())
            .hint(Self::hint(&FocusSearch, SEARCH_KEY, "Search", window).essential())
            .hint(Self::hint(
                &crate::ui::namespace_picker::PickNamespaces,
                crate::ui::namespace_picker::PICK_NAMESPACES_KEY,
                "Namespaces",
                window,
            ))
            .hint(Self::hint(&ToggleUninstalled, "", uninstalled, window));
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .p(space.panel_inset)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space.control_gap)
                    .child(
                        div()
                            .flex_1()
                            .on_action(cx.listener(Self::on_action_clear_search))
                            .child(crate::ui::text_field::text_field(&search, cx)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .debug_selector(|| "helm-releases-count".into())
                            .child(count),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(list_state::table_area_element(
                        data_table(&table, cx).into_any_element(),
                        &area,
                        KIND_LABEL,
                        loading_shown,
                        cx,
                    )),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(crate::ui::style::surface_raised(cx))
                    .child(hints),
            )
            .into_any_element()
    }

    fn on_action_focus_search(
        &mut self,
        _: &FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search
            .focus(super::panel::SEARCH_PLACEHOLDER, window, cx);
    }

    /// Escape in the search box: clears it and hands focus back to the table.
    fn on_action_clear_search(
        &mut self,
        _: &gpui_kit::component::input::Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = match &self.table {
            Some(table) => table.read(cx).focus_handle(cx),
            None => self.focus_handle.clone(),
        };
        self.search.clear(&focus, window, cx);
        cx.notify();
    }

    /// Down with no row selected selects the first, Up the last, so the
    /// keyboard reaches the rows without tabbing into the table (`list_keys`).
    fn capture_select_down(
        &mut self,
        _: &gpui_kit::base::actions::SelectDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step(crate::ui::list_keys::Step::Down, window, cx);
    }

    fn capture_select_up(
        &mut self,
        _: &gpui_kit::base::actions::SelectUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step(crate::ui::list_keys::Step::Up, window, cx);
    }

    fn step(
        &mut self,
        step: crate::ui::list_keys::Step,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(table) = self.table.clone()
            && crate::ui::list_keys::step(&table, &self.focus_handle, step, window, cx)
        {
            let row_ix = table.read(cx).selected_row();
            if let Some(row_ix) = row_ix {
                table.update(cx, |table, _| {
                    table.delegate_mut().remember_selection(row_ix)
                });
            }
            cx.stop_propagation();
        }
    }

    fn on_action_toggle_uninstalled(
        &mut self,
        _: &ToggleUninstalled,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_uninstalled = !self.show_uninstalled;
        cx.notify();
    }
}

impl Render for HelmReleasesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let space = crate::ui::space::spacing(cx);
        let content = self.content(window, cx);
        let refreshing = matches!(self.phase(cx), LoadPhase::Refreshing { .. });
        let refreshing = self.indicators.refreshing.ready(
            refreshing,
            |this: &mut Self| &mut this.indicators.refreshing,
            cx,
        );
        let this = cx.weak_entity();
        let namespaces = self.namespaces.read(cx).names().to_vec();
        let namespace_bar = self.namespace_picker.element(
            &self.scope,
            &namespaces,
            move |namespaces, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.set_namespaces(namespaces, cx)
                });
            },
            window,
            cx,
        );
        let header = div()
            .flex()
            .items_center()
            .gap(space.control_gap)
            .px(space.panel_inset)
            .py(space.control_gap)
            .bg(crate::ui::style::surface_raised(cx))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(panel_title::context_label(
                &self.scope,
                cx.theme().muted_foreground,
            ))
            .when(refreshing, |header| {
                header.child(list_state::refreshing(cx))
            })
            .children(namespace_bar);
        let mut key_context = KeyContext::default();
        key_context.add(PANEL_KEY_CONTEXT);
        key_context.add(crate::ui::list_sort::KEY_CONTEXT);
        crate::ui::list_sort::on_sort_actions(div(), |this: &Self| this.table.clone(), cx)
            .size_full()
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .capture_action(cx.listener(Self::capture_select_down))
            .capture_action(cx.listener(Self::capture_select_up))
            .on_action(cx.listener(
                |this, _: &crate::ui::namespace_picker::PickNamespaces, window, cx| {
                    this.namespace_picker.open(window, cx)
                },
            ))
            .on_action(cx.listener(Self::on_action_focus_search))
            .on_action(cx.listener(Self::on_action_toggle_uninstalled))
            .on_action(
                cx.listener(|this, _: &OpenRelease, window, cx| this.open_selected(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &DescribeRelease, window, cx| this.open_selected(window, cx)),
            )
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(div().flex_1().min_h_0().child(content)),
            )
            .focus_trap("helm-releases-panel-tab-trap", &self.focus_handle)
    }
}
