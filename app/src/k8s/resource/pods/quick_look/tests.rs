//! `pod-quick-look` section 1, in a real window with the app's keymap
//! (`util::shell::init`) over a fake cluster (`k8s::test_cluster`): Space on a
//! selected pod opens the quick look, Space and Escape close it, Enter and its
//! button open the detail panel, Down moves it, and it follows the pod live.

use super::view::{
    CLOSE_HINT, OPEN_DETAILS_ID, OPEN_HINT, POPOVER_ID, value_selector, warning_text,
};
use crate::k8s::resource::pod_detail::glance::glance;
use crate::k8s::resource::pods::test_window::{Harness, PODS, open, pod};
use crate::ui::detail::lifecycle::{BANNER_ID, Lifecycle};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, TestAppContext};
use serde_json::{Value, json};
use std::cell::Cell;
use std::rc::Rc;

/// Spec "Opening a quick look": Space on a selected crashing pod shows its
/// phase, `1/2` ready, restarts, each container's image and state, and its
/// latest `BackOff` warning, beside the row.
#[gpui_kit::test]
async fn space_on_a_crashing_pod_shows_it_at_a_glance(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    assert!(!harness.drawn(POPOVER_ID), "nothing open yet");

    harness.press("space");
    assert_eq!(harness.looking_at().as_deref(), Some("web-1"));
    assert!(harness.drawn(POPOVER_ID), "the popover is drawn");
    harness.wait_for("listed the latest warning", |panel, cx| {
        let popover = panel.quick_look().unwrap().read(cx);
        popover.latest_warning(cx).is_some()
    });

    harness.vcx.update(|_, cx| {
        let popover = harness.panel.read(cx).quick_look().unwrap().read(cx);
        let glance = glance(popover.pod().expect("listed"), jiff::Timestamp::now());
        assert_eq!(glance.status, "Running");
        assert_eq!(glance.ready, "1/2");
        assert_eq!(glance.restarts, 5);
        assert_eq!(glance.node, "node-a");
        assert_eq!(glance.pod_ip, "10.0.0.7");
        assert_eq!(glance.owners[0].name, "web-7d9f");
        let app = &glance.containers[0];
        assert_eq!(app.image, "shop/web:1.4");
        assert!(app.state.contains("CrashLoopBackOff"), "{}", app.state);
        let warning = popover.latest_warning(cx).unwrap();
        assert!(
            warning_text(&warning, jiff::Timestamp::now()).starts_with("BackOff: Back-off"),
            "{warning:?}"
        );
    });
    harness.vcx.update(|window, cx| window.render_frame(cx));
    assert!(harness.vcx.debug_bounds("quick-look-status").is_some());
    assert!(harness.vcx.debug_bounds("quick-look-warning").is_some());
    let popover = harness.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(POPOVER_ID).expect("drawn").bounds()
    });
    let status = harness.vcx.debug_bounds("quick-look-status").unwrap();
    assert!(
        popover.contains(&status.center()),
        "the fields are laid out inside the popover, not collapsed out of it"
    );
}

/// Space and Escape close the quick look and keep the selection; Enter and the
/// Open Details button each open the pod's detail panel and close it.
#[gpui_kit::test]
async fn the_quick_look_closes_and_opens_details_from_the_keyboard_and_mouse(
    cx: &mut TestAppContext,
) {
    let mut harness = open(cx);

    harness.press("space");
    harness.press("escape");
    assert_eq!(harness.looking_at(), None, "Escape closes it");
    assert_eq!(
        harness.selected().as_deref(),
        Some("web-1"),
        "and keeps the row"
    );

    harness.press("space");
    harness.press("space");
    assert_eq!(harness.looking_at(), None, "Space closes it again");

    harness.press("space");
    harness.press("enter");
    assert_eq!(harness.looking_at(), None, "Enter closes it");
    assert_eq!(
        harness.details_opened.get(),
        1,
        "and opens the detail panel"
    );

    harness.press("space");
    assert!(harness.drawn(OPEN_DETAILS_ID));
    let button = harness.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(OPEN_DETAILS_ID).expect("drawn")
    });
    harness
        .vcx
        .simulate_click(button.bounds().center(), Modifiers::none());
    harness.vcx.run_until_parked();
    assert_eq!(harness.looking_at(), None, "the button closes it");
    assert_eq!(
        harness.details_opened.get(),
        2,
        "and opens the detail panel"
    );
}

