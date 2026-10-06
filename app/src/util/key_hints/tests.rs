//! The key hints overlay in the real window, the app's keymap bound: `?` with a
//! Pods panel focused lists exactly what the palette would offer there - its
//! Pods commands among them - shows a command whose key was removed as unbound
//! rather than leaving it out, and Escape hands focus back to the panel.

use super::{row_selector, rows};
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::keymap::KeymapConfig;
use crate::util::shell::{MainWindow, init};
use gpui_kit::AppContext as _;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{FocusHandle, TestAppContext, VisualTestContext};
use std::collections::BTreeMap;

/// A window with Pods shown and focused, `keymap.toml` holding `overrides`.
fn window_with_pods(cx: &mut TestAppContext, overrides: &[(&str, &str)]) -> VisualTestContext {
    cx.executor().allow_parking();
    let workspace = crate::util::test_paths::temp_path("key-hints-workspace");
    let keymap = crate::util::test_paths::temp_path("key-hints-keymap");
    if !overrides.is_empty() {
        let config = KeymapConfig {
            bindings: overrides
                .iter()
                .map(|(id, keys)| (id.to_string(), keys.to_string()))
                .collect::<BTreeMap<_, _>>(),
        };
        crate::config::save(&keymap, &config).expect("wrote keymap.toml");
    }
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    // In a `Root`, as every app window is: the overlay is one of its dialogs.
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| main.update(cx, |main, cx| main.test_focus(window, cx)));
    vcx.simulate_keystrokes("cmd-1");
    vcx.run_until_parked();
    vcx
}

fn focused(vcx: &mut VisualTestContext) -> Option<FocusHandle> {
    vcx.update(|window, cx| window.focused(cx))
}

fn palette_ids(vcx: &mut VisualTestContext) -> Vec<&'static str> {
    vcx.update(|window, cx| {
        let contexts: Vec<gpui_kit::SharedString> = window
            .context_stack()
            .iter()
            .filter_map(|context| context.primary().map(|entry| entry.key.clone()))
            .collect();
        let names: Vec<&str> = contexts.iter().map(|name| name.as_ref()).collect();
        cx.global::<CommandRegistry>()
            .available(&names)
            .iter()
            .map(|command| command.id)
            .collect()
    })
}

/// 7.1: `?` opens the overlay over the Pods panel, listing the palette's
/// commands for that focus - Pods-scoped ones included - each with its key.
#[gpui_kit::test]
async fn key_hints_list_what_the_palette_offers_here(cx: &mut TestAppContext) {
    let mut vcx = window_with_pods(cx, &[]);
    let listed = vcx.update(|window, cx| rows(window, cx));
    let ids: Vec<&str> = listed.iter().map(|row| row.id).collect();
    assert_eq!(ids, palette_ids(&mut vcx), "the palette's set");
    assert!(
        ids.contains(&"pods.quick_look"),
        "Pods commands, with Pods focused"
    );
    let quick_look = listed
        .iter()
        .find(|row| row.id == "pods.quick_look")
        .unwrap();
    assert_eq!(quick_look.key.as_deref(), Some("space"));

    vcx.simulate_keystrokes("?");
    vcx.run_until_parked();
    assert!(
        vcx.update(|window, cx| window.has_active_dialog(cx)),
        "`?` opens it"
    );
    vcx.update(|window, cx| {
        use gpui_kit::test::TestWindowExt as _;
        window.render_frame(cx);
    });
    for id in ["pods.quick_look", "global.show_key_hints"] {
        let selector: &'static str = Box::leak(row_selector(id).into_boxed_str());
        assert!(vcx.debug_bounds(selector).is_some(), "{id} is listed");
    }
}

/// 7.2: a command whose key was removed - the keybindings editor's Remove, an
/// empty entry in keymap.toml - is listed as unbound, not left out.
#[gpui_kit::test]
async fn an_unbound_command_is_still_listed(cx: &mut TestAppContext) {
    let mut vcx = window_with_pods(cx, &[("pods.describe", "")]);
    let listed = vcx.update(|window, cx| rows(window, cx));
    let describe = listed
        .iter()
        .find(|row| row.id == "pods.describe")
        .expect("still listed");
    assert_eq!(describe.key, None, "as unbound");
}

/// 7.3: dismissing the overlay returns focus to where it was.
#[gpui_kit::test]
async fn dismissing_returns_focus(cx: &mut TestAppContext) {
    let mut vcx = window_with_pods(cx, &[]);
    let before = focused(&mut vcx).expect("the Pods panel has focus");

    vcx.simulate_keystrokes("?");
    vcx.run_until_parked();
    assert_ne!(
        focused(&mut vcx).as_ref(),
        Some(&before),
        "the overlay took focus"
    );
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();

    assert!(
        !vcx.update(|window, cx| window.has_active_dialog(cx)),
        "closed"
    );
    assert_eq!(focused(&mut vcx), Some(before), "focus is back");
}

/// `?` is a character in a text field: the overlay is offered - and bound -
/// only outside one, so typing `?` into a filter types it.
#[test]
fn the_overlay_is_not_offered_while_typing() {
    let mut registry = CommandRegistry::new();
    super::register_commands(&mut registry);
    let offered = |contexts: &[&str]| {
        registry
            .available(contexts)
            .iter()
            .any(|command| command.id == super::SHOW_KEY_HINTS_COMMAND_ID)
    };
    assert!(offered(&["PodsPanel"]));
    assert!(!offered(&["ObjectListPanel", "Input"]));
}
