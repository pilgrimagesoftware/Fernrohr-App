//! Section 4: the Resource panel driven the way `.claude/rules/keyboard-first.md`
//! requires tests to - real keystrokes through `VisualTestContext`, not direct
//! handler calls. Sections 2.2/2.3's collapse-count/no-leak behavior is
//! already covered by `grouping.rs`, and section 3.3's "nothing matched" by
//! `filter.rs`; this file is section 4's own new ground - the keyboard route
//! onto the same state those cover.

use super::super::ResourceState;
use super::super::keyboard::{
    CollapseSection, ExpandSection, OpenSelected, PANEL_KEY_CONTEXT, SelectNext, SelectPrevious,
};
use super::{kind, stub_panel};
use crate::command::CommandRegistry;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::keymap::KeymapConfig;
use crate::ui::nav::NavTarget;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{TestAppContext, VisualTestContext};

/// Binds this panel's own Up/Down/Enter/Left/Right, plus `/` through a
/// registry built the same way `util/shell.rs::init` builds the real one -
/// `/` only resolves at all because it is a registered `Command`, so a test
/// that skipped this and bound `panel_bindings()` alone would pass for the
/// wrong reason.
pub(super) fn bind_panel_keys(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut registry = CommandRegistry::new();
        crate::ui::resource_panel::register_commands(&mut registry);
        let keymap = KeymapConfig::default();
        let bindings = crate::keymap::bindings(&registry, &keymap, &gpui_kit::DummyKeyboardMapper);
        cx.bind_keys(bindings);
        cx.bind_keys(crate::ui::resource_panel::panel_bindings());
    });
}

fn workloads_and_network_kinds() -> Vec<DiscoveredKind> {
    vec![kind("", "Pod"), kind("", "Service")]
}

/// Section 4.1: with the panel focused, Down twice then Enter opens the
/// second visible kind exactly as a double-click would - the same
/// `request_open` path, reached this time by a keystroke.
#[gpui_kit::test]
async fn down_twice_then_enter_opens_the_second_visible_kind(cx: &mut TestAppContext) {
    bind_panel_keys(cx);
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    let kinds = workloads_and_network_kinds();
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

    window
        .update(cx, |panel, window, cx| {
            panel.state = ResourceState::Loaded(kinds.clone());
            panel.focus_handle.clone().focus(window, cx);
            let opened = opened.clone();
            let entity = cx.entity();
            cx.subscribe_in(
                &entity,
                window,
                move |_panel, _entity, event, _window, _cx| {
                    let super::super::ResourceEvent::Open(target) = event else {
                        return;
                    };
                    opened.borrow_mut().push(target.clone());
                },
            )
            .detach();
            cx.notify();
        })
        .unwrap();

    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    vcx.simulate_keystrokes("down");
    vcx.simulate_keystrokes("down");
    vcx.simulate_keystrokes("enter");

    assert_eq!(
        opened.borrow().as_slice(),
        [NavTarget::Kind(kinds[1].clone())],
        "Down twice moves past Pod onto Service, and Enter opens it"
    );
}

