//! `list-search` #189 for the Pods panel: its filter box runs through the
//! shared `crate::ui::list_search::ListSearch`, the same mechanism the Events
//! browser and every object list use - `/` focuses it, typing narrows rows by
//! any visible column (not name alone), case-insensitively, Escape clears it
//! and returns focus to the table, the hint row shows its live key, and its
//! text round-trips through a saved panel layout.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::resource::pods::test_support::pod_in;
use crate::k8s::resource::pods::{FocusFilter, PANEL_KEY_CONTEXT, PodsPanel};
use crate::keymap::{self, KeymapConfig};
use crate::ui::list_search::ListSearch;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::component::dock::{BasePanel as _, PanelInfo};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{
    AppContext as _, Entity, Focusable as _, Keystroke, TestAppContext, VisualTestContext,
};
use kube_runtime::watcher;

struct Harness {
    vcx: VisualTestContext,
    panel: Entity<PodsPanel>,
}

fn test_client(cx: &mut TestAppContext) -> kube::Client {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let _guard = handle.enter();
    kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
}

/// A Connected Pods panel in a `Root`, with the registry's real bindings
/// (`overrides` on top of their defaults), over two pods in different
/// namespaces - "web" in "production", "api" in "staging". `setup` runs on
/// the panel before its first render - a restored panel's saved filter, which
/// has to be in the box before anything reads it (`ui::list_search`'s own doc
/// comment on why).
fn harness(
    cx: &mut TestAppContext,
    overrides: &[(&str, &str)],
    setup: impl FnOnce(&mut PodsPanel),
) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = crate::command::CommandRegistry::new();
        crate::k8s::resource::pods::register_commands(&mut registry);
        let mut config = KeymapConfig::default();
        for (id, key) in overrides {
            config.bindings.insert(id.to_string(), key.to_string());
        }
        let bindings = keymap::bindings(&registry, &config, cx.keyboard_mapper().as_ref());
        cx.bind_keys(bindings);
    });
    let client = test_client(cx);
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::pods(), "dev".into());
        let connection =
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
        let namespaces = cx.new(|_| NamespaceList::empty());
        let panel = cx.new(|cx| {
            let mut panel = PodsPanel::with_connection(scope, connection, namespaces, cx);
            setup(&mut panel);
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|_, cx| {
        let table = panel.read(cx).table.clone();
        table.update(cx, |table, cx| {
            table.apply(watcher::Event::Init);
            table.apply(watcher::Event::InitApply(pod_in(
                "production",
                "u1",
                "web",
                0,
            )));
            table.apply(watcher::Event::InitApply(pod_in("staging", "u2", "api", 0)));
            table.apply(watcher::Event::InitDone);
            cx.notify();
        });
    });
    vcx.run_until_parked();
    Harness { vcx, panel }
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    for key in keys.split(' ') {
        let key = Keystroke::parse(key).expect("valid").unparse();
        vcx.simulate_keystrokes(&key);
    }
    vcx.run_until_parked();
}

fn focus_panel(h: &mut Harness) {
    let panel = h.panel.clone();
    h.vcx.update(|window, cx| {
        panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    h.vcx.run_until_parked();
}

fn row_names(h: &mut Harness) -> Vec<String> {
    h.vcx.update(|_, cx| {
        let table = h
            .panel
            .read(cx)
            .pod_table
            .clone()
            .expect("the table is drawn");
        table
            .read(cx)
            .delegate()
            .rows()
            .iter()
            .map(|row| row.row.name.clone())
            .collect()
    })
}

/// `/` focuses the filter, from the panel with no selection needed first.
#[gpui_kit::test]
async fn slash_focuses_the_filter(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[], |_| {});
    focus_panel(&mut h);

    press(&mut h.vcx, "/");

    let focused = h
        .vcx
        .update(|window, cx| h.panel.read(cx).filter_focused(window, cx));
    assert!(focused, "`/` focused the filter box");
}

/// Namespace is a visible column but not Name: filtering by its text (in a
/// different case) narrows rows the same way, proving the shared matcher
/// checks every visible column, not name alone.
#[gpui_kit::test]
async fn filtering_by_a_non_name_column_is_case_insensitive(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[], |_| {});
    focus_panel(&mut h);

    press(&mut h.vcx, "/");
    h.vcx.simulate_input("STAGING");
    h.vcx.run_until_parked();

    assert_eq!(
        row_names(&mut h),
        ["api"],
        "matched the namespace column despite the different case"
    );
}

