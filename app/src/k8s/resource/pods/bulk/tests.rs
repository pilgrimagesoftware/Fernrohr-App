//! Checking rows and acting on them in a real Pods panel - the app's keymap,
//! with the bulk actions bound to keys in a test `keymap.toml` (they have none
//! by default) - over a fake cluster that records deletes. Every route is a
//! real keystroke or click.

use crate::k8s::resource::pods::test_window::{Harness, open_with};
use crate::ui::bulk::bar::BAR_SELECTOR;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext};

/// The bulk actions' keys for these tests.
const KEYMAP: &str = r#"[bindings]
"bulk.delete" = "f13"
"bulk.copy_names" = "f14"
"#;

fn open(cx: &mut TestAppContext, count: usize) -> Harness {
    let pods: Vec<(String, String)> = (0..count)
        .map(|n| (format!("u{n}"), format!("web-{n}")))
        .collect();
    let pods: Vec<(&str, &str, bool)> = pods
        .iter()
        .map(|(uid, name)| (uid.as_str(), name.as_str(), false))
        .collect();
    open_with(cx, &pods, KEYMAP)
}

fn checked(harness: &mut Harness) -> Vec<String> {
    harness.vcx.update(|_, cx| {
        let panel = harness.panel.read(cx);
        let table = panel.pod_table.as_ref().expect("built").read(cx);
        table
            .delegate()
            .checked_rows()
            .iter()
            .map(|row| row.selection.name.clone())
            .collect()
    })
}

fn row_of(harness: &mut Harness, name: &str) -> usize {
    harness.vcx.update(|_, cx| {
        let panel = harness.panel.read(cx);
        let table = panel.pod_table.as_ref().expect("built").read(cx);
        table
            .delegate()
            .rows()
            .iter()
            .position(|row| row.selection.name == name)
            .expect("listed")
    })
}

/// Clicks `name`'s checkbox, holding `modifiers`.
fn click_checkbox(harness: &mut Harness, name: &str, modifiers: Modifiers) {
    let row_ix = row_of(harness, name);
    harness.vcx.update(|window, cx| window.render_frame(cx));
    let selector: &'static str = format!("pod-check-{row_ix}").leak();
    let at = harness
        .vcx
        .debug_bounds(selector)
        .expect("the checkbox is drawn")
        .center();
    harness.vcx.simulate_event(MouseDownEvent {
        position: at,
        modifiers,
        button: MouseButton::Left,
        click_count: 1,
        first_mouse: false,
    });
    harness.vcx.simulate_event(MouseUpEvent {
        position: at,
        modifiers,
        button: MouseButton::Left,
        click_count: 1,
    });
    harness.vcx.run_until_parked();
}

fn bar_drawn(harness: &mut Harness) -> bool {
    harness.vcx.update(|window, cx| window.render_frame(cx));
    harness.vcx.debug_bounds(BAR_SELECTOR).is_some()
}

fn dialog_open(harness: &mut Harness) -> bool {
    harness
        .vcx
        .update(|window, cx| window.has_active_dialog(cx))
}

