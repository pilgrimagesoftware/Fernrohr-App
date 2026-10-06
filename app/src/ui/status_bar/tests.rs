// Named imports rather than `use super::*;` - see the comment above `mod tests;` in
// `status_bar.rs` for why: a glob here re-imports `gpui_kit::*`'s huge surface a second
// time and blows this toolchain's macro-expansion budget once a `#[gpui_kit::test]` item
// sits in the same module.
use super::{Clock, StatusBarView};
use crate::consts::STATUS_TICK_INTERVAL;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::context_health::{ContextHealth, Severity};
use crate::k8s::cluster::health::HealthTransition;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::cluster::watch_registry::PauseReason;
use gpui_kit::{AppContext as _, TestAppContext};
use kube::{Client, Config};
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

fn init(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
}

/// Subscribes to `context_name`'s Pods watch through a session whose connection is
/// fixed at `Connected`, so no real connect for the fixture context can finish later
/// and flip the health to `Failed` mid-test.
fn subscribe_connected(cx: &mut TestAppContext, context_name: &str, client: Client) {
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            context_name,
            ConnectionState::Connected(client.clone()),
        );
        ClusterRegistry::subscribe_pods(cx, context_name, client);
    });
}

fn test_client(cx: &mut TestAppContext) -> Client {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let _guard = handle.enter();
    Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
}

/// Section 2.1: two contexts, one paused - the spec's "Problem items first" scenario.
#[gpui_kit::test]
async fn non_connected_items_sort_before_connected_ones(cx: &mut TestAppContext) {
    init(cx);
    let client_a = test_client(cx);
    let client_b = test_client(cx);
    subscribe_connected(cx, "northbay", client_a);
    subscribe_connected(cx, "southbay", client_b);
    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "southbay",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });

    let bar = cx.update(|cx| {
        cx.new(|cx| StatusBarView::new(vec!["northbay".to_string(), "southbay".to_string()], cx))
    });

    cx.update(|cx| {
        let items = bar.read(cx).items(cx);
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].context_name, "southbay",
            "the paused context is listed before the connected one"
        );
        assert_eq!(items[1].context_name, "northbay");
    });

    // `southbay` stays paused for the rest of the test, so `bar`'s tick would otherwise
    // keep re-arming its timer forever (by design - see `ensure_tick`'s doc comment).
    // Dropping the entity here cancels that task instead of leaving it for the test
    // harness's own end-of-test drain to spin on indefinitely.
    drop(bar);
    cx.run_until_parked();
}

/// Section 2.1: the window's context list is fixed at construction (design.md decision
/// 3), so unsubscribing every panel from a context's watch - what closing its last panel
/// does - must not drop that context's item.
#[gpui_kit::test]
async fn closing_the_last_panel_for_a_context_keeps_its_status_item(cx: &mut TestAppContext) {
    init(cx);
    let client = test_client(cx);
    subscribe_connected(cx, "kind-dev", client);

    let bar = cx.update(|cx| cx.new(|cx| StatusBarView::new(vec!["kind-dev".to_string()], cx)));
    assert_eq!(cx.update(|cx| bar.read(cx).items(cx).len()), 1);

    cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, "kind-dev"));

    assert_eq!(
        cx.update(|cx| bar.read(cx).items(cx).len()),
        1,
        "the item stays even once nothing is subscribed to the context anymore"
    );
}

/// Section 2.1: the spec's "Readable without color" scenario - every state pairs with a
/// distinct icon and text, so no two states look alike in a grayscale screenshot.
#[test]
fn icon_and_text_differ_per_state() {
    let now = Instant::now();
    let healths = [
        ContextHealth::Connected,
        ContextHealth::WaitingForTunnel { since: now },
        ContextHealth::Paused {
            reason: PauseReason::Reconnecting,
            since: now,
        },
        ContextHealth::Paused {
            reason: PauseReason::CredentialRefresh,
            since: now,
        },
        ContextHealth::Failed {
            reason: "boom".to_string(),
            since: now,
        },
    ];

    let mut seen = HashSet::new();
    for health in &healths {
        let (icon, text) = StatusBarView::icon_and_text(health);
        assert!(
            seen.insert((format!("{icon:?}"), text)),
            "duplicate icon/text pair for {health:?}"
        );
    }
}

