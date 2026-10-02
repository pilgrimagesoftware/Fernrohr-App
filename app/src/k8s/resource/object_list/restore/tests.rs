// Named imports rather than `use super::*`: the parent's `gpui_kit::*` would
// shadow the built-in `#[test]`.
use super::{from_state, restores_as_placeholder};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_list::{ObjectListPanel, ObjectsTable};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::component::dock::BasePanel as _;
use gpui_kit::component::dock::PanelInfo;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext, px};
use kube::core::GroupVersionKind;
use serde_json::json;

fn services() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
    }
}

fn test_client(cx: &mut TestAppContext) -> kube::Client {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let _guard = handle.enter();
    kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
}

/// Spec: "Restored with the window" - a Services list saves its kind, context,
/// namespace selection and column layout, and reading that back gives the same.
#[gpui_kit::test]
async fn a_list_panel_round_trips_kind_namespaces_and_columns(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let client = test_client(cx);
    let objects = cx.update(|cx| cx.new(|_| ObjectsTable::default()));
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(services()), "kind-dev".into())
            .scoped_to(vec!["staging".into()]);
        let panel =
            cx.new(|cx| ObjectListPanel::with_table(services(), scope, objects, client, cx));
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
    assert_eq!(saved.kind, services());
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

/// D4: the placeholder only stands in once discovery has finished without the kind;
/// a kind still served, or discovery not yet done, restores as a list.
#[test]
fn only_a_kind_discovery_no_longer_reports_restores_as_a_placeholder() {
    let other = DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "ConfigMap"),
        plural: "configmaps".into(),
        namespaced: true,
    };
    assert!(
        !restores_as_placeholder(&services(), None),
        "discovery still running"
    );
    assert!(!restores_as_placeholder(
        &services(),
        Some(&[services(), other.clone()])
    ));
    assert!(
        restores_as_placeholder(&services(), Some(&[other])),
        "no longer served"
    );
}

/// The saved layout is the one a restored panel's table starts with.
#[gpui_kit::test]
async fn a_restored_layout_is_the_tables_starting_layout(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let client = test_client(cx);
    let objects = cx.update(|cx| cx.new(|_| ObjectsTable::default()));
    let saved = from_state(&json!({
        "context_name": "kind-dev", "namespaces": [],
        "group": "", "version": "v1", "kind": "Service", "plural": "services",
        "namespaced": true,
        "columns": [{ "id": "namespace", "width": 120.0 }, { "id": "name", "width": 300.0 }],
    }))
    .unwrap();
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(services()), "kind-dev".into());
        let panel = cx.new(|cx| {
            let mut panel = ObjectListPanel::with_table(services(), scope, objects, client, cx);
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