fn wait_for_deletes(harness: &mut Harness, count: usize) -> Vec<String> {
    for _ in 0..400 {
        if harness.cluster.deletes().len() >= count {
            break;
        }
        harness.vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut names: Vec<String> = harness
        .cluster
        .deletes()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    names.sort();
    names
}

/// Task 1.3: `x` checks the cursor's row and leaves the cursor where it is; a
/// second `x` unchecks it.
#[gpui_kit::test]
async fn x_checks_the_cursors_row_and_again_unchecks_it(cx: &mut TestAppContext) {
    let mut harness = open(cx, 3);
    harness.press("x");
    assert_eq!(checked(&mut harness), ["web-0"]);
    assert_eq!(harness.selected().as_deref(), Some("web-0"), "cursor kept");
    assert!(bar_drawn(&mut harness), "the bar shows while checked");

    harness.press("x");
    assert!(checked(&mut harness).is_empty());
    assert!(!bar_drawn(&mut harness), "and goes with nothing checked");
}

/// Task 1.2: clicking another row's checkbox checks it without moving the
/// cursor off its own row.
#[gpui_kit::test]
async fn clicking_a_checkbox_checks_without_moving_the_cursor(cx: &mut TestAppContext) {
    let mut harness = open(cx, 3);
    click_checkbox(&mut harness, "web-2", Modifiers::none());
    assert_eq!(checked(&mut harness), ["web-2"]);
    assert_eq!(harness.selected().as_deref(), Some("web-0"));
}

/// Task 1.4: Shift+`x` checks from the last checked row to the cursor's, 3
/// through 7.
#[gpui_kit::test]
async fn shift_x_checks_the_range_from_the_last_checked_row(cx: &mut TestAppContext) {
    let mut harness = open(cx, 10);
    for _ in 0..3 {
        harness.press("down");
    }
    harness.press("x");
    for _ in 0..4 {
        harness.press("down");
    }
    harness.press("shift-x");
    assert_eq!(
        checked(&mut harness),
        ["web-3", "web-4", "web-5", "web-6", "web-7"]
    );
}

/// The mouse twin: Shift-click on a checkbox extends from the last check.
#[gpui_kit::test]
async fn shift_click_extends_from_the_last_check(cx: &mut TestAppContext) {
    let mut harness = open(cx, 10);
    click_checkbox(&mut harness, "web-3", Modifiers::none());
    click_checkbox(&mut harness, "web-7", Modifiers::shift());
    assert_eq!(
        checked(&mut harness),
        ["web-3", "web-4", "web-5", "web-6", "web-7"]
    );
}

/// Task 1.5: checked rows are kept by identity - a sort that reorders the
/// table keeps the same pods checked.
#[gpui_kit::test]
async fn a_reorder_keeps_the_same_pods_checked(cx: &mut TestAppContext) {
    let mut harness = open(cx, 4);
    harness.press("x");
    harness.press("down");
    harness.press("down");
    harness.press("x");
    assert_eq!(checked(&mut harness), ["web-0", "web-2"]);
    harness.vcx.update(|_, cx| {
        let table = harness.panel.read(cx).pod_table.clone().expect("built");
        table.update(cx, |table, cx| {
            table
                .delegate_mut()
                .resort(0, gpui_kit::component::table::ColumnSort::Descending);
            cx.notify();
        });
    });
    harness.vcx.run_until_parked();
    assert_eq!(row_of(&mut harness, "web-0"), 3, "the order changed");
    assert_eq!(checked(&mut harness), ["web-2", "web-0"]);
}

/// Task 7.1: two checked pods, bulk Delete, confirmed with the irreversible
/// shortcut, sends both deletes - and unchecks what it deleted.
#[gpui_kit::test]
async fn bulk_delete_asks_then_deletes_every_checked_pod(cx: &mut TestAppContext) {
    let mut harness = open(cx, 3);
    harness.press("x");
    harness.press("down");
    harness.press("x");

    harness.press("f13");
    assert!(dialog_open(&mut harness), "Delete asks first");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("enter");
    assert!(
        harness.cluster.deletes().is_empty(),
        "Enter is Cancel's: an irreversible confirmation opens on Cancel"
    );

    harness.press("f13");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("secondary-backspace");
    assert_eq!(wait_for_deletes(&mut harness, 2), ["web-0", "web-1"]);
    harness.wait_for("unchecked the deleted pods", |panel, cx| {
        let table = panel.pod_table.as_ref().expect("built").read(cx);
        table.delegate().checked_rows().is_empty()
    });
}

/// Task 7.1's second test: Escape cancels, sends nothing, and leaves every
/// checked row checked.
#[gpui_kit::test]
async fn canceling_bulk_delete_sends_nothing_and_keeps_the_rows_checked(cx: &mut TestAppContext) {
    let mut harness = open(cx, 3);
    harness.press("x");
    harness.press("down");
    harness.press("x");
    harness.press("f13");
    harness.press("escape");
    assert!(!dialog_open(&mut harness));
    assert!(harness.cluster.deletes().is_empty());
    assert_eq!(checked(&mut harness), ["web-0", "web-1"]);
}

/// The single-row `ctrl-d` keeps acting on the cursor's row alone, whatever
/// else is checked.
#[gpui_kit::test]
async fn ctrl_d_deletes_only_the_cursors_row_with_others_checked(cx: &mut TestAppContext) {
    let mut harness = open(cx, 3);
    harness.press("down");
    harness.press("x");
    harness.press("down");
    harness.press("x");
    harness.press("up");
    harness.press("up");
    assert_eq!(harness.selected().as_deref(), Some("web-0"));

    harness.press("ctrl-d");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("secondary-backspace");
    assert_eq!(wait_for_deletes(&mut harness, 1), ["web-0"]);
    assert_eq!(checked(&mut harness), ["web-1", "web-2"]);
}

/// Tasks 6.2 and 7.6: Copy Names copies the checked pods' names, one per
/// line, at once - no confirmation.
#[gpui_kit::test]
async fn copy_names_copies_one_name_per_line_without_asking(cx: &mut TestAppContext) {
    let mut harness = open(cx, 4);
    harness.press("x");
    harness.press("down");
    harness.press("x");
    harness.press("down");
    harness.press("x");
    harness.press("f14");
    assert!(!dialog_open(&mut harness), "no confirmation");
    let copied = harness
        .vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some("web-0\nweb-1\nweb-2"));
}

