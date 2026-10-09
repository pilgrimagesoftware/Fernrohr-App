//! Window-level tests for the events browser: a panel in a `Root` with the
//! registry's real bindings, over an [`EventsTable`] the test feeds watch events
//! itself - no cluster. Keyboard routes go through real keystrokes.

use super::EventsPanel;
use super::filters::{EventFilters, Facet};
use super::store::EventsTable;
use crate::command::CommandRegistry;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::keymap::{self, KeymapConfig};
use crate::ui::link::FollowReference;
use crate::ui::list_search::ListSearch;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::table::ColumnSort;
use gpui_kit::{
    AppContext as _, Entity, Focusable as _, Keystroke, Modifiers, TestAppContext,
    VisualTestContext,
};
use k8s_openapi::api::core::v1::Event as K8sEvent;
use kube_runtime::watcher;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;

struct Harness {
    vcx: VisualTestContext,
    panel: Entity<EventsPanel>,
    events: Entity<EventsTable>,
    /// Every `FollowReference` the window saw.
    followed: Rc<RefCell<Vec<FollowReference>>>,
}

/// An event `uid` about `kind/name` in `namespace`, `minutes` ago.
// Each of an event's facets is a parameter, so every fixture reads as the
// event it is, in one line per field.
#[allow(clippy::too_many_arguments)]
fn event(
    uid: &str,
    type_: &str,
    reason: &str,
    kind: &str,
    namespace: &str,
    name: &str,
    message: &str,
    minutes: i64,
) -> K8sEvent {
    let last = jiff::Timestamp::now() - jiff::SignedDuration::from_mins(minutes);
    serde_json::from_value(json!({
        "metadata": { "uid": uid, "name": format!("{name}.{uid}"), "namespace": namespace },
        "involvedObject": { "kind": kind, "apiVersion": "v1", "namespace": namespace, "name": name },
        "type": type_, "reason": reason, "message": message, "count": 1,
        "lastTimestamp": last.to_string(),
        "source": { "component": "kubelet" },
    }))
    .unwrap()
}

/// Three events: an old Warning BackOff about a Pod in `payments`, a newer
/// Normal Scheduled about a Pod in `payments`, and the newest, a Warning
/// FailedMount about a Pod in `staging`.
fn fixture() -> Vec<K8sEvent> {
    vec![
        event(
            "e1",
            "Warning",
            "BackOff",
            "Pod",
            "payments",
            "web-1",
            "Back-off restarting failed container",
            30,
        ),
        event(
            "e2",
            "Normal",
            "Scheduled",
            "Pod",
            "payments",
            "web-2",
            "Successfully assigned payments/web-2",
            10,
        ),
        event(
            "e3",
            "Warning",
            "FailedMount",
            "Pod",
            "staging",
            "api-1",
            "MountVolume.SetUp failed: ImagePullBackOff while pulling the sidecar image registry.example/sidecar:1.2.3, which is a long message",
            1,
        ),
    ]
}

fn harness(cx: &mut TestAppContext, events: Vec<K8sEvent>) -> Harness {
    harness_with(cx, events, |_| {})
}

/// [`harness`], with `setup` run on the panel before it first draws - a
/// restored panel's saved state.
fn harness_with(
    cx: &mut TestAppContext,
    events: Vec<K8sEvent>,
    setup: impl FnOnce(&mut EventsPanel),
) -> Harness {
    harness_overriding_keymap(cx, events, &[], setup)
}

