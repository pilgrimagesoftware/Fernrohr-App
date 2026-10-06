//! `open-in-background` 2.1-2.2 for a list panel: `cmd-enter` on the selected row,
//! a `cmd`-click, a middle-click and a `cmd`-double-click each ask for the row's
//! object in the background - and only that - with the selection left where it was;
//! and `cmd-enter` in the filter field asks for nothing.

use super::{Harness, deployments, focus_table, harness, object, press, record_opens};
use crate::ui::nav::OpenMode;
use gpui_kit::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext};

/// Three Deployments, the table focused, the first selected.
fn three(cx: &mut TestAppContext) -> Harness {
    let mut h = harness(
        cx,
        deployments(),
        vec![
            object("api", Some("shop")),
            object("cart", Some("shop")),
            object("web", Some("shop")),
        ],
    );
    focus_table(&mut h);
    press(&mut h.vcx, "down");
    assert_eq!(selected(&mut h), Some(0));
    h
}

fn selected(h: &mut Harness) -> Option<usize> {
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| panel.read(cx).selected_row(cx))
}

fn row_name(h: &mut Harness, row_ix: usize) -> String {
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        let table = panel.read(cx).table.clone().expect("drawn");
        table.read(cx).delegate().rows()[row_ix].object.name.clone()
    })
}

/// A click on `row_ix`'s name cell.
fn click(h: &mut Harness, row_ix: usize, button: MouseButton, modifiers: Modifiers, count: usize) {
    let selector: &'static str = format!("object-cell-{row_ix}-0").leak();
    let at = h
        .vcx
        .debug_bounds(selector)
        .expect("the row is drawn")
        .center();
    for click_count in 1..=count {
        h.vcx.simulate_event(MouseDownEvent {
            position: at,
            modifiers,
            button,
            click_count,
            first_mouse: false,
        });
        h.vcx.simulate_event(MouseUpEvent {
            position: at,
            modifiers,
            button,
            click_count,
        });
    }
    h.vcx.run_until_parked();
}

#[gpui_kit::test]
async fn cmd_enter_opens_the_selected_row_in_the_background(cx: &mut TestAppContext) {
    let opened = record_opens(cx);
    let mut h = three(cx);
    let name = row_name(&mut h, 0);

    press(&mut h.vcx, "secondary-enter");

    let opened = opened.borrow();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].target.name, name);
    assert_eq!(opened[0].mode, OpenMode::Background);
    assert_eq!(selected(&mut h), Some(0), "the selection didn't move");
}

#[gpui_kit::test]
async fn cmd_enter_in_the_filter_opens_nothing(cx: &mut TestAppContext) {
    let opened = record_opens(cx);
    let mut h = three(cx);
    press(&mut h.vcx, "/");
    press(&mut h.vcx, "secondary-enter");
    assert!(opened.borrow().is_empty(), "typing isn't intercepted");
}

#[gpui_kit::test]
async fn a_cmd_click_opens_that_row_and_keeps_the_selection(cx: &mut TestAppContext) {
    let opened = record_opens(cx);
    let mut h = three(cx);
    let name = row_name(&mut h, 2);

    click(&mut h, 2, MouseButton::Left, Modifiers::secondary_key(), 1);

    let opened = opened.borrow();
    assert_eq!(opened.len(), 1, "{opened:?}");
    assert_eq!(opened[0].target.name, name);
    assert_eq!(opened[0].mode, OpenMode::Background);
    assert_eq!(selected(&mut h), Some(0), "the first row stays selected");
}

#[gpui_kit::test]
async fn a_middle_click_opens_that_row_and_keeps_the_selection(cx: &mut TestAppContext) {
    let opened = record_opens(cx);
    let mut h = three(cx);
    let name = row_name(&mut h, 1);

    click(&mut h, 1, MouseButton::Middle, Modifiers::none(), 1);

    let opened = opened.borrow();
    assert_eq!(opened.len(), 1, "{opened:?}");
    assert_eq!(opened[0].target.name, name);
    assert_eq!(opened[0].mode, OpenMode::Background);
    assert_eq!(selected(&mut h), Some(0));
}

/// A double-click's second click is still a background open - not the plain
/// double-click's foreground one.
#[gpui_kit::test]
async fn a_cmd_double_click_only_opens_in_the_background(cx: &mut TestAppContext) {
    let opened = record_opens(cx);
    let mut h = three(cx);

    click(&mut h, 1, MouseButton::Left, Modifiers::secondary_key(), 2);

    let opened = opened.borrow();
    assert!(!opened.is_empty());
    assert!(
        opened.iter().all(|open| open.mode == OpenMode::Background),
        "{opened:?}"
    );
    assert_eq!(selected(&mut h), Some(0));
}
