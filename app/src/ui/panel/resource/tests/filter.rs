//! Section 3: the live filter box wired to a real panel - `logic.rs` already
//! covers `matches_filter`/`visible_sections` as pure functions; this pins
//! that `ResourcePanel::filter_text`/`visible_sections` actually read the
//! `InputState` those functions are handed. Real keystrokes (`/`, typing,
//! Escape) are section 4's `keyboard_panel.rs`, once the panel has a key
//! context to dispatch them into.

use super::super::ResourceState;
use super::{kind, stub_panel};
use gpui_kit::TestAppContext;

/// Setting the filter box's text narrows `visible_sections` to the matching
/// kind, and clearing it restores every kind.
#[gpui_kit::test]
async fn setting_the_filter_narrows_visible_sections(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    let kinds = vec![kind("", "Pod"), kind("", "Service")];

    window
        .update(cx, |panel, window, cx| {
            panel.state = ResourceState::Loaded(kinds.clone());
            panel
                .filter_input
                .update(cx, |input, cx| input.set_value("service", window, cx));
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |panel, _window, cx| {
            let sections = panel.visible_sections(cx);
            let matched: Vec<String> = sections
                .iter()
                .flat_map(|section| section.matches.iter())
                .map(|kind| kind.label())
                .collect();
            assert_eq!(matched, vec!["Service"], "typing narrowed the list");
        })
        .unwrap();

    window
        .update(cx, |panel, window, cx| {
            panel
                .filter_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |panel, _window, cx| {
            let sections = panel.visible_sections(cx);
            let matched: usize = sections.iter().map(|section| section.matches.len()).sum();
            assert_eq!(matched, 2, "clearing the filter restores every kind");
        })
        .unwrap();
}

/// Section 3.3: a filter matching no kind leaves nothing for `render` to
/// show but the empty-result message - `render_kinds`'s `sections.is_empty()`
/// branch, pinned here at the state it reads rather than the text it draws.
#[gpui_kit::test]
async fn filtering_to_a_substring_no_kind_contains_shows_no_sections(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    let kinds = vec![kind("", "Pod"), kind("", "Service")];

    window
        .update(cx, |panel, window, cx| {
            panel.state = ResourceState::Loaded(kinds);
            panel.filter_input.update(cx, |input, cx| {
                input.set_value("zzz-no-such-kind", window, cx)
            });
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |panel, _window, cx| {
            assert!(
                panel.visible_sections(cx).is_empty(),
                "no section holds a match, so none should render"
            );
        })
        .unwrap();
}