/// [`harness_with`], with `overrides` (command id, key) added to the keymap -
/// so a test can prove the hint row's key is the live, rebound one rather
/// than always its fallback.
fn harness_overriding_keymap(
    cx: &mut TestAppContext,
    events: Vec<K8sEvent>,
    overrides: &[(&str, &str)],
    setup: impl FnOnce(&mut EventsPanel),
) -> Harness {
    cx.executor().allow_parking();
    let followed: Rc<RefCell<Vec<FollowReference>>> = Rc::default();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        super::register_commands(&mut registry);
        crate::ui::list_sort::register_commands(&mut registry);
        let mut config = KeymapConfig::default();
        for (id, key) in overrides {
            config.bindings.insert(id.to_string(), key.to_string());
        }
        let bindings = keymap::bindings(&registry, &config, cx.keyboard_mapper().as_ref());
        cx.bind_keys(bindings);
        let followed = followed.clone();
        cx.on_action(move |action: &FollowReference, _cx| {
            followed.borrow_mut().push(action.clone())
        });
    });
    let client = {
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    };
    let table = cx.update(|cx| {
        cx.new(|_| {
            let mut table = EventsTable::default();
            table.replace_all(events);
            table
        })
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(DiscoveredKind::events()), "kind-dev".into());
        let panel = cx.new(|cx| {
            let mut panel = EventsPanel::with_table(
                scope,
                table.clone(),
                client,
                vec![DiscoveredKind::pods()],
                cx,
            );
            setup(&mut panel);
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    Harness {
        vcx,
        panel,
        events: table,
        followed,
    }
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    for key in keys.split(' ') {
        let key = Keystroke::parse(key).expect("valid").unparse();
        vcx.simulate_keystrokes(&key);
    }
    vcx.run_until_parked();
}

/// The displayed rows' uids, top to bottom.
fn shown(h: &mut Harness) -> Vec<String> {
    h.vcx.update(|_, cx| {
        let table = h.panel.read(cx).table.clone().expect("the table is drawn");
        table
            .read(cx)
            .delegate()
            .rows()
            .iter()
            .map(|row| row.uid.clone())
            .collect()
    })
}

fn focus_table(h: &mut Harness) {
    let panel = h.panel.clone();
    h.vcx.update(|window, cx| {
        let table = panel.read(cx).table.clone().expect("the table is drawn");
        table.read(cx).focus_handle(cx).focus(window, cx);
    });
    h.vcx.run_until_parked();
}

/// Selects displayed row `row_ix`, the way a click does.
fn select(h: &mut Harness, row_ix: usize) {
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        let table = panel.read(cx).table.clone().expect("the table is drawn");
        table.update(cx, |table, cx| {
            table.delegate_mut().remember_selection(row_ix);
            table.set_selected_row(row_ix, cx);
        });
    });
    h.vcx.run_until_parked();
}

/// Spec: "Default sort is newest first" - Last Seen ascending, newest on top,
/// and the header shows the sort.
#[gpui_kit::test]
async fn it_opens_newest_first(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    assert_eq!(shown(&mut h), ["e3", "e2", "e1"]);
    let sort = h.vcx.update(|_, cx| h.panel.read(cx).sort(cx));
    assert_eq!(
        sort,
        Some((super::columns::EventColumn::LastSeen, ColumnSort::Ascending))
    );
}

/// Spec: "Live updates" - a new event from the watch appears at the top.
#[gpui_kit::test]
async fn a_new_event_appears_at_the_top(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    let events = h.events.clone();
    h.vcx.update(|_, cx| {
        events.update(cx, |table, cx| {
            table.apply(watcher::Event::Apply(event(
                "e4",
                "Normal",
                "ScalingReplicaSet",
                "Deployment",
                "payments",
                "web",
                "Scaled up replica set web-7d9f to 3",
                0,
            )));
            cx.notify();
        })
    });
    h.vcx.run_until_parked();
    assert_eq!(shown(&mut h).first().map(String::as_str), Some("e4"));
}

/// A changed sort is what the panel saves, and a saved sort is restored.
#[gpui_kit::test]
async fn a_changed_sort_is_saved_and_restored(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    let panel = h.panel.clone();
    h.vcx.update(|window, cx| {
        let table = panel.read(cx).table.clone().expect("drawn");
        table.update(cx, |table, cx| {
            use gpui_kit::component::table::TableDelegate as _;
            table
                .delegate_mut()
                .perform_sort(2, ColumnSort::Ascending, window, cx)
        });
    });
    h.vcx.run_until_parked();
    let saved = h
        .vcx
        .update(|_, cx| super::restore::dump(panel.read(cx), cx));
    let restored = super::restore::from_state(&saved).expect("a saved panel reads back");
    assert_eq!(
        restored.sort,
        Some((super::columns::EventColumn::Reason, ColumnSort::Ascending))
    );
}

