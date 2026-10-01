//! `custom-resource-grouping` §4 and §6.2: the keyboard route through Custom
//! Resources' subgroups, driven by real keystrokes as
//! `.claude/rules/keyboard-first.md` requires.

use super::keyboard_panel::bind_panel_keys;
use super::{kind, stub_panel};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::nav::NavTarget;
use gpui_kit::{TestAppContext, VisualTestContext, WindowHandle};

/// Three single-kind CRD groups, all in Custom Resources, so the drawn order
/// is: alpha header, Widget, beta header, WidgetPolicy, gamma header, Gadget.
fn three_groups() -> Vec<DiscoveredKind> {
    vec![
        kind("alpha.example.com", "Widget"),
        kind("beta.example.com", "WidgetPolicy"),
        kind("gamma.example.com", "Gadget"),
    ]
}

/// The panel loaded with [`three_groups`], every subgroup opened (they start
/// collapsed), keys bound and the list focused.
fn focused_panel(
    cx: &mut TestAppContext,
) -> (WindowHandle<super::super::ResourcePanel>, VisualTestContext) {
    bind_panel_keys(cx);
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    window
        .update(cx, |panel, window, cx| {
            panel.state = super::super::ResourceState::Loaded(three_groups());
            panel.expanded_subgroups = three_groups()
                .into_iter()
                .map(|kind| kind.gvk.group)
                .collect();
            panel.focus_handle.clone().focus(window, cx);
            cx.notify();
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    (window, vcx)
}

/// Where the cursor is: a subgroup header's group, or a row's kind name.
fn cursor(
    window: WindowHandle<super::super::ResourcePanel>,
    vcx: &mut VisualTestContext,
) -> (Option<String>, Option<String>) {
    window
        .update(vcx, |panel, _window, _cx| {
            let row = match panel.highlighted() {
                Some(NavTarget::Kind(kind)) => Some(kind.gvk.kind.clone()),
                _ => None,
            };
            (panel.highlighted_subgroup().map(str::to_string), row)
        })
        .unwrap()
}

/// 4.1: subgroup headers are stops in the focus order, so Down at a
/// subgroup's last visible kind lands on the next subgroup's header rather
/// than skipping to its first kind - and Up steps back across the same line.
#[gpui_kit::test]
async fn down_at_a_subgroups_last_kind_focuses_the_next_subgroups_header(cx: &mut TestAppContext) {
    let (window, mut vcx) = focused_panel(cx);

    vcx.simulate_keystrokes("down");
    assert_eq!(
        cursor(window, &mut vcx),
        (Some("alpha.example.com".into()), None),
        "the first stop is the first subgroup's header"
    );
    vcx.simulate_keystrokes("down");
    assert_eq!(
        cursor(window, &mut vcx),
        (None, Some("Widget".into())),
        "then alpha's one (and so last) kind"
    );
    vcx.simulate_keystrokes("down");
    assert_eq!(
        cursor(window, &mut vcx),
        (Some("beta.example.com".into()), None),
        "Down at alpha's last kind focuses beta's header"
    );
    vcx.simulate_keystrokes("up");
    assert_eq!(cursor(window, &mut vcx), (None, Some("Widget".into())));
}

/// 4.2: the toggle-group command (Space) collapses the subgroup the cursor
/// is in and moves the cursor to its header, since the kind it was on is no
/// longer drawn; Space on the header then expands it again.
#[gpui_kit::test]
async fn toggling_the_focused_subgroup_collapses_it_and_focuses_its_header(
    cx: &mut TestAppContext,
) {
    let (window, mut vcx) = focused_panel(cx);
    let expanded = |vcx: &mut VisualTestContext| {
        window
            .update(vcx, |panel, _window, _cx| panel.expanded_subgroups.clone())
            .unwrap()
    };

    vcx.simulate_keystrokes("down down");
    assert_eq!(cursor(window, &mut vcx), (None, Some("Widget".into())));

    vcx.simulate_keystrokes("space");
    assert!(
        !expanded(&mut vcx).contains("alpha.example.com"),
        "Space collapses the subgroup holding the focused kind"
    );
    assert_eq!(expanded(&mut vcx).len(), 2, "and only that one");
    assert_eq!(
        cursor(window, &mut vcx),
        (Some("alpha.example.com".into()), None),
        "focus moves to the collapsed subgroup's header"
    );

    vcx.simulate_keystrokes("space");
    assert_eq!(
        expanded(&mut vcx).len(),
        3,
        "Space on the header expands it"
    );
    assert_eq!(
        cursor(window, &mut vcx),
        (Some("alpha.example.com".into()), None),
        "expanding leaves focus on the header"
    );
}

/// Space is text in the filter box, so the toggle binding stands aside there:
/// typing a space filters rather than toggling a subgroup.
#[gpui_kit::test]
async fn space_in_the_filter_is_typed_not_a_toggle(cx: &mut TestAppContext) {
    let (window, mut vcx) = focused_panel(cx);
    vcx.simulate_keystrokes("down down");
    vcx.simulate_keystrokes("/");
    vcx.simulate_keystrokes("space");
    let (filter, expanded) = window
        .update(&mut vcx, |panel, _window, cx| {
            (panel.filter_text(cx), panel.expanded_subgroups.len())
        })
        .unwrap();
    assert_eq!(filter, " ");
    assert_eq!(expanded, 3, "no subgroup toggled");
}

/// Both stores the collapse/expand-all commands must leave consistent: the
/// expanded subgroups, and the top-level sections they must not touch.
fn state(
    window: WindowHandle<super::super::ResourcePanel>,
    vcx: &mut VisualTestContext,
) -> (Vec<String>, usize) {
    window
        .update(vcx, |panel, _window, _cx| {
            let mut expanded: Vec<String> = panel.expanded_subgroups.iter().cloned().collect();
            expanded.sort();
            (expanded, panel.collapsed.len())
        })
        .unwrap()
}

/// 6.2: with some subgroups collapsed, expand-all (Shift-Right) expands every
/// subgroup, and leaves the top-level sections as they were.
#[gpui_kit::test]
async fn expand_all_expands_every_subgroup(cx: &mut TestAppContext) {
    let (window, mut vcx) = focused_panel(cx);
    window
        .update(&mut vcx, |panel, _window, cx| {
            panel
                .expanded_subgroups
                .retain(|group| group == "beta.example.com");
            cx.notify();
        })
        .unwrap();
    assert_eq!(
        state(window, &mut vcx),
        (vec!["beta.example.com".into()], 0)
    );

    vcx.simulate_keystrokes("shift-right");
    assert_eq!(
        state(window, &mut vcx),
        (
            vec![
                "alpha.example.com".into(),
                "beta.example.com".into(),
                "gamma.example.com".into(),
            ],
            0
        ),
        "every subgroup expanded, no section collapsed"
    );
}

/// 6.2: with a kind inside a subgroup focused, collapse-all (Shift-Left)
/// collapses every subgroup and moves the cursor to that kind's subgroup
/// header - not the first header - leaving the top-level sections alone.
#[gpui_kit::test]
async fn collapse_all_collapses_every_subgroup_and_focuses_the_kinds_header(
    cx: &mut TestAppContext,
) {
    let (window, mut vcx) = focused_panel(cx);
    vcx.simulate_keystrokes("down down down down");
    assert_eq!(
        cursor(window, &mut vcx),
        (None, Some("WidgetPolicy".into()))
    );

    vcx.simulate_keystrokes("shift-left");
    assert_eq!(
        state(window, &mut vcx),
        (vec![], 0),
        "every subgroup collapsed, no section collapsed"
    );
    assert_eq!(
        cursor(window, &mut vcx),
        (Some("beta.example.com".into()), None),
        "focus moves to WidgetPolicy's own subgroup header"
    );
}
