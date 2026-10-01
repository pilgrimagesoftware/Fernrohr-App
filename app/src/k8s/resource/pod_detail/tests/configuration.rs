//! The Configuration tab against a fixture API server, through real keystrokes
//! (`pod-configuration-tab` 2.2-2.3): nothing is read until the tab is shown,
//! a Secret value is revealed by Tab and Space and hidden by a tab switch or
//! `h`, and no revealed value reaches the panel's `Debug` output or its saved
//! layout.

use super::config_fixture::{
    PASSWORD, USERNAME, db, harness, reveal_password_by_keyboard, shown, wait_for,
};
use crate::k8s::resource::pod_detail::model::DetailSection;
use gpui_kit::component::dock::BasePanel as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use std::sync::atomic::Ordering;

#[gpui_kit::test]
async fn reveal_one_value_by_keyboard_and_hide_it_again(cx: &mut TestAppContext) {
    let h = harness(cx);
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    let panel = h.panel.clone();
    wait_for(&mut vcx, &panel, |panel| panel.pod().is_some());

    // 2.2: loading the pod reads no ConfigMap or Secret.
    assert_eq!(h.config_reads.load(Ordering::SeqCst), 0);

    h.window
        .update(&mut vcx, |_, window, cx| {
            panel.read(cx).focus_handle.clone().focus(window, cx)
        })
        .unwrap();
    vcx.simulate_keystrokes("3");
    wait_for(&mut vcx, &panel, |panel| {
        panel.active_tab() == DetailSection::Configuration
            && panel.configuration.cards.len() == 2
            && panel.configuration.cards.values().all(|card| {
                !matches!(
                    card,
                    crate::k8s::resource::pod_detail::configuration::CardContents::Loading
                )
            })
    });
    assert_eq!(
        h.config_reads.load(Ordering::SeqCst),
        2,
        "one read per card"
    );

    // 2.3: Tab + Space reveals the one value asked for, and only that one.
    reveal_password_by_keyboard(&mut vcx, &h);
    wait_for(&mut vcx, &panel, shown);
    let state = vcx.update(|_, cx| format!("{:?}", panel.read(cx).configuration));
    assert!(
        !state.contains(PASSWORD) && !state.contains(USERNAME),
        "the reveal state's Debug output names a value: {state}"
    );
    let dump = vcx.update(|_, cx| format!("{:?}", panel.read(cx).dump(cx)));
    assert!(
        !dump.contains(PASSWORD),
        "the saved layout holds a value: {dump}"
    );
    assert!(vcx.update(|_, cx| {
        panel
            .read(cx)
            .configuration
            .reveal_of(&db(), "username")
            .is_none()
    }));

    // Leaving the tab hides it.
    vcx.simulate_keystrokes("1");
    vcx.run_until_parked();
    assert!(vcx.update(|_, cx| panel.read(cx).configuration.revealed.is_empty()));

    // Back on the tab - `1` was pressed on the reveal button, which unmounted
    // with the tab, yet focus stayed in the panel, so `3` still works - reveal
    // again; `h` hides every revealed value.
    vcx.simulate_keystrokes("3");
    vcx.run_until_parked();
    reveal_password_by_keyboard(&mut vcx, &h);
    wait_for(&mut vcx, &panel, shown);
    h.window
        .update(&mut vcx, |_, window, cx| {
            panel.read(cx).focus_handle.clone().focus(window, cx)
        })
        .unwrap();
    vcx.simulate_keystrokes("h");
    vcx.run_until_parked();
    assert!(vcx.update(|_, cx| panel.read(cx).configuration.revealed.is_empty()));
}

/// The tab keys are positional: `3` is Configuration, and Volumes, Events and
/// Managed Fields moved up one each.
#[gpui_kit::test]
async fn tab_keys_follow_tab_order(cx: &mut TestAppContext) {
    let h = harness(cx);
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    let panel = h.panel.clone();
    wait_for(&mut vcx, &panel, |panel| panel.pod().is_some());
    h.window
        .update(&mut vcx, |_, window, cx| {
            panel.read(cx).focus_handle.clone().focus(window, cx)
        })
        .unwrap();
    for (key, section) in [
        ("3", DetailSection::Configuration),
        ("4", DetailSection::Volumes),
        ("5", DetailSection::Events),
        ("6", DetailSection::ManagedFields),
        ("2", DetailSection::Containers),
        ("1", DetailSection::Overview),
    ] {
        vcx.simulate_keystrokes(key);
        vcx.run_until_parked();
        assert_eq!(
            vcx.update(|_, cx| panel.read(cx).active_tab()),
            section,
            "after `{key}`"
        );
    }
}
