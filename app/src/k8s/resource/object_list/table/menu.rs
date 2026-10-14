//! A list row's context menu: Open and Open in New Panel, then the actions the
//! row's kind and object are offered (`resource-specific-actions`), which
//! dispatch their commands from the panel's focus, so each runs as its key
//! does. Right-clicking a row selects it first, so those commands act on it.

use super::{ListRow, ObjectTableDelegate};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::table::TableState;
use gpui_kit::*;
use std::rc::Rc;

/// A row's kind-action items: each label and the action it dispatches.
pub(super) type KindMenu = Rc<dyn Fn(&ListRow) -> Vec<(&'static str, Box<dyn Action>)>>;

impl ObjectTableDelegate {
    /// What a row's menu offers beyond Open, and the focus it dispatches from.
    pub(in crate::k8s::resource::object_list) fn set_kind_menu(
        &mut self,
        focus: FocusHandle,
        items: impl Fn(&ListRow) -> Vec<(&'static str, Box<dyn Action>)> + 'static,
    ) {
        self.action_context = Some(focus);
        self.kind_menu = Some(Rc::new(items));
    }

    pub(super) fn row_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some(row) = self.rows.get(row_ix).cloned() else {
            return menu;
        };
        cx.defer_in(window, move |table, _window, cx| {
            table.set_selected_row(row_ix, cx);
        });
        let mut menu = match self.on_open.clone() {
            Some(on_open) => menu.item(
                PopupMenuItem::new("Open")
                    .on_click(move |_event, window, cx| on_open(row_ix, window, cx)),
            ),
            None => menu,
        };
        if let Some(open) = self.on_open_in_new_panel.clone() {
            let row = row.clone();
            menu = menu.item(
                PopupMenuItem::new("Open in New Panel")
                    .on_click(move |_event, window, cx| open(&row, window, cx)),
            );
        }
        let (Some(items), Some(focus)) = (self.kind_menu.clone(), self.action_context.clone())
        else {
            return menu;
        };
        let items = items(&row);
        if items.is_empty() {
            return menu;
        }
        let mut menu = menu.separator().action_context(focus);
        for (label, action) in items {
            menu = menu.menu(label, action);
        }
        menu
    }
}