/// #308: Cmd/Ctrl+`a` checks every row, leaving the cursor where it is.
#[gpui_kit::test]
async fn select_all_checks_every_row(cx: &mut TestAppContext) {
    let mut harness = open(cx, 3);
    harness.press("down");
    harness.press("secondary-a");
    assert_eq!(checked(&mut harness), ["web-0", "web-1", "web-2"]);
    assert_eq!(harness.selected().as_deref(), Some("web-1"), "cursor kept");
    assert!(bar_drawn(&mut harness));
}

/// In the filter box, Cmd/Ctrl+`a` is the text field's own Select All: it
/// checks nothing.
#[gpui_kit::test]
async fn select_all_in_the_filter_checks_nothing(cx: &mut TestAppContext) {
    let mut harness = open(cx, 3);
    harness.press("/");
    harness.vcx.simulate_input("web");
    harness.press("secondary-a");
    assert!(checked(&mut harness).is_empty());
}

/// With a filter applied, Select All checks only the rows the filter shows.
#[gpui_kit::test]
async fn select_all_checks_only_the_filtered_rows(cx: &mut TestAppContext) {
    let mut harness = open(cx, 12);
    harness.press("/");
    harness.vcx.simulate_input("web-1");
    harness.vcx.run_until_parked();
    let panel = harness.panel.clone();
    harness.vcx.update(|window, cx| {
        let table = panel.read(cx).pod_table.clone().expect("built");
        let focus = gpui_kit::Focusable::focus_handle(table.read(cx), cx);
        window.focus(&focus, cx);
    });
    harness.press("secondary-a");
    assert_eq!(checked(&mut harness), ["web-1", "web-10", "web-11"]);
}

/// The mouse route: a row's context menu ends with Check All Rows.
#[gpui_kit::test]
async fn the_row_menus_check_all_rows_checks_every_row(cx: &mut TestAppContext) {
    let mut harness = open(cx, 3);
    harness.vcx.update(|window, cx| window.render_frame(cx));
    let row = harness
        .vcx
        .debug_bounds("pod-cell-1-0")
        .expect("the row is drawn")
        .center();
    harness.vcx.simulate_event(MouseDownEvent {
        position: row,
        modifiers: Modifiers::none(),
        button: MouseButton::Right,
        click_count: 1,
        first_mouse: false,
    });
    harness.vcx.simulate_event(MouseUpEvent {
        position: row,
        modifiers: Modifiers::none(),
        button: MouseButton::Right,
        click_count: 1,
    });
    harness.vcx.run_until_parked();
    // Check All Rows is last: Up wraps to it.
    harness.press("up");
    harness.press("enter");
    assert_eq!(checked(&mut harness), ["web-0", "web-1", "web-2"]);
}
