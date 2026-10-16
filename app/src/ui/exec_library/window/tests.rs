//! The Exec Library window by real keystrokes (`exec-command-library` 3.1):
//! opened from a Pods panel, typing filters and Enter runs in that panel's
//! context; Delete asks with focus on Cancel, so Enter keeps the entry; with no
//! cluster panel behind it, Run asks which context; and the toolbar's icons
//! name their actions and keys in tooltips.

use crate::config::exec_library::Target;
use crate::ui::exec_library::test_support::{dialog_open, entry, open, press, selector};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, TestAppContext, VisualContext as _};

fn library() -> Vec<crate::config::exec_library::Entry> {
    vec![
        entry("Uptime", "uptime", Target::Any),
        entry("Stream info", "nats stream info X", selector("app=api")),
    ]
}

/// Spec "Run from the keyboard".
#[gpui_kit::test]
async fn typing_part_of_a_name_and_enter_runs_it_in_the_panels_context(cx: &mut TestAppContext) {
    let mut harness = open(cx, library());
    let mut window = harness.open_library();
    let context = harness.vcx.update(|_, cx| {
        super::test_view(cx)
            .expect("open")
            .read(cx)
            .opener
            .context
            .clone()
    });
    assert_eq!(context.as_deref(), Some("demo"), "the Pods panel's context");

    window.simulate_input("stream");
    press(&mut window, "enter");
    harness.settle();
    let sessions = harness.sessions();
    assert_eq!(sessions.len(), 1, "it ran");
    assert_eq!(sessions[0].pod, "api-1");
    assert_eq!(
        sessions[0]
            .command
            .as_ref()
            .map(|command| command.entry_name.as_str()),
        Some("Stream info")
    );
}

/// Spec "Delete needs a deliberate confirmation": Enter in the confirmation
/// cancels; the deliberate shortcut deletes.
#[gpui_kit::test]
async fn delete_needs_a_deliberate_confirmation(cx: &mut TestAppContext) {
    let mut harness = open(cx, library());
    let mut window = harness.open_library();
    press(&mut window, "tab");
    press(&mut window, "secondary-backspace");
    assert!(dialog_open(&mut window), "it asks first");
    crate::ui::confirm_dialog::deliver_first_frame(&mut window);
    press(&mut window, "enter");
    assert!(!dialog_open(&mut window), "Enter closes it");
    assert_eq!(
        harness.names(),
        ["Uptime", "Stream info"],
        "and keeps the entry"
    );

    press(&mut window, "secondary-backspace");
    crate::ui::confirm_dialog::deliver_first_frame(&mut window);
    press(&mut window, "secondary-backspace");
    assert_eq!(
        harness.names(),
        ["Stream info"],
        "the deliberate key deletes"
    );
}

/// With no cluster panel behind it, Run asks which connected context to use.
#[gpui_kit::test]
async fn with_no_cluster_panel_run_asks_for_the_context(cx: &mut TestAppContext) {
    let mut harness = open(cx, library());
    let main = harness.vcx.window_handle();
    harness.vcx.update(|_, cx| {
        super::open_or_focus(
            super::Opener {
                main: Some(main),
                context: None,
            },
            cx,
        )
    });
    harness.settle();
    let handle = harness
        .vcx
        .update(|_, cx| super::test_handle(cx))
        .expect("open");
    let mut window = gpui_kit::VisualTestContext::from_window(handle.into(), &harness.vcx);
    window.simulate_input("stream");
    press(&mut window, "enter");
    assert!(dialog_open(&mut window), "it asks which context");
    press(&mut window, "enter");
    harness.settle();
    assert_eq!(harness.sessions().len(), 1, "run in the picked context");
}

/// `icon-buttons.md`: each toolbar icon names its action in a tooltip.
#[gpui_kit::test]
async fn the_toolbar_icons_name_their_actions(cx: &mut TestAppContext) {
    let mut harness = open(cx, library());
    let mut window = harness.open_library();
    for (name, label) in [
        ("run", "Run"),
        ("new", "New entry"),
        ("delete", "Delete entry"),
    ] {
        let id: &'static str = super::render::toolbar_id(name).to_string().leak();
        let at = window.update(|window, cx| {
            window.render_frame(cx);
            window.find(id).bounds().center()
        });
        window.simulate_mouse_move(at, None, Modifiers::none());
        window
            .executor()
            .advance_clock(std::time::Duration::from_secs(1));
        window.run_until_parked();
        let selector: &'static str = crate::ui::icon_tooltip::selector(label).leak();
        assert!(
            window.debug_bounds(selector).is_some(),
            "{label}'s tooltip shows"
        );
    }
}
