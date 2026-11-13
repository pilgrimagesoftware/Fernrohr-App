//! #357: a context's folded groups outlive the panel that folded them - a
//! panel opened on that context, or switched to it, starts with them, and a
//! different context keeps its own.

use super::super::category::Category;
use super::super::{ResourcePanel, ResourceState};
use super::keyboard_panel::bind_panel_keys;
use super::kind;
use crate::config::workspace::ResourceGroups;
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::ResourceGroupMemory;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext, WindowHandle};

/// A panel on `context`, wired to a stub connection like `stub_panel`.
fn panel_on(cx: &mut TestAppContext, context: &str) -> WindowHandle<ResourcePanel> {
    let connection =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    let context = context.to_string();
    cx.add_window(|window, cx| {
        ResourcePanel::with_connection(
            context.clone(),
            vec![context.clone(), "other".to_string()],
            connection.clone(),
            window,
            cx,
        )
    })
}

fn init(cx: &mut TestAppContext) {
    bind_panel_keys(cx);
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        // Registered, so switching to either never starts a real connection.
        for context in ["demo", "other"] {
            ClusterRegistry::insert_test_session(cx, context, ConnectionState::Connecting);
        }
    });
}

/// Every section has a stable key that names it back, so a saved fold
/// survives a retitled section.
#[test]
fn every_section_round_trips_through_its_key() {
    for category in Category::ALL {
        assert_eq!(Category::from_key(category.key()), Some(category));
    }
    assert_eq!(Category::from_key("no-such-section"), None);
}

/// Left on a Workloads row collapses Workloads, and the fold is recorded for
/// the panel's context: a new panel there starts collapsed, one on another
/// context does not.
#[gpui_kit::test]
async fn a_keyboard_fold_carries_to_the_next_panel_on_that_context(cx: &mut TestAppContext) {
    init(cx);
    let pod = kind("", "Pod");
    let first = panel_on(cx, "demo");
    first
        .update(cx, |panel, window, cx| {
            panel.state = ResourceState::Loaded(vec![pod.clone()]);
            panel.focus_handle.clone().focus(window, cx);
            cx.notify();
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(first.into(), cx);
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down left");

    let remembered = vcx.update(|_, cx| ResourceGroupMemory::get(cx, "demo"));
    assert_eq!(
        remembered,
        Some(ResourceGroups {
            collapsed_sections: vec!["workloads".into()],
            expanded_subgroups: vec![],
        })
    );

    let again = panel_on(cx, "demo");
    let elsewhere = panel_on(cx, "other");
    let collapsed = |handle: WindowHandle<ResourcePanel>, cx: &mut TestAppContext| {
        handle
            .update(cx, |panel, _, _| panel.is_section_collapsed(&pod))
            .unwrap()
    };
    assert!(collapsed(again, cx), "the same context reopens folded");
    assert!(!collapsed(elsewhere, cx), "another context keeps its own");
}

/// Switching a panel's context swaps in that context's folds, and switching
/// back brings the first one's back. Expanding to the default forgets the
/// context rather than storing an empty entry.
#[gpui_kit::test]
async fn switching_context_swaps_the_folds(cx: &mut TestAppContext) {
    init(cx);
    let panel = panel_on(cx, "demo");
    let contexts = vec!["demo".to_string(), "other".to_string()];
    panel
        .update(cx, |panel, _, cx| {
            panel.toggle_section(Category::Network, cx);
            panel.toggle_subgroup("example.io", cx);
            panel.set_active_context("other".into(), contexts.clone(), cx);
            assert!(panel.collapsed.is_empty(), "other has no folds");
            assert!(panel.expanded_subgroups.is_empty());
            panel.set_active_context("demo".into(), contexts.clone(), cx);
            assert_eq!(panel.collapsed, [Category::Network].into_iter().collect());
            assert_eq!(
                panel.expanded_subgroups,
                ["example.io".to_string()].into_iter().collect()
            );

            panel.toggle_section(Category::Network, cx);
            panel.toggle_subgroup("example.io", cx);
        })
        .unwrap();
    let remembered = cx.update(|cx| ResourceGroupMemory::get(cx, "demo"));
    assert_eq!(remembered, None, "back at the default, nothing is kept");
}
