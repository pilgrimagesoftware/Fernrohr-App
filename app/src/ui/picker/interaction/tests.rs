// `use super::*` here would re-import `gpui_kit`'s `test` attribute macro (this file's
// `use gpui_kit::*` brings it in), shadowing the builtin `#[test]` and sending a plain
// sync test into `#[gpui_kit::test]`'s async-runtime expansion instead - hence the
// explicit imports below rather than a glob.
use crate::ui::picker::ClusterPicker;

/// A connect stub shared by the tests below: hands back a connection that never
/// leaves `Connecting`, so `handle_row_click`/`confirm_row`/`connect_selected` can
/// be exercised without any real I/O (a real connect spawns tokio work on a
/// runtime worker thread, which gpui's test scheduler flags as nondeterminism).
fn stub_connecting(
    cx: &mut gpui_kit::App,
    _context_name: &str,
) -> gpui_kit::Entity<crate::k8s::cluster::connection::ClusterConnection> {
    use gpui_kit::AppContext as _;
    cx.new(|_| {
        crate::k8s::cluster::connection::ClusterConnection::test_with_state(
            crate::k8s::cluster::connection::ConnectionState::Connecting,
        )
    })
}

/// "The context picker should not connect on a single click" - a single click
/// (click count 1) only moves the highlight, so [`ClusterPicker::connect_button`]
/// and `Enter` have something to act on; it must not itself start a connection.
#[gpui_kit::test]
async fn a_single_click_selects_without_connecting(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, _cx| {
            picker.contexts = Ok(vec!["kind-dev".to_string(), "staging".to_string()]);
        })
        .unwrap();

    window
        .update(cx, |picker, window, cx| {
            picker.handle_row_click("staging".to_string(), 1, 1, window, cx);
        })
        .unwrap();

    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(picker.selected_context.as_deref(), Some("staging"));
            assert!(
                picker.attempt.is_none(),
                "a single click must not start a connection"
            );
        })
        .unwrap();
}

/// A double click (click count 2) on a row connects immediately, matching
/// familiar file-manager conventions.
#[gpui_kit::test]
async fn a_double_click_connects(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, cx| {
            picker.contexts = Ok(vec!["kind-dev".to_string()]);
            picker.connection_factory = Some(stub_connecting);
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |picker, window, cx| {
            picker.handle_row_click("kind-dev".to_string(), 0, 2, window, cx);
        })
        .unwrap();

    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(
                picker
                    .attempt
                    .as_ref()
                    .expect("a double click should start a connection")
                    .context_name,
                "kind-dev"
            );
        })
        .unwrap();
}

/// [`ClusterPicker::connect_button`] connects the current selection through
/// `connect_selected`, and is disabled - a no-op if a click still reaches it -
/// with nothing selected.
#[gpui_kit::test]
async fn connect_selected_connects_and_is_a_no_op_without_a_selection(
    cx: &mut gpui_kit::TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, cx| {
            picker.contexts = Ok(vec!["kind-dev".to_string()]);
            picker.connection_factory = Some(stub_connecting);
            picker.selected_context = None;
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |picker, _window, cx| {
            assert!(
                picker.selected_context.is_none() && !picker.is_connect_in_flight(cx),
                "the Connect button should render disabled with nothing selected"
            );
            picker.connect_selected(cx);
        })
        .unwrap();
    window
        .update(cx, |picker, _window, _cx| {
            assert!(
                picker.attempt.is_none(),
                "connecting with nothing selected must be a no-op"
            );
        })
        .unwrap();

    window
        .update(cx, |picker, _window, cx| {
            picker.selected_context = Some("kind-dev".to_string());
            picker.connect_selected(cx);
        })
        .unwrap();
    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(
                picker
                    .attempt
                    .as_ref()
                    .expect("the Connect button should connect the selected context")
                    .context_name,
                "kind-dev"
            );
        })
        .unwrap();
}

/// `Enter` reaches `confirm_row` through `Command`'s own confirm action and the
/// `on_confirm` wired in `render` (see the module doc comment) - connecting the
/// highlighted row exactly as the Connect button and a double click do.
#[gpui_kit::test]
async fn confirm_row_connects_the_highlighted_context(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, cx| {
            picker.contexts = Ok(vec!["kind-dev".to_string(), "staging".to_string()]);
            picker.connection_factory = Some(stub_connecting);
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |picker, _window, cx| picker.confirm_row(1, cx))
        .unwrap();

    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(
                picker
                    .attempt
                    .as_ref()
                    .expect("Enter should connect the highlighted row")
                    .context_name,
                "staging"
            );
        })
        .unwrap();
}

/// Hover moves `Command`'s own highlight (its `select`), which must not move what
/// Connect targets: only a click selects. A fresh picker starts with nothing
/// selected, so Connect is disabled until the user picks a row.
#[gpui_kit::test]
async fn hover_highlight_never_changes_the_selection(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::component::IndexPath;

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(picker.selected_context, None, "nothing selected at start");
            picker.contexts = Ok(vec!["kind-dev".to_string(), "staging".to_string()]);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |picker, window, cx| {
            picker.handle_row_click("kind-dev".to_string(), 0, 1, window, cx);
            // What a hover over the second row does inside `Command`.
            picker.command_state.update(cx, |state, cx| {
                state.set_selected_index(Some(IndexPath::new(1)), window, cx)
            });
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(picker.selected_context.as_deref(), Some("kind-dev"));
        })
        .unwrap();
}
