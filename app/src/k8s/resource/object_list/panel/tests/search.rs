//! `list-search` #189's regression for the object list: its filter box runs
//! through the shared `crate::ui::list_search::ListSearch`, so this covers what
//! [`super::the_filter_narrows_rows_by_keyboard_and_escape_clears_it`] does not -
//! matching a column other than Name, case-insensitivity, and the hint row's
//! key tracking a `keymap.toml` override rather than only its fallback.

use super::{Harness, deployments, focus_table, harness, harness_overriding_keymap, object, press};
use crate::k8s::resource::object_list::commands::{FocusFilter, PANEL_KEY_CONTEXT};
use gpui_kit::TestAppContext;
use gpui_kit::component::kbd::Kbd;

fn row_namespaces(h: &mut Harness) -> Vec<Option<String>> {
    h.vcx.update(|_, cx| {
        let table = h.panel.read(cx).table.clone().expect("the table is drawn");
        table
            .read(cx)
            .delegate()
            .rows()
            .iter()
            .map(|row| row.object.namespace.clone())
            .collect()
    })
}

/// Namespace is a visible column but not Name: filtering by its text (in a
/// different case) narrows rows the same way, proving the shared matcher
/// checks every visible column, not name alone.
#[gpui_kit::test]
async fn filtering_by_a_non_name_column_is_case_insensitive(cx: &mut TestAppContext) {
    let mut h = harness(
        cx,
        deployments(),
        vec![
            object("web", Some("production")),
            object("api", Some("staging")),
        ],
    );
    focus_table(&mut h);

    press(&mut h.vcx, "/");
    h.vcx.simulate_input("STAGING");
    h.vcx.run_until_parked();

    assert_eq!(
        row_namespaces(&mut h),
        [Some("staging".to_string())],
        "matched the namespace column despite the different case"
    );
}

/// A `keymap.toml` override of `object_list.focus_filter` rebinds `/`: the old
/// default no longer focuses the box, the new key does, and the hint row -
/// which reads the same live keymap through `Kbd::binding_for_action` - has a
/// key to show.
#[gpui_kit::test]
async fn the_filter_hint_follows_a_keymap_override(cx: &mut TestAppContext) {
    let mut h = harness_overriding_keymap(
        cx,
        deployments(),
        vec![object("web", Some("production"))],
        None,
        &[("object_list.focus_filter", "ctrl-/")],
    );
    focus_table(&mut h);

    press(&mut h.vcx, "/");
    let focused_by_default_key = h
        .vcx
        .update(|window, cx| h.panel.read(cx).filter_focused(window, cx));
    assert!(
        !focused_by_default_key,
        "the default key no longer focuses the filter box once rebound"
    );

    press(&mut h.vcx, "ctrl-/");
    let focused_by_override = h
        .vcx
        .update(|window, cx| h.panel.read(cx).filter_focused(window, cx));
    assert!(focused_by_override, "the rebound key does");

    let hinted = h
        .vcx
        .update(|window, _| Kbd::binding_for_action(&FocusFilter, Some(PANEL_KEY_CONTEXT), window));
    assert!(hinted.is_some(), "the hint row has a live key to show");
}