/// Specs "Scanning several pods", "Live while open" and "The pod goes away":
/// Down moves the quick look with the selection, a ready-count change shows in
/// place, and a pod being deleted reads as Terminating, then as deleted with its
/// last state kept - the detail panels' words - with the popover still open.
#[gpui_kit::test]
async fn the_quick_look_follows_the_selection_and_the_pod_live(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.press("space");

    harness.press("down");
    assert_eq!(harness.selected().as_deref(), Some("web-2"));
    assert_eq!(
        harness.looking_at().as_deref(),
        Some("web-2"),
        "it followed"
    );

    harness
        .cluster
        .apply(PODS.0, PODS.1, pod("u2", "web-2", true));
    harness.wait_for("showed web-2's ready count drop", |panel, cx| {
        let popover = panel.quick_look().unwrap().read(cx);
        popover
            .pod()
            .is_some_and(|pod| glance(pod, jiff::Timestamp::now()).ready == "1/2")
    });

    let mut terminating = pod("u2", "web-2", true);
    let deadline = jiff::Timestamp::now() + jiff::SignedDuration::from_secs(30);
    terminating["metadata"]["deletionTimestamp"] = json!(deadline.to_string());
    harness.cluster.apply(PODS.0, PODS.1, terminating);
    harness.wait_for("showed web-2 Terminating", |panel, cx| {
        let popover = panel.quick_look().unwrap().read(cx);
        matches!(popover.lifecycle(), Some(Lifecycle::Terminating { .. }))
    });
    assert!(harness.drawn(BANNER_ID), "the banner says so");

    harness.cluster.delete(PODS.0, PODS.1, "shop", "web-2");
    harness.wait_for("showed web-2 deleted", |panel, cx| {
        let popover = panel.quick_look().unwrap().read(cx);
        matches!(popover.lifecycle(), Some(Lifecycle::Deleted { .. }))
    });
    assert_eq!(harness.looking_at().as_deref(), Some("web-2"), "still open");
    harness.vcx.update(|_, cx| {
        let popover = harness.panel.read(cx).quick_look().unwrap().read(cx);
        let last = glance(
            popover.pod().expect("its last state"),
            jiff::Timestamp::now(),
        );
        assert_eq!(last.ready, "1/2", "its last state stays");
        let message = popover
            .lifecycle()
            .unwrap()
            .message("pod", jiff::Timestamp::now());
        assert!(message.starts_with("This pod was deleted at "), "{message}");
    });
    assert!(harness.drawn(POPOVER_ID), "and still drawn");
    assert!(harness.drawn(BANNER_ID), "under the deleted banner");
}

/// Design D2: the warning watch starts with the popover, moves - after the
/// debounce - with the selection, and ends when it closes.
#[gpui_kit::test]
async fn closing_the_quick_look_releases_its_event_watch(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.press("space");
    let first = harness.vcx.update(|_, cx| {
        let popover = harness.panel.read(cx).quick_look().unwrap().read(cx);
        popover.events_table().expect("watching")
    });

    harness.press("down");
    assert!(
        first.upgrade().is_none(),
        "moving on drops the old pod's watch"
    );
    assert!(
        harness.vcx.update(|_, cx| {
            harness
                .panel
                .read(cx)
                .quick_look()
                .unwrap()
                .read(cx)
                .events_table()
                .is_none()
        }),
        "not before the selection has rested"
    );
    harness
        .vcx
        .executor()
        .advance_clock(crate::consts::QUICK_LOOK_EVENTS_DEBOUNCE);
    harness.wait_for("restarted the watch on web-2", |panel, cx| {
        panel
            .quick_look()
            .unwrap()
            .read(cx)
            .events_table()
            .is_some()
    });
    let second = harness.vcx.update(|_, cx| {
        let popover = harness.panel.read(cx).quick_look().unwrap().read(cx);
        popover.events_table().expect("watching")
    });

    harness.press("escape");
    assert!(second.upgrade().is_none(), "closing drops the watch");
}

