//! `window-title-and-menu` 1.2: the exact titles, app-name casing included
//! (design.md Risks, last bullet).

// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use super::{initial_title, title_for, title_from};
use crate::util::shell::test_support::pods_panel_descriptor;
use crate::util::shell::{MainWindow, WindowLayout, WindowMode, open_window};
use gpui_kit::component::Root;
use gpui_kit::{TestAppContext, VisualTestContext, WindowHandle};

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

/// What the window last passed to `set_window_title`. `Window::window_title`
/// reads the platform back and is empty on the test platform, which only
/// stores the title for `VisualTestContext` to read.
fn window_title(window: &WindowHandle<MainWindow>, cx: &mut TestAppContext) -> Option<String> {
    VisualTestContext::from_window((*window).into(), cx).window_title()
}

/// 3.1-3.3: both `apply` paths reach the OS window - `enter_workspace` and
/// `enter_picker` directly, an edit to `contexts` through
/// `sync_context_children`'s deferred one.
#[gpui_kit::test]
fn the_window_title_follows_its_mode(cx: &mut TestAppContext) {
    init(cx);
    let window =
        cx.add_window(|window, cx| MainWindow::test_workspace(names(&["staging"]), window, cx));
    assert_eq!(
        window_title(&window, cx).as_deref(),
        Some("staging - Fernrohr")
    );

    window
        .update(cx, |main_window, _, cx| {
            if let WindowMode::Workspace { contexts, .. } = &mut main_window.mode {
                contexts.push("production".to_string());
            }
            main_window.sync_context_children(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        window_title(&window, cx).as_deref(),
        Some("2 clusters - Fernrohr")
    );

    window
        .update(cx, |main_window, window, cx| {
            main_window.enter_picker(window, cx)
        })
        .unwrap();
    assert_eq!(window_title(&window, cx).as_deref(), Some("Fernrohr"));
}

/// The same deferred re-title through the real `open_window`, whose root view
/// is `Root` wrapping the `MainWindow` rather than the `MainWindow` itself.
#[gpui_kit::test]
fn a_window_opened_for_real_is_retitled(cx: &mut TestAppContext) {
    init(cx);
    let layout = WindowLayout {
        contexts: names(&["staging"]),
        panels: vec![pods_panel_descriptor("staging")],
        ..Default::default()
    };
    cx.update(|cx| open_window(cx, layout));
    cx.run_until_parked();
    let handle = cx.update(|cx| cx.windows()[0]);
    let mut vcx = VisualTestContext::from_window(handle, cx);
    assert_eq!(vcx.window_title().as_deref(), Some("staging - Fernrohr"));

    let main_window = vcx.update(|window, cx| {
        let root = window.root::<Root>().flatten().expect("a Root window");
        root.read(cx)
            .view()
            .clone()
            .downcast::<MainWindow>()
            .unwrap()
    });
    vcx.update(|_, cx| {
        main_window.update(cx, |main_window, cx| {
            if let WindowMode::Workspace { contexts, .. } = &mut main_window.mode {
                contexts.push("production".to_string());
            }
            main_window.sync_context_children(cx);
        });
    });
    vcx.run_until_parked();
    assert_eq!(vcx.window_title().as_deref(), Some("2 clusters - Fernrohr"));
}
