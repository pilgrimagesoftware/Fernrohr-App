// Named imports rather than `use super::*`: the parent's `gpui_kit::*` would
// shadow the built-in `#[test]`.
use super::{from_state, restores_as_placeholder};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_list::{ObjectListPanel, ObjectsTable};
use crate::ui::list_search::ListSearch;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::component::dock::BasePanel as _;
use gpui_kit::component::dock::PanelInfo;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext, px};
use kube::core::GroupVersionKind;
use serde_json::json;

/// A namespaced kind with only the base columns (`standard-resource-panels`
/// section 2 gives Service and most built-ins their own), so these tests are
/// about the layout mechanics alone.
fn leases() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("coordination.k8s.io", "v1", "Lease"),
        plural: "leases".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

fn test_client(cx: &mut TestAppContext) -> kube::Client {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let _guard = handle.enter();
    kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
}

/// Spec: "Restored with the window" - a Leases list saves its kind, context,
/// namespace selection and column layout, and reading that back gives the same.
#[gpui_kit::test]
async fn a_list_panel_round_trips_kind_namespaces_and_columns(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let client = test_client(cx);
    let objects = cx.update(|cx| cx.new(|_| ObjectsTable::default()));
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(leases()), "kind-dev".into())
            .scoped_to(vec!["staging".into()]);
        let panel = cx.new(|cx| ObjectListPanel::with_table(leases(), scope, objects, client, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    // The user moves Age to the front and widens Name.
    vcx.update(|_, cx| {
        let table = panel.read(cx).table.clone().expect("the table is drawn");
        table.update(cx, |table, _| {
            let delegate = table.delegate_mut();
            delegate.reorder_columns(2, 0);
            delegate.set_widths(&[px(80.), px(330.), px(150.)]);
        });
    });

    let state = vcx.update(|_, cx| panel.read(cx).dump(cx));
    assert_eq!(state.panel_name, "ObjectList");
    let PanelInfo::Panel(data) = state.info else {
        panic!("a list panel saves panel state");
    };
    let saved = from_state(&data).expect("the state names its kind and cluster");
    assert_eq!(saved.kind, leases());
    assert_eq!(saved.context_name, "kind-dev");
    assert_eq!(saved.namespaces, ["staging"]);
    let ids: Vec<&str> = saved
        .columns
        .iter()
        .map(|column| column.id.as_str())
        .collect();
    assert_eq!(ids, ["age", "name", "namespace"]);
    assert_eq!(saved.columns[1].width, 330.);
}

/// A layout saved before list panels existed - a `Resource` placeholder, which saved
/// the kind but no columns - still reads, with the default column layout.
#[test]
fn a_pre_list_placeholders_state_still_restores() {
    let state = json!({
        "context_name": "kind-dev", "namespaces": [],
        "group": "apps", "version": "v1", "kind": "Deployment",
        "plural": "deployments", "namespaced": true,
    });
    let saved = from_state(&state).expect("the kind fields are all there");
    assert_eq!(saved.kind.gvk.kind, "Deployment");
    assert!(
        saved.columns.is_empty(),
        "no saved layout, so the defaults apply"
    );
    assert!(from_state(&json!({ "context_name": "kind-dev" })).is_none());
}

/// Data saved before `filter`/`sort` existed (`saved-panel-layouts` 1.6) still
/// restores, reading both as absent rather than failing.
#[test]
fn state_without_filter_or_sort_still_restores() {
    let state = json!({
        "context_name": "kind-dev", "namespaces": [],
        "group": "apps", "version": "v1", "kind": "Deployment",
        "plural": "deployments", "namespaced": true,
    });
    let saved = from_state(&state).expect("the kind fields are all there");
    assert_eq!(saved.filter, None);
    assert_eq!(saved.sort, None);
}

/// Three objects in `staging`, two named `web-*`, for the filter/sort round
/// trip below.
fn web_objects_fixture() -> ObjectsTable {
    use kube::api::{DynamicObject, ObjectMeta};
    use kube_runtime::watcher;

    let mut table = ObjectsTable::default();
    table.apply(watcher::Event::Init);
    for (uid, name) in [("u1", "web-a"), ("u2", "web-b"), ("u3", "other")] {
        table.apply(watcher::Event::InitApply(DynamicObject {
            types: None,
            metadata: ObjectMeta {
                uid: Some(uid.into()),
                name: Some(name.into()),
                namespace: Some("staging".into()),
                ..Default::default()
            },
            data: serde_json::Value::Null,
        }));
    }
    table.apply(watcher::Event::InitDone);
    table
}

/// Spec: "Namespaces and filters are saved and restored" - a list scoped to
/// two namespaces, with filter text entered and a descending sort, saves and
/// reads back the same namespaces, filter (with rows filtered by it), and
/// sort.
#[gpui_kit::test]
async fn filter_text_and_sort_are_saved_and_restored(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let client = test_client(cx);
    let objects = cx.update(|cx| cx.new(|_| web_objects_fixture()));
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(leases()), "kind-dev".into())
            .scoped_to(vec!["staging".into()]);
        let panel = cx.new(|cx| ObjectListPanel::with_table(leases(), scope, objects, client, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    // The user types a filter and sorts the Name column descending.
    vcx.update(|window, cx| {
        let filter = panel.update(cx, |panel, cx| panel.filter_input(window, cx));
        filter.update(cx, |filter, cx| filter.set_value("web", window, cx));
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        let table = panel.read(cx).table.clone().expect("the table is drawn");
        table.update(cx, |table, _| table.delegate_mut().set_sort("name", true));
    });

    let state = vcx.update(|_, cx| panel.read(cx).dump(cx));
    let PanelInfo::Panel(data) = state.info else {
        panic!("a list panel saves panel state");
    };
    let saved = from_state(&data).expect("the state names its kind and cluster");
    assert_eq!(saved.namespaces, ["staging"]);
    assert_eq!(saved.filter.as_deref(), Some("web"));
    assert_eq!(saved.sort, Some(("name".to_string(), true)));

    // Restoring with that saved state shows the same filter text, with rows
    // filtered by it and sorted descending by name - a fresh panel and table
    // over the same objects, as a restored window's would be.
    let client2 = test_client(cx);
    let objects2 = cx.update(|cx| cx.new(|_| web_objects_fixture()));
    let mut restored_built = None;
    let window2 = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(leases()), "kind-dev".into())
            .scoped_to(saved.namespaces.clone());
        let panel = cx.new(|cx| {
            let mut panel = ObjectListPanel::with_table(leases(), scope, objects2, client2, cx);
            panel.filter = ListSearch::restored(saved.filter.clone());
            panel.initial_sort = saved.sort.clone();
            panel
        });
        restored_built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let restored = restored_built.unwrap();
    let mut rvcx = VisualTestContext::from_window(window2.into(), cx);
    rvcx.run_until_parked();

    let filter_text = rvcx.update(|_, cx| restored.read(cx).filter.query(cx));
    assert_eq!(filter_text, "web");
    let names: Vec<String> = rvcx.update(|_, cx| {
        let table = restored
            .read(cx)
            .table
            .clone()
            .expect("the table is built on first render");
        table
            .read(cx)
            .delegate()
            .rows()
            .iter()
            .map(|row| row.object.name.clone())
            .collect()
    });
    assert_eq!(
        names,
        ["web-b", "web-a"],
        "filtered to the two `web-` rows, descending by name"
    );
}

/// D4: the placeholder only stands in once discovery has finished without the kind;
/// a kind still served, or discovery not yet done, restores as a list.
#[test]
fn only_a_kind_discovery_no_longer_reports_restores_as_a_placeholder() {
    let other = DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "ConfigMap"),
        plural: "configmaps".into(),
        namespaced: true,
        verbs: Default::default(),
    };
    assert!(
        !restores_as_placeholder(&leases(), None),
        "discovery still running"
    );
    assert!(!restores_as_placeholder(
        &leases(),
        Some(&[leases(), other.clone()])
    ));
    assert!(
        restores_as_placeholder(&leases(), Some(&[other])),
        "no longer served"
    );
}

/// The saved layout is the one a restored panel's table starts with.
#[gpui_kit::test]
async fn a_restored_layout_is_the_tables_starting_layout(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let client = test_client(cx);
    let objects = cx.update(|cx| cx.new(|_| ObjectsTable::default()));
    let saved = from_state(&json!({
        "context_name": "kind-dev", "namespaces": [],
        "group": "coordination.k8s.io", "version": "v1", "kind": "Lease", "plural": "leases",
        "namespaced": true,
        "columns": [{ "id": "namespace", "width": 120.0 }, { "id": "name", "width": 300.0 }],
    }))
    .unwrap();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(leases()), "kind-dev".into());
        let panel = cx.new(|cx| {
            let mut panel = ObjectListPanel::with_table(leases(), scope, objects, client, cx);
            panel.initial_layout = saved.columns;
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let table = vcx.update(|_, cx| panel.read(cx).table.clone().expect("the table is drawn"));
    let layout = vcx.update(|_, cx| table.read(cx).delegate().layout());
    let ids: Vec<&str> = layout.iter().map(|column| column.id.as_str()).collect();
    assert_eq!(
        ids,
        ["namespace", "name", "age"],
        "saved order first, then the rest"
    );
    assert_eq!(layout[0].width, 120.);
}
