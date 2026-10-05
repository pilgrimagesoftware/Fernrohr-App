//! `open-in-background` 2.1-2.2 for the Pods panel: `cmd-enter`, a `cmd`-click and
//! a middle-click ask for a pod's detail in the background - never through
//! `ShowPodDetail` - and leave the selected pod, `SelectedPod`, where it was;
//! `cmd-enter` in the namespace filter asks for nothing.

use crate::k8s::resource::pods::test_window::{Harness, open};
use crate::ui::nav::OpenPodInBackground;
use gpui_kit::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext};
use std::cell::RefCell;
use std::rc::Rc;

/// Every background pod open the panel asks the window for.
fn record(harness: &mut Harness) -> Rc<RefCell<Vec<String>>> {
    let opened = Rc::new(RefCell::new(Vec::new()));
    harness.vcx.update(|_, cx| {
        let opened = opened.clone();
        cx.on_action(move |action: &OpenPodInBackground, _| {
            opened.borrow_mut().push(action.name.clone())
        });
    });
    opened
}

/// The displayed row `name` is on.
fn row_of(harness: &mut Harness, name: &str) -> usize {
    let panel = harness.panel.clone();
    harness.vcx.update(|_, cx| {
        let table = panel.read(cx).pod_table.clone().expect("drawn");
        table
            .read(cx)
            .delegate()
            .rows()
            .iter()
            .position(|row| row.selection.name == name)
            .expect("listed")
    })
}

fn click(harness: &mut Harness, name: &str, button: MouseButton, modifiers: Modifiers) {
    let row_ix = row_of(harness, name);
    let selector: &'static str = format!("pod-cell-{row_ix}-0").leak();
    let at = harness
        .vcx
        .debug_bounds(selector)
        .expect("the row is drawn")
        .center();
    harness.vcx.simulate_event(MouseDownEvent {
        position: at,
        modifiers,
        button,
        click_count: 1,
        first_mouse: false,
    });
    harness.vcx.simulate_event(MouseUpEvent {
        position: at,
        modifiers,
        button,
        click_count: 1,
    });
    harness.vcx.run_until_parked();
}

#[gpui_kit::test]
async fn cmd_enter_opens_the_selected_pod_in_the_background(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    let opened = record(&mut harness);

    harness.press("secondary-enter");

    assert_eq!(*opened.borrow(), ["web-1"]);
    assert_eq!(harness.details_opened.get(), 0, "not a foreground open");
    assert_eq!(harness.selected().as_deref(), Some("web-1"));
}

#[gpui_kit::test]
async fn cmd_enter_in_the_namespace_filter_opens_nothing(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    let opened = record(&mut harness);
    harness.press("n");
    let typing = harness.vcx.update(|window, _| {
        window
            .context_stack()
            .iter()
            .any(|context| context.contains("Input"))
    });
    assert!(typing, "`n` focuses the namespace filter");

    harness.press("secondary-enter");

    assert!(opened.borrow().is_empty());
}

#[gpui_kit::test]
async fn a_cmd_click_opens_that_pod_and_keeps_the_selection(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    let opened = record(&mut harness);

    click(
        &mut harness,
        "web-2",
        MouseButton::Left,
        Modifiers::secondary_key(),
    );

    assert_eq!(*opened.borrow(), ["web-2"]);
    assert_eq!(
        harness.selected().as_deref(),
        Some("web-1"),
        "SelectedPod - which Logs follows - didn't move"
    );
}

#[gpui_kit::test]
async fn a_middle_click_opens_that_pod_and_keeps_the_selection(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    let opened = record(&mut harness);

    click(
        &mut harness,
        "web-2",
        MouseButton::Middle,
        Modifiers::none(),
    );

    assert_eq!(*opened.borrow(), ["web-2"]);
    assert_eq!(harness.selected().as_deref(), Some("web-1"));
}