/// Section 4.1: Left collapses the highlighted row's own section, and Right
/// expands it again - the keyboard route for the same state a header click
/// toggles (section 2.2).
#[gpui_kit::test]
async fn left_and_right_collapse_and_expand_the_highlighted_section(cx: &mut TestAppContext) {
    bind_panel_keys(cx);
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    let pod = kind("", "Pod");

    window
        .update(cx, |panel, window, cx| {
            panel.state = ResourceState::Loaded(vec![pod.clone()]);
            panel.focus_handle.clone().focus(window, cx);
            cx.notify();
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    vcx.simulate_keystrokes("down");
    assert!(
        !window
            .update(&mut vcx, |panel, _, _| panel.is_section_collapsed(&pod))
            .unwrap(),
        "Workloads starts expanded"
    );

    vcx.simulate_keystrokes("left");
    assert!(
        window
            .update(&mut vcx, |panel, _, _| panel.is_section_collapsed(&pod))
            .unwrap(),
        "Left collapses the highlighted row's section"
    );

    vcx.simulate_keystrokes("right");
    assert!(
        !window
            .update(&mut vcx, |panel, _, _| panel.is_section_collapsed(&pod))
            .unwrap(),
        "Right expands it again"
    );
}

/// Section 4.1: `SidebarMenuItem`'s own `.hover()` is styling only - nothing
/// in this panel wires a hover callback to `set_highlighted` the way
/// gpui-component's `Command` wires hover to its own selection. This pins
/// that a plain render (the only thing a mouse-over ever reaches, absent a
/// click) never moves the highlight.
#[gpui_kit::test]
async fn rendering_never_moves_the_highlight(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    window
        .update(cx, |panel, _window, cx| {
            panel.state = ResourceState::Loaded(workloads_and_network_kinds());
            cx.notify();
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |panel, _window, _cx| {
            assert!(
                panel.highlighted().is_none(),
                "rendering alone must not select a row"
            );
        })
        .unwrap();
}

/// Section 4.2: `/` focuses the filter (through the registered `resource.focus_filter`
/// command, not a raw binding), typing narrows the list, and Escape clears the
/// filter and returns focus to the list.
#[gpui_kit::test]
async fn slash_focuses_filter_typing_narrows_and_escape_clears_it(cx: &mut TestAppContext) {
    bind_panel_keys(cx);
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    let kinds = workloads_and_network_kinds();

    window
        .update(cx, |panel, window, cx| {
            panel.state = ResourceState::Loaded(kinds.clone());
            panel.focus_handle.clone().focus(window, cx);
            cx.notify();
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    vcx.simulate_keystrokes("/");
    vcx.simulate_input("service");
    vcx.run_until_parked();

    window
        .update(&mut vcx, |panel, _window, cx| {
            let sections = panel.visible_sections(cx);
            let matched: Vec<String> = sections
                .iter()
                .flat_map(|section| section.matches.iter())
                .map(|kind| kind.label())
                .collect();
            assert_eq!(matched, vec!["Service"], "typing narrowed the list");
        })
        .unwrap();

    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();

    window
        .update(&mut vcx, |panel, window, cx| {
            assert_eq!(panel.filter_text(cx), "", "Escape clears the filter");
            assert!(
                panel.focus_handle.is_focused(window),
                "Escape returns focus to the list"
            );
        })
        .unwrap();
}

/// Section 4.3: the hint row reads the live keymap rather than printing a key
/// no binding answers to - every action it names must actually resolve once
/// the panel's bindings are registered, or `hint_row`'s fallback would be
/// silently masking a missing binding.
#[gpui_kit::test]
async fn every_hinted_action_resolves_in_the_live_keymap(cx: &mut TestAppContext) {
    bind_panel_keys(cx);
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);

    window
        .update(&mut vcx, |_panel, window, _cx| {
            assert!(
                Kbd::binding_for_action(&SelectNext, Some(PANEL_KEY_CONTEXT), window).is_some(),
                "Down has no bound key"
            );
            assert!(
                Kbd::binding_for_action(&SelectPrevious, Some(PANEL_KEY_CONTEXT), window).is_some(),
                "Up has no bound key"
            );
            assert!(
                Kbd::binding_for_action(&OpenSelected, Some(PANEL_KEY_CONTEXT), window).is_some(),
                "Enter has no bound key"
            );
            assert!(
                Kbd::binding_for_action(&CollapseSection, Some(PANEL_KEY_CONTEXT), window)
                    .is_some(),
                "Left has no bound key"
            );
            assert!(
                Kbd::binding_for_action(&ExpandSection, Some(PANEL_KEY_CONTEXT), window).is_some(),
                "Right has no bound key"
            );
            assert!(
                Kbd::binding_for_action(
                    &super::super::actions::FocusFilter,
                    Some(PANEL_KEY_CONTEXT),
                    window,
                )
                .is_some(),
                "`/` has no bound key"
            );
        })
        .unwrap();
}
