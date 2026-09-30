//! Section 2.2/2.3: a live panel's collapse behavior - it needs a real
//! `ResourcePanel` (unlike `logic.rs`'s pure `group_kinds`), since collapse
//! state lives on the panel itself.

use super::super::category::Category;
use super::super::{ResourceState, api_version_label};
use super::{kind, stub_panel};
use gpui_kit::TestAppContext;

/// Section 2.2: collapsing a section hides its rows but keeps its row count -
/// `render_section`'s header always reports `section.total`, whether or not
/// the section is expanded.
#[gpui_kit::test]
async fn collapsing_a_section_keeps_reporting_its_count(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    let kinds = vec![kind("", "Pod"), kind("apps", "Deployment")];

    window
        .update(cx, |panel, _window, cx| {
            panel.state = ResourceState::Loaded(kinds.clone());
            panel.toggle_section(Category::Workloads, cx);
        })
        .unwrap();

    window
        .update(cx, |panel, _window, cx| {
            let sections = panel.visible_sections(cx);
            let workloads = sections
                .iter()
                .find(|section| section.category.to_string() == "Workloads")
                .expect("Workloads is still a section, just collapsed");
            assert_eq!(
                workloads.total, 2,
                "the header still reports how many kinds it holds"
            );
            assert!(!workloads.expanded, "rows are hidden while collapsed");
            assert!(
                panel.collapsed.contains(&Category::Workloads),
                "the click collapsed it"
            );
        })
        .unwrap();
}

/// Section 2.3: collapse state is per-panel, this window only - a freshly
/// constructed panel starts with every section expanded, even after another
/// panel collapsed one.
#[gpui_kit::test]
async fn a_freshly_constructed_panel_starts_fully_expanded(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let collapsed_elsewhere = stub_panel(cx);
    collapsed_elsewhere
        .update(cx, |panel, _window, cx| {
            panel.toggle_section(Category::Workloads, cx);
        })
        .unwrap();

    let fresh = stub_panel(cx);
    fresh
        .update(cx, |panel, _window, _cx| {
            assert!(
                panel.collapsed.is_empty(),
                "a new panel starts with every section expanded"
            );
        })
        .unwrap();
}

/// A row reads just its kind; the API group moves to the row's tooltip. Only kinds
/// that share a name in the same list keep their group, so their rows stay distinct.
#[gpui_kit::test]
async fn rows_name_the_kind_and_keep_the_group_only_for_shared_names(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    let kinds = vec![
        kind("apps", "Deployment"),
        kind("", "Event"),
        kind("events.k8s.io", "Event"),
    ];
    window
        .update(cx, |panel, _window, _cx| {
            let labels: Vec<String> = panel.rows(&kinds).into_iter().map(|row| row.0).collect();
            assert_eq!(labels, vec!["Deployment", "Event", "Event · events.k8s.io"]);
        })
        .unwrap();

    assert_eq!(api_version_label(&kind("apps", "Deployment")), "apps/v1");
    assert_eq!(api_version_label(&kind("", "Pod")), "v1 (core)");
}
