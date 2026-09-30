//! Section 2.2/2.3: a live panel's collapse behavior - it needs a real
//! `ResourcePanel` (unlike `logic.rs`'s pure `group_kinds`), since collapse
//! state lives on the panel itself.

use super::super::ResourceState;
use super::super::category::Category;
use super::{kind, stub_panel};
use gpui_kit::TestAppContext;

/// Section 2.2: collapsing a section hides its rows but keeps its row count -
/// `render_section`'s header always reports `section.kinds.len()`, whether or
/// not the section is expanded.
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
        .update(cx, |panel, _window, _cx| {
            let sections = panel.sections();
            let workloads = sections
                .iter()
                .find(|section| section.category.to_string() == "Workloads")
                .expect("Workloads is still a section, just collapsed");
            assert_eq!(
                workloads.kinds.len(),
                2,
                "the header still reports how many kinds it holds"
            );
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