/// Spec: "Following an event to its pod" - the detail strip's link and Enter
/// both follow the selected event's involved object.
#[gpui_kit::test]
async fn the_involved_object_is_followed_by_click_and_enter(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    select(&mut h, 2); // e1, the BackOff about web-1
    let link = h
        .vcx
        .debug_bounds("events-detail-strip")
        .expect("the selected event's strip is drawn");
    assert!(link.size.height > gpui_kit::px(0.));
    let target = h
        .vcx
        .debug_bounds("kind-icon events-involved-0 Pod")
        .expect("the involved object is a link with its kind icon");
    h.vcx.simulate_click(target.center(), Modifiers::none());
    h.vcx.run_until_parked();
    focus_table(&mut h);
    press(&mut h.vcx, "enter");

    let followed = h.followed.borrow().clone();
    assert_eq!(followed.len(), 2, "one follow each: {followed:?}");
    for follow in followed {
        assert_eq!(follow.target.kind, "Pod");
        assert_eq!(follow.target.name, "web-1");
        assert_eq!(follow.target.namespace.as_deref(), Some("payments"));
        assert_eq!(follow.context_name, "kind-dev");
    }
}

/// Spec: "Reading a long message" - the strip shows the whole message.
#[gpui_kit::test]
async fn a_selected_events_whole_message_is_shown(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    select(&mut h, 0); // e3, the long message
    let message = h
        .vcx
        .update(|_, cx| h.panel.read(cx).selected_event(cx).map(|row| row.message));
    assert!(message.is_some_and(|message| message.ends_with("which is a long message")));
    assert!(h.vcx.debug_bounds("events-detail-message").is_some());
}

/// Spec: "Warnings only" and "Combining filters", by setting the filters
/// directly; the chips show and clear.
#[gpui_kit::test]
async fn filters_narrow_and_their_chips_clear_them(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    let panel = h.panel.clone();
    let set = |h: &mut Harness, filters: EventFilters| {
        let panel = panel.clone();
        h.vcx.update(|_, cx| {
            panel.update(cx, |panel, cx| {
                panel.filters = filters;
                cx.notify();
            })
        });
        h.vcx.run_until_parked();
    };
    let mut warnings = EventFilters::default();
    warnings.toggle(Facet::Type, "Warning");
    set(&mut h, warnings.clone());
    assert_eq!(shown(&mut h), ["e3", "e1"], "Warnings only");
    assert!(h.vcx.debug_bounds("events-chip-type-Warning").is_some());

    let mut combined = warnings;
    combined.toggle(Facet::Kind, "Pod");
    combined.toggle(Facet::Reason, "BackOff");
    set(&mut h, combined);
    assert_eq!(shown(&mut h), ["e1"], "Warning BackOff about Pods");

    let chip = h
        .vcx
        .debug_bounds("events-chip-reason-BackOff")
        .expect("the reason chip is drawn");
    h.vcx.simulate_click(chip.center(), Modifiers::none());
    h.vcx.run_until_parked();
    assert_eq!(
        shown(&mut h),
        ["e3", "e1"],
        "the chip cleared its value only"
    );

    let clear = h
        .vcx
        .debug_bounds("events-clear-filters")
        .expect("Clear filters is drawn");
    h.vcx.simulate_click(clear.center(), Modifiers::none());
    h.vcx.run_until_parked();
    assert_eq!(shown(&mut h).len(), 3, "every filter cleared");
}

/// Spec: "Restored with the window" - filters are saved and come back.
#[gpui_kit::test]
async fn filters_are_saved_and_restored(cx: &mut TestAppContext) {
    let mut filters = EventFilters::default();
    filters.toggle(Facet::Type, "Warning");
    let saved_filters = filters.clone();
    let mut h = harness_with(cx, fixture(), move |panel| panel.filters = filters);
    assert_eq!(shown(&mut h), ["e3", "e1"], "restored filters apply");
    let saved = h
        .vcx
        .update(|_, cx| super::restore::dump(h.panel.read(cx), cx));
    let restored = super::restore::from_state(&saved).expect("reads back");
    assert_eq!(restored.filters, saved_filters);
}

