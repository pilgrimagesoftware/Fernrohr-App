//! `unwatchable-kinds`, in a list panel: a polled kind says so and re-lists on
//! `r` or its Refresh button; a kind that can't be listed says that instead of
//! drawing an empty table; ComponentStatus carries its deprecation note.

use super::{Harness, deployments, focus_table, harness, object, press};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_list::ListMode;
use futures_util::FutureExt as _;
use gpui_kit::{Modifiers, TestAppContext};
use kube::core::GroupVersionKind;
use std::sync::Arc;
use tokio::sync::Notify;

/// Puts `h`'s table in `mode` and redraws.
fn set_mode(h: &mut Harness, mode: ListMode) {
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        let objects = panel.read(cx).objects.clone();
        objects.update(cx, |table, cx| {
            table.set_mode(mode);
            cx.notify();
        });
    });
    h.vcx.run_until_parked();
}

/// Whether `refresh` was notified since it was last waited on.
fn woken(refresh: &Notify) -> bool {
    refresh.notified().now_or_never().is_some()
}

#[gpui_kit::test]
async fn a_polled_list_says_so_and_r_refreshes_it(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), vec![object("web", Some("staging"))]);
    let refresh = Arc::new(Notify::new());
    set_mode(
        &mut h,
        ListMode::Polled {
            refresh: refresh.clone(),
        },
    );
    assert!(
        h.vcx.debug_bounds("object-list-polled").is_some(),
        "the polled note is drawn"
    );
    focus_table(&mut h);

    press(&mut h.vcx, "r");

    assert!(woken(&refresh), "`r` asked the poller to re-list");
}

#[gpui_kit::test]
async fn the_refresh_button_refreshes_a_polled_list(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), vec![object("web", Some("staging"))]);
    let refresh = Arc::new(Notify::new());
    set_mode(
        &mut h,
        ListMode::Polled {
            refresh: refresh.clone(),
        },
    );
    focus_table(&mut h);
    let button = h
        .vcx
        .debug_bounds("object-list-refresh")
        .expect("the Refresh button is drawn");

    h.vcx.simulate_click(button.center(), Modifiers::none());
    h.vcx.run_until_parked();

    assert!(woken(&refresh), "the button asked the poller to re-list");
}

#[gpui_kit::test]
async fn a_watched_list_has_no_polled_note(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), vec![object("web", Some("staging"))]);
    assert!(h.vcx.debug_bounds("object-list-polled").is_none());
    assert!(h.vcx.debug_bounds("object-list-deprecated").is_none());
}

#[gpui_kit::test]
async fn an_unlistable_kind_says_it_cant_be_listed(cx: &mut TestAppContext) {
    let mut h = harness(cx, deployments(), Vec::new());
    set_mode(&mut h, ListMode::Unlistable);
    assert!(h.vcx.debug_bounds("object-list-unlistable").is_some());
}

#[gpui_kit::test]
async fn component_status_carries_its_deprecation_note(cx: &mut TestAppContext) {
    let kind = DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "ComponentStatus"),
        plural: "componentstatuses".into(),
        namespaced: false,
        verbs: Default::default(),
    };
    let mut h = harness(cx, kind, vec![object("etcd-0", None)]);
    assert!(h.vcx.debug_bounds("object-list-deprecated").is_some());
}
