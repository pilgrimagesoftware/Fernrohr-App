//! `window-title-and-menu` 1.2: the exact titles, app-name casing included
//! (design.md Risks, last bullet).

// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use super::{initial_title, title_for, title_from};
use crate::util::shell::MainWindow;
use gpui_kit::{TestAppContext, WindowHandle};

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

#[test]
fn picker_is_the_app_name_alone() {
    assert_eq!(title_from(&[], 0), "Fernrohr");
}

#[test]
fn one_context_names_it() {
    assert_eq!(title_from(&names(&["staging"]), 0), "staging - Fernrohr");
}

#[test]
fn several_contexts_give_the_count_not_a_name() {
    let contexts = names(&["staging", "production", "kind-dev"]);
    // Whichever is active, the title is the count: naming one of several
    // would read as a window showing only that cluster.
    for active in 0..contexts.len() {
        let title = title_from(&contexts, active);
        assert_eq!(title, "3 clusters - Fernrohr");
        assert!(contexts.iter().all(|name| !title.contains(name.as_str())));
    }
    assert_eq!(
        title_from(&names(&["staging", "production"]), 1),
        "2 clusters - Fernrohr"
    );
}

#[test]
fn a_stale_active_index_falls_back_to_the_app_name() {
    assert_eq!(title_from(&names(&["staging"]), 1), "Fernrohr");
}

/// `title_for` reads the real `WindowMode` the way production will.
fn title_of(window: &WindowHandle<MainWindow>, cx: &mut TestAppContext) -> String {
    window
        .update(cx, |main_window, _, _| title_for(&main_window.mode))
        .unwrap()
}

fn init(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
}

#[gpui_kit::test]
fn picker_window_is_titled_with_the_app_name(cx: &mut TestAppContext) {
    init(cx);
    let window = cx.add_window(MainWindow::test_picker_window);
    assert_eq!(title_of(&window, cx), "Fernrohr");
}

#[gpui_kit::test]
fn workspace_windows_are_titled_from_their_contexts(cx: &mut TestAppContext) {
    init(cx);
    let one =
        cx.add_window(|window, cx| MainWindow::test_workspace(names(&["staging"]), window, cx));
    assert_eq!(title_of(&one, cx), "staging - Fernrohr");

    let two = cx.add_window(|window, cx| {
        MainWindow::test_workspace(names(&["staging", "production"]), window, cx)
    });
    assert_eq!(title_of(&two, cx), "2 clusters - Fernrohr");
}

/// 2.1: the title `open_window` passes before the mode exists is the one
/// `title_for` gives once the window has entered its workspace, so a restored
/// window's first-frame title is never replaced by a different one.
#[gpui_kit::test]
fn initial_title_matches_the_entered_workspace(cx: &mut TestAppContext) {
    init(cx);
    for contexts in [
        names(&[]),
        names(&["staging"]),
        names(&["staging", "production"]),
    ] {
        let initial = initial_title(&contexts);
        let window = if contexts.is_empty() {
            cx.add_window(MainWindow::test_picker_window)
        } else {
            let entered = contexts.clone();
            cx.add_window(move |window, cx| MainWindow::test_workspace(entered, window, cx))
        };
        assert_eq!(title_of(&window, cx), initial, "contexts: {contexts:?}");
    }
    assert_eq!(
        initial_title(&names(&["staging", "production"])),
        "2 clusters - Fernrohr"
    );
}