/// `saved-panel-layouts` 1.6: the search box's text is saved and, once a
/// restored panel's search box is built, comes back narrowing the rows the
/// same way typing it would.
#[gpui_kit::test]
async fn search_text_is_saved_and_restored(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    focus_table(&mut h);
    press(&mut h.vcx, "/");
    h.vcx.simulate_input("imagepullbackoff");
    h.vcx.run_until_parked();
    assert_eq!(shown(&mut h), ["e3"]);

    let saved = h
        .vcx
        .update(|_, cx| super::restore::dump(h.panel.read(cx), cx));
    let restored = super::restore::from_state(&saved).expect("a saved panel reads back");
    assert_eq!(restored.filter.as_deref(), Some("imagepullbackoff"));

    let mut h2 = harness_with(cx, fixture(), move |panel| {
        panel.search = ListSearch::restored(restored.filter)
    });
    assert_eq!(shown(&mut h2), ["e3"], "the restored search narrows rows");
    let search_text = h2.vcx.update(|_, cx| h2.panel.read(cx).search.query(cx));
    assert_eq!(search_text, "imagepullbackoff");
}

/// Data saved before the search box's text was persisted
/// (`saved-panel-layouts` 1.6) still restores, with no search text.
#[test]
fn state_without_a_saved_search_still_restores() {
    let state = json!({ "context_name": "kind-dev", "namespaces": [] });
    let restored = super::restore::from_state(&state).expect("the context name is there");
    assert_eq!(restored.filter, None);
}

/// Spec: "Searching messages" - `/`, then text, narrows the rows and shows a
/// count; Escape clears it.
#[gpui_kit::test]
async fn searching_narrows_rows_and_counts_them(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    focus_table(&mut h);
    press(&mut h.vcx, "/");
    h.vcx.simulate_input("imagepullbackoff");
    h.vcx.run_until_parked();
    assert_eq!(shown(&mut h), ["e3"]);
    let count = h.vcx.debug_bounds("events-count");
    assert!(count.is_some(), "the match count is drawn");

    press(&mut h.vcx, "escape");
    assert_eq!(shown(&mut h).len(), 3, "Escape clears the search");
}

/// Spec: "Filter by keyboard" - `t`, type to narrow, Enter toggles Warning,
/// Escape closes; only Warnings remain, without the mouse.
#[gpui_kit::test]
async fn filtering_by_type_from_the_keyboard(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    focus_table(&mut h);
    press(&mut h.vcx, "t");
    assert!(
        h.vcx.debug_bounds("events-facet-picker").is_some(),
        "the type picker opened"
    );
    h.vcx.simulate_input("warn");
    h.vcx.run_until_parked();
    press(&mut h.vcx, "enter");
    press(&mut h.vcx, "escape");

    assert_eq!(shown(&mut h), ["e3", "e1"], "only Warnings remain");
    let filters = h.vcx.update(|_, cx| h.panel.read(cx).filters.clone());
    assert!(filters.types.contains("Warning"));
}

/// The namespace scope narrows before the filters.
#[gpui_kit::test]
async fn the_namespace_scope_narrows_events(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    let panel = h.panel.clone();
    h.vcx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.scope = panel.scope.scoped_to(vec!["payments".into()]);
            cx.notify();
        })
    });
    h.vcx.run_until_parked();
    assert_eq!(shown(&mut h), ["e2", "e1"]);
}

/// The mouse route to a filter: the Type button opens the same picker.
#[gpui_kit::test]
async fn the_type_button_opens_the_type_picker(cx: &mut TestAppContext) {
    let mut h = harness(cx, fixture());
    let button = h
        .vcx
        .debug_bounds("events-filter-type")
        .expect("the Type filter button is drawn");
    h.vcx.simulate_click(button.center(), Modifiers::none());
    h.vcx.run_until_parked();
    assert!(h.vcx.debug_bounds("events-facet-picker").is_some());
}

/// `list-search` #189's regression: the shared `ListSearch` component behind
/// this search box, case-insensitivity and the hint row's live key.
mod search;
mod sort;
mod states;
