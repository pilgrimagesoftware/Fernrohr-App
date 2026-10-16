//! The entry dialog by real keystrokes (`exec-command-library` 3.2): created
//! from the window's list with the keyboard alone; a taken name and a command
//! that doesn't split refused inline, with nothing saved or run; and Escape
//! closing it with focus back on the list.

use super::render::error_selector;
use crate::config::exec_library::{Field, Target};
use crate::ui::exec_library::test_support::{dialog_open, entry, open, press};
use gpui_kit::{TestAppContext, VisualTestContext};

/// Whether the library window's list has focus.
fn list_focused(
    harness: &mut crate::ui::exec_library::test_support::Harness,
    window: &mut VisualTestContext,
) -> bool {
    let list = harness.vcx.update(|_, cx| {
        crate::ui::exec_library::window::test_view(cx)
            .expect("open")
            .read(cx)
            .list_focus
            .clone()
    });
    window.update(|window, _| list.is_focused(window))
}

fn drawn(window: &mut VisualTestContext, selector: String) -> bool {
    use gpui_kit::test::TestWindowExt as _;
    window.update(|window, cx| window.render_frame(cx));
    window.debug_bounds(selector.leak()).is_some()
}

/// Spec "Create without the mouse" (from the window): `n`, a name, Tab, a
/// command, Enter - saved, and focus is back on the list.
#[gpui_kit::test]
async fn an_entry_is_created_without_the_mouse(cx: &mut TestAppContext) {
    let mut harness = open(cx, vec![entry("Uptime", "uptime", Target::Any)]);
    let mut window = harness.open_library();
    press(&mut window, "tab");
    press(&mut window, "n");
    assert!(dialog_open(&mut window), "New opens the dialog");
    window.simulate_input("Stream info");
    press(&mut window, "tab");
    window.simulate_input("nats stream info X");
    press(&mut window, "enter");
    assert!(!dialog_open(&mut window), "saved and closed");
    assert_eq!(harness.names(), ["Uptime", "Stream info"]);
    assert!(
        list_focused(&mut harness, &mut window),
        "focus is back on the list"
    );
}

/// Spec "Duplicate name".
#[gpui_kit::test]
async fn a_taken_name_is_refused_beside_the_name(cx: &mut TestAppContext) {
    let mut harness = open(cx, vec![entry("Stream info", "date", Target::Any)]);
    let mut window = harness.open_library();
    press(&mut window, "tab");
    press(&mut window, "n");
    window.simulate_input(" stream INFO ");
    press(&mut window, "tab");
    window.simulate_input("uptime");
    press(&mut window, "enter");
    assert!(dialog_open(&mut window), "still open");
    assert!(
        drawn(&mut window, error_selector(Field::Name)),
        "the error is beside Name"
    );
    assert_eq!(harness.names(), ["Stream info"], "nothing saved");
}

/// Spec "Unbalanced quote": Save and Save and Run are both refused.
#[gpui_kit::test]
async fn an_unbalanced_quote_refuses_save_and_save_and_run(cx: &mut TestAppContext) {
    let mut harness = open(cx, Vec::new());
    let mut window = harness.open_library();
    press(&mut window, "tab");
    press(&mut window, "n");
    window.simulate_input("Echo");
    press(&mut window, "tab");
    window.simulate_input("echo \"unterminated");
    press(&mut window, "enter");
    assert!(
        drawn(&mut window, error_selector(Field::Command)),
        "Save refused"
    );
    press(&mut window, "secondary-enter");
    assert!(dialog_open(&mut window), "Save and Run refused too");
    harness.settle();
    assert!(harness.names().is_empty(), "nothing saved");
    assert!(harness.sessions().is_empty(), "nothing run");
}

/// Escape closes the dialog and puts focus back where it was.
#[gpui_kit::test]
async fn escape_closes_the_dialog_with_focus_back(cx: &mut TestAppContext) {
    let mut harness = open(cx, vec![entry("Uptime", "uptime", Target::Any)]);
    let mut window = harness.open_library();
    press(&mut window, "tab");
    press(&mut window, "e");
    assert!(dialog_open(&mut window), "Edit opens it");
    press(&mut window, "escape");
    assert!(!dialog_open(&mut window));
    assert!(
        list_focused(&mut harness, &mut window),
        "focus is back on the list"
    );
    assert_eq!(harness.names(), ["Uptime"], "nothing changed");
}
