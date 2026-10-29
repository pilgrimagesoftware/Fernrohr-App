//! #246: a section's and a Custom Resources subgroup's count shows only while
//! it is collapsed, toggling as it collapses and expands - by keystroke and by
//! a click on its header.

use super::super::ResourceState;
use super::super::render::{section_count_selector, subgroup_count_selector};
use super::keyboard_panel::bind_panel_keys;
use super::{kind, stub_panel};
use crate::k8s::cluster::discovery::DiscoveredKind;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};

/// The panel loaded with `kinds`, keys bound and the list focused.
fn focused(cx: &mut TestAppContext, kinds: Vec<DiscoveredKind>) -> VisualTestContext {
    bind_panel_keys(cx);
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);
    window
        .update(cx, |panel, window, cx| {
            panel.state = ResourceState::Loaded(kinds);
            panel.focus_handle.clone().focus(window, cx);
            cx.notify();
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx
}

fn drawn(vcx: &mut VisualTestContext, selector: String) -> bool {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.debug_bounds(selector.leak()).is_some()
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    vcx.simulate_keystrokes(keys);
    vcx.run_until_parked();
}

fn click(vcx: &mut VisualTestContext, id: &'static str) {
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    });
    vcx.run_until_parked();
}

#[gpui_kit::test]
async fn a_sections_count_shows_only_while_it_is_collapsed(cx: &mut TestAppContext) {
    let mut vcx = focused(cx, vec![kind("", "Pod"), kind("apps", "Deployment")]);
    let count = || section_count_selector("Workloads");
    assert!(!drawn(&mut vcx, count()), "expanded: the rows show it");

    // Keyboard: Left on a row collapses its section, Right opens it again.
    press(&mut vcx, "down");
    press(&mut vcx, "left");
    assert!(
        drawn(&mut vcx, count()),
        "collapsed: the count says what's inside"
    );
    press(&mut vcx, "right");
    assert!(!drawn(&mut vcx, count()), "and goes as soon as it opens");

    // Mouse: a click on the header toggles it the same way.
    click(&mut vcx, "resource-section-Workloads");
    assert!(drawn(&mut vcx, count()));
    click(&mut vcx, "resource-section-Workloads");
    assert!(!drawn(&mut vcx, count()));
}

#[gpui_kit::test]
async fn a_subgroups_count_shows_only_while_it_is_collapsed(cx: &mut TestAppContext) {
    let mut vcx = focused(
        cx,
        vec![
            kind("alpha.example.com", "Widget"),
            kind("beta.example.com", "Gadget"),
        ],
    );
    let alpha = || subgroup_count_selector("alpha.example.com");
    let beta = || subgroup_count_selector("beta.example.com");
    assert!(
        drawn(&mut vcx, alpha()) && drawn(&mut vcx, beta()),
        "subgroups start collapsed, counted"
    );

    // Keyboard: Shift+Right opens every subgroup, Shift+Left closes them.
    press(&mut vcx, "down");
    press(&mut vcx, "shift-right");
    assert!(!drawn(&mut vcx, alpha()) && !drawn(&mut vcx, beta()));
    press(&mut vcx, "shift-left");
    assert!(drawn(&mut vcx, alpha()) && drawn(&mut vcx, beta()));

    // Mouse: a click on one subgroup's header toggles that one alone.
    click(&mut vcx, "resource-subgroup-alpha.example.com");
    assert!(!drawn(&mut vcx, alpha()), "opened: no count");
    assert!(
        drawn(&mut vcx, beta()),
        "the other stays collapsed, counted"
    );
}