/// Spec "Logs from the context menu": right-clicking a row selects it, and the
/// menu's Logs runs the same command as `l` on that row.
#[gpui_kit::test]
async fn logs_from_a_rows_context_menu_opens_that_pods_logs(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent};
    let mut harness = open(cx);
    let logs_opened = Rc::new(Cell::new(0));
    harness.vcx.update(|_, cx| {
        let opened = logs_opened.clone();
        cx.on_action(move |_: &crate::ui::nav::ShowLogs, _| opened.set(opened.get() + 1));
    });
    harness.vcx.update(|window, cx| window.render_frame(cx));
    let row = harness
        .vcx
        .debug_bounds("pod-cell-1-0")
        .expect("web-2's row is drawn")
        .center();

    harness.vcx.simulate_event(MouseDownEvent {
        position: row,
        button: MouseButton::Right,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    harness.vcx.simulate_event(MouseUpEvent {
        position: row,
        button: MouseButton::Right,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    harness.vcx.run_until_parked();
    assert_eq!(
        harness.selected().as_deref(),
        Some("web-2"),
        "right-click selects"
    );

    // Quick Look, Open Details, Logs, YAML: Down to the third, Enter.
    harness.press("down");
    harness.press("down");
    harness.press("down");
    harness.press("enter");
    assert_eq!(logs_opened.get(), 1, "Logs ran");
    assert_eq!(harness.selected().as_deref(), Some("web-2"), "for that pod");
}

/// `web-1` with values longer than any popover: a cloud-style node name and a
/// fully qualified image. Placeholder names, nobody's real cluster.
fn long_valued_pod() -> Value {
    let mut long = pod("u1", "web-1", true);
    long["spec"]["nodeName"] =
        json!("gke-example-cluster-default-node-pool-0123abcd-long-node-name-x9z8");
    long["spec"]["containers"][0]["image"] = json!(
        "registry.example.com/example-org/example-team/very-long-image-name:2026.10.03-build.1234"
    );
    long
}

/// A built selector as `debug_bounds` takes it. Leaked: a test's handful of
/// selectors live for the run anyway.
fn leak(selector: String) -> &'static str {
    Box::leak(selector.into_boxed_str())
}

/// Whether `inner` lies within `outer`, to the pixel.
fn within(
    inner: gpui_kit::Bounds<gpui_kit::Pixels>,
    outer: gpui_kit::Bounds<gpui_kit::Pixels>,
) -> bool {
    let slack = gpui_kit::px(0.5);
    inner.left() >= outer.left() - slack
        && inner.top() >= outer.top() - slack
        && inner.right() <= outer.right() + slack
        && inner.bottom() <= outer.bottom() + slack
}

/// `0-quick-look-layout` (1): a 60+ character node name and a long image stay
/// inside the popover - ellipsized, not overflowing its edge - at a normal and a
/// narrow window width, and the popover stays within its width bounds.
#[gpui_kit::test]
async fn long_values_stay_inside_the_popover(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.cluster.apply(PODS.0, PODS.1, long_valued_pod());
    harness.wait_for("listed the long node name", |panel, cx| {
        panel.table.read(cx).pods().iter().any(|pod| {
            pod.spec
                .as_ref()
                .and_then(|spec| spec.node_name.as_ref())
                .is_some_and(|node| node.len() > 60)
        })
    });
    harness.press("space");

    for width in [1280., 560.] {
        harness
            .vcx
            .simulate_resize(gpui_kit::size(gpui_kit::px(width), gpui_kit::px(900.)));
        harness.vcx.run_until_parked();
        let popover = harness.vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(POPOVER_ID).expect("drawn").bounds()
        });
        assert!(
            popover.size.width
                <= gpui_kit::px(width * crate::consts::QUICK_LOOK_MAX_WIDTH_FRACTION)
                    + gpui_kit::px(0.5)
                || popover.size.width
                    <= gpui_kit::px(crate::consts::QUICK_LOOK_MIN_WIDTH) + gpui_kit::px(0.5),
            "{width}px window: the popover is {popover:?}"
        );
        for key in [
            "name",
            "node",
            "ip",
            "owner",
            "image app",
            "state app",
            "image sidecar",
            "state sidecar",
        ] {
            let value = harness
                .vcx
                .debug_bounds(leak(value_selector(key)))
                .unwrap_or_else(|| panic!("{key} is drawn"));
            assert!(
                within(value, popover),
                "{width}px window: {key} at {value:?} leaves the popover at {popover:?}"
            );
        }
        // One line each, so the long ones are cut short with an ellipsis, not
        // wrapped down the popover. Element boxes are what a test can read -
        // not painted glyphs - so this pins the truncating layout, and the
        // boxes above pin where it sits.
        let name = harness
            .vcx
            .debug_bounds(leak(value_selector("name")))
            .unwrap();
        for key in ["node", "image app"] {
            let value = harness.vcx.debug_bounds(leak(value_selector(key))).unwrap();
            assert!(
                value.size.height <= name.size.height + gpui_kit::px(1.),
                "{width}px window: {key} stays on one line, {value:?}"
            );
        }
    }
}

/// `0-quick-look-layout` (2): the footer reads "↵ Open Details" then "Esc Close" -
/// each key just before its own label, the two pairs apart.
#[gpui_kit::test]
async fn the_footer_pairs_each_key_with_its_action(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.press("space");
    harness.vcx.update(|window, cx| window.render_frame(cx));
    let bounds = |harness: &mut Harness, selector: String| {
        let selector = leak(selector);
        harness
            .vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"))
    };
    let open_key = bounds(&mut harness, format!("{OPEN_HINT} key"));
    let open_label = bounds(&mut harness, format!("{OPEN_HINT} label"));
    let close_key = bounds(&mut harness, format!("{CLOSE_HINT} key"));
    let close_label = bounds(&mut harness, format!("{CLOSE_HINT} label"));
    let open = bounds(&mut harness, OPEN_HINT.to_string());
    let close = bounds(&mut harness, CLOSE_HINT.to_string());

    assert!(
        open_key.right() <= open_label.left(),
        "↵ comes before Open Details"
    );
    assert!(
        close_key.right() <= close_label.left(),
        "Esc comes before Close"
    );
    assert!(
        open.right() + gpui_kit::px(8.) <= close.left(),
        "the pairs are set apart: {open:?} then {close:?}"
    );
    assert!(
        within(open_key, open) && within(open_label, open),
        "one group"
    );
    assert!(
        within(close_key, close) && within(close_label, close),
        "one group"
    );
}