/// Escape clears the filter and hands focus back to the table.
#[gpui_kit::test]
async fn escape_clears_the_filter_and_refocuses_the_table(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[], |_| {});
    focus_panel(&mut h);

    press(&mut h.vcx, "/");
    h.vcx.simulate_input("STAGING");
    h.vcx.run_until_parked();
    assert_eq!(row_names(&mut h), ["api"]);

    press(&mut h.vcx, "escape");

    assert_eq!(
        row_names(&mut h).len(),
        2,
        "clearing the filter restores every row"
    );
    let table_focused = h.vcx.update(|window, cx| {
        let table = h.panel.read(cx).pod_table.clone().unwrap();
        table.read(cx).focus_handle(cx).is_focused(window)
    });
    assert!(table_focused, "focus is back on the table");
}

/// A `keymap.toml` override of `pods.focus_filter` rebinds `/`: the old
/// default no longer focuses the box, the new key does, and the hint row -
/// which reads the same live keymap through `Kbd::binding_for_action` - has a
/// key to show.
#[gpui_kit::test]
async fn the_filter_hint_follows_a_keymap_override(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[("pods.focus_filter", "ctrl-/")], |_| {});
    focus_panel(&mut h);

    press(&mut h.vcx, "/");
    let focused_by_default_key = h
        .vcx
        .update(|window, cx| h.panel.read(cx).filter_focused(window, cx));
    assert!(
        !focused_by_default_key,
        "the default key no longer focuses the filter box once rebound"
    );

    press(&mut h.vcx, "ctrl-/");
    let focused_by_override = h
        .vcx
        .update(|window, cx| h.panel.read(cx).filter_focused(window, cx));
    assert!(focused_by_override, "the rebound key does");

    let hinted = h
        .vcx
        .update(|window, _| Kbd::binding_for_action(&FocusFilter, Some(PANEL_KEY_CONTEXT), window));
    assert!(hinted.is_some(), "the hint row has a live key to show");
}

/// The filter's text is in `dump()`, and restoring it filters rows on the
/// first frame - before any keystroke redraws it.
#[gpui_kit::test]
async fn the_filter_is_saved_and_restored(cx: &mut TestAppContext) {
    let mut h = harness(cx, &[], |_| {});
    focus_panel(&mut h);
    press(&mut h.vcx, "/");
    h.vcx.simulate_input("staging");
    h.vcx.run_until_parked();

    let state = h.vcx.update(|_, cx| h.panel.read(cx).dump(cx));
    let PanelInfo::Panel(data) = state.info else {
        panic!("a Pods panel saves panel state");
    };
    let filter = crate::k8s::resource::pods::filter::filter_from_state(&data);
    assert_eq!(filter.as_deref(), Some("staging"));

    let mut restored = harness(cx, &[], |panel| {
        panel.filter = ListSearch::restored(filter);
    });
    assert_eq!(
        row_names(&mut restored),
        ["api"],
        "the restored filter narrows rows on the first frame"
    );
}

/// State saved before the `filter` field existed (`saved-panel-layouts` 1.6)
/// still restores, with every row shown rather than an empty table.
#[gpui_kit::test]
async fn data_without_a_saved_filter_still_restores_every_row(cx: &mut TestAppContext) {
    let saved = serde_json::json!({});
    let filter = crate::k8s::resource::pods::filter::filter_from_state(&saved);
    assert_eq!(filter, None, "no saved filter reads back as none");

    let mut h = harness(cx, &[], |panel| {
        panel.filter = ListSearch::restored(filter);
    });
    assert_eq!(row_names(&mut h).len(), 2, "every row, unfiltered");
}