/// Section 2.2: elapsed time advances and escalates past 30 s as the injected clock
/// advances, decoupled from whether the tick's own timer has fired.
#[gpui_kit::test]
async fn elapsed_time_advances_and_escalates_with_the_injected_clock(cx: &mut TestAppContext) {
    init(cx);
    let client = test_client(cx);
    subscribe_connected(cx, "kind-dev", client);
    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });

    let clock = Rc::new(Cell::new(Instant::now()));
    let bar = cx.update(|cx| {
        cx.new(|cx| {
            StatusBarView::new_with_clock(
                vec!["kind-dev".to_string()],
                gpui_kit::WeakEntity::new_invalid(),
                Clock::Fake(clock.clone()),
                cx,
            )
        })
    });

    // `since` (`WatchRegistry::pause`'s own `Instant::now()`) and this fake clock's start
    // are two independent real timestamps a few microseconds apart, so elapsed lands at
    // "5s plus a hair" rather than exactly 5s - assert a range, not an exact value.
    clock.set(clock.get() + Duration::from_secs(5));
    cx.update(|cx| {
        let items = bar.read(cx).items(cx);
        let elapsed = items[0]
            .elapsed
            .expect("a paused item always has an elapsed time");
        assert!(
            elapsed >= Duration::from_secs(5) && elapsed < Duration::from_secs(6),
            "expected roughly 5s elapsed, got {elapsed:?}"
        );
        assert_eq!(
            items[0].severity,
            Severity::Warning,
            "not yet past the 30 second escalation"
        );
    });

    clock.set(clock.get() + Duration::from_secs(26));
    cx.update(|cx| {
        let items = bar.read(cx).items(cx);
        assert!(items[0].elapsed.unwrap() > Duration::from_secs(30));
        assert_eq!(
            items[0].severity,
            Severity::Danger,
            "past the 30 second escalation"
        );
    });

    // `kind-dev` stays paused for the rest of the test - see the same note in
    // `non_connected_items_sort_before_connected_ones`.
    drop(bar);
    cx.run_until_parked();
}

/// `window-context-bar` task 4.1: `set_context_names` is what lets the window's live
/// `contexts` (section 3's add/disconnect) drive the bar, rather than the list it was
/// built with. Adding a context grows `items`, and its pause afterward still reaches
/// the bar - proving the per-connection observation was rebuilt, not just the list.
#[gpui_kit::test]
async fn set_context_names_grows_items_and_observes_the_new_context(cx: &mut TestAppContext) {
    init(cx);
    let client_a = test_client(cx);
    let client_b = test_client(cx);
    subscribe_connected(cx, "kind-dev", client_a);

    let bar = cx.update(|cx| cx.new(|cx| StatusBarView::new(vec!["kind-dev".to_string()], cx)));
    assert_eq!(cx.update(|cx| bar.read(cx).items(cx).len()), 1);

    subscribe_connected(cx, "staging", client_b);
    cx.update(|cx| {
        bar.update(cx, |bar, cx| {
            bar.set_context_names(vec!["kind-dev".to_string(), "staging".to_string()], cx);
        });
    });
    assert_eq!(cx.update(|cx| bar.read(cx).items(cx).len()), 2);

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "staging",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });
    let items = cx.update(|cx| bar.read(cx).items(cx));
    let staging = items
        .iter()
        .find(|item| item.context_name == "staging")
        .expect("staging is in the list after set_context_names");
    assert!(
        matches!(
            staging.health,
            ContextHealth::Paused {
                reason: PauseReason::Reconnecting,
                ..
            }
        ),
        "the newly added context's own pause reached the bar: {:?}",
        staging.health
    );

    drop(bar);
    cx.run_until_parked();
}

/// `window-context-bar` task 4.1: dropping a context out of `set_context_names` must
/// stop the bar from listing it - the disconnect confirmation's "the chip is removed"
/// promise, one layer down.
#[gpui_kit::test]
async fn set_context_names_shrinks_items(cx: &mut TestAppContext) {
    init(cx);
    let client_a = test_client(cx);
    let client_b = test_client(cx);
    subscribe_connected(cx, "kind-dev", client_a);
    subscribe_connected(cx, "staging", client_b);

    let bar = cx.update(|cx| {
        cx.new(|cx| StatusBarView::new(vec!["kind-dev".to_string(), "staging".to_string()], cx))
    });
    assert_eq!(cx.update(|cx| bar.read(cx).items(cx).len()), 2);

    cx.update(|cx| {
        bar.update(cx, |bar, cx| {
            bar.set_context_names(vec!["kind-dev".to_string()], cx);
        });
    });

    let items = cx.update(|cx| bar.read(cx).items(cx));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].context_name, "kind-dev");
}

/// Section 2.2: the one-second tick runs only while something is unhealthy, and stops
/// once every item is connected again.
#[gpui_kit::test]
async fn the_tick_runs_while_unhealthy_and_stops_once_connected(cx: &mut TestAppContext) {
    init(cx);
    let client = test_client(cx);
    subscribe_connected(cx, "kind-dev", client);
    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });

    let bar = cx.update(|cx| cx.new(|cx| StatusBarView::new(vec!["kind-dev".to_string()], cx)));
    assert!(
        cx.update(|cx| bar.read(cx).tick.is_some()),
        "a paused context must start the tick"
    );

    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let _subscription =
        cx.update(|cx| cx.observe(&bar, move |_bar, _cx| observed.set(observed.get() + 1)));

    cx.executor().advance_clock(STATUS_TICK_INTERVAL);
    cx.run_until_parked();
    assert!(
        notifications.get() >= 1,
        "the tick must notify at least once while an item is unhealthy"
    );

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(cx, "kind-dev", HealthTransition::Resume)
    });
    cx.run_until_parked();

    assert!(
        cx.update(|cx| bar.read(cx).tick.is_none()),
        "a fully connected bar must stop ticking"
    );
}
