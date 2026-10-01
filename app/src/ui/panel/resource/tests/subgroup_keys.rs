//! `custom-resource-grouping` §4: the keyboard route through Custom
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

/// The panel loaded with [`three_groups`], keys bound and the list focused.
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
    let collapsed = |vcx: &mut VisualTestContext| {
        window
            .update(vcx, |panel, _window, _cx| panel.collapsed_subgroups.clone())
            .unwrap()
    };

    vcx.simulate_keystrokes("down down");
    assert_eq!(cursor(window, &mut vcx), (None, Some("Widget".into())));

    vcx.simulate_keystrokes("space");
    assert!(
        collapsed(&mut vcx).contains("alpha.example.com"),
        "Space collapses the subgroup holding the focused kind"
    );
    assert_eq!(collapsed(&mut vcx).len(), 1, "and only that one");
    assert_eq!(
        cursor(window, &mut vcx),
        (Some("alpha.example.com".into()), None),
        "focus moves to the collapsed subgroup's header"
    );

    vcx.simulate_keystrokes("space");
    assert!(
        collapsed(&mut vcx).is_empty(),
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
    let (filter, collapsed) = window
        .update(&mut vcx, |panel, _window, cx| {
            (panel.filter_text(cx), panel.collapsed_subgroups.clone())
        })
        .unwrap();
    assert_eq!(filter, " ");
    assert!(collapsed.is_empty());
}
