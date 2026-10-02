// Named imports rather than `use super::*`: the parent's `gpui_kit::*` would
// shadow the built-in `#[test]`.
use super::ObjectListPanel;
use crate::command::CommandRegistry;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_list::ObjectsTable;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::component::table::ColumnSort;
use gpui_kit::{
    AppContext as _, Entity, Focusable as _, Keystroke, TestAppContext, VisualTestContext,
};
use kube::api::{DynamicObject, ObjectMeta};
use kube::core::GroupVersionKind;
use kube_runtime::watcher;

fn deployments() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("apps", "v1", "Deployment"),
        plural: "deployments".into(),
        namespaced: true,
    }
}

fn nodes() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Node"),
        plural: "nodes".into(),
        namespaced: false,
    }
}

fn object(name: &str, namespace: Option<&str>) -> DynamicObject {
    DynamicObject {
        types: None,
        metadata: ObjectMeta {
            uid: Some(format!("uid-{name}")),
            name: Some(name.into()),
            namespace: namespace.map(Into::into),
            ..Default::default()
        },
        data: serde_json::Value::Null,
    }
}

struct Harness {
    vcx: VisualTestContext,
    panel: Entity<ObjectListPanel>,
}

/// A connected list panel of `kind` in a `Root`, with the registry's real bindings,
/// over a table holding `objects` - no watch, no cluster.
fn harness(cx: &mut TestAppContext, kind: DiscoveredKind, objects: Vec<DynamicObject>) -> Harness {
    harness_with(cx, kind, objects, None)
}

/// [`harness`], with the server's `refused` message already on the table.
fn harness_with(
    cx: &mut TestAppContext,
    kind: DiscoveredKind,
    objects: Vec<DynamicObject>,
    refused: Option<&str>,
) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_list::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let client = {
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    };
    let table = cx.update(|cx| {
        cx.new(|_| {
            let mut table = ObjectsTable::default();
            for object in objects {
                table.apply(watcher::Event::Apply(object));
            }
            if let Some(refused) = refused {
                table.set_refused(refused.into());
            }
            table
        })
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(kind.clone()), "kind-dev".into());
        let panel = cx.new(|cx| ObjectListPanel::with_table(kind, scope, table, client, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let vcx = VisualTestContext::from_window(window.into(), cx);
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

fn column_ids(h: &mut Harness) -> Vec<String> {
    h.vcx.update(|_, cx| {
        let table = h.panel.read(cx).table.clone().expect("the table is drawn");
        table
            .read(cx)
            .delegate()
            .columns()
            .iter()
            .map(|column| column.id.to_string())
            .collect()
    })
}

fn row_names(h: &mut Harness) -> Vec<String> {
    h.vcx.update(|_, cx| {
        let table = h.panel.read(cx).table.clone().expect("the table is drawn");
        table
            .read(cx)
            .delegate()
            .rows()
            .iter()
            .map(|row| row.object.name.clone())
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

/// Spec: a namespaced kind's list has a Namespace column; a cluster-scoped kind's
/// (Nodes) has none, and no namespace picker.
#[gpui_kit::test]
async fn a_namespaced_kind_shows_namespace_and_a_cluster_scoped_kind_does_not(
    cx: &mut TestAppContext,
) {
    let mut h = harness(cx, deployments(), vec![object("web", Some("staging"))]);
    assert_eq!(column_ids(&mut h), ["name", "namespace", "age"]);
    let title = h
        .vcx
        .update(|_, cx| crate::ui::panel_title::title(&h.panel.read(cx).scope));
    assert!(title.starts_with("Deployments"), "a plural title: {title}");

    let mut h = harness(cx, nodes(), vec![object("node-a", None)]);
    assert_eq!(column_ids(&mut h), ["name", "age"]);
    let namespaced = h.vcx.update(|_, cx| h.panel.read(cx).scope.is_namespaced());
    assert!(!namespaced, "no namespace picker for a cluster-scoped kind");
}

/// `/` focuses the filter from the table; typing narrows rows by name - `w` types
/// there, it doesn't warp the namespace, though a row is selected for it to warp
/// to - and Escape clears it and returns to the table.
#[gpui_kit::test]
async fn the_filter_narrows_rows_by_keyboard_and_escape_clears_it(cx: &mut TestAppContext) {
    let mut h = harness(
        cx,
        deployments(),
        vec![
            object("web", Some("a")),
            object("worker", Some("b")),
            object("api", Some("a")),
        ],
    );
    focus_table(&mut h);
    press(&mut h.vcx, "down");
    let selected = h.vcx.update(|_, cx| {
        h.panel
            .read(cx)
            .table
            .clone()
            .unwrap()
            .read(cx)
            .selected_row()
    });
    assert!(
        selected.is_some(),
        "a row is selected, so a stray warp would move the scope"
    );

    press(&mut h.vcx, "/ w e");
    assert_eq!(
        row_names(&mut h),
        ["web"],
        "`we` matches web only; `e` alone would add worker"
    );
    let scoped = h
        .vcx
        .update(|_, cx| h.panel.read(cx).scope.namespaces.clone());
    assert!(
        scoped.is_empty(),
        "`w` typed into the filter rather than warping"
    );

    press(&mut h.vcx, "escape");
    assert_eq!(
        row_names(&mut h).len(),
        3,
        "clearing the filter restores every row"
    );
    let table_focused = h.vcx.update(|window, cx| {
        let table = h.panel.read(cx).table.clone().unwrap();
        table.read(cx).focus_handle(cx).is_focused(window)
    });
    assert!(table_focused, "focus is back on the table");
}

/// Rows sort by the clicked column and keep sorting as the panel re-renders.
#[gpui_kit::test]
async fn rows_sort_by_column(cx: &mut TestAppContext) {
    let mut h = harness(
        cx,
        nodes(),
        vec![
            object("node-c", None),
            object("node-a", None),
            object("node-b", None),
        ],
    );
    let table = h
        .vcx
        .update(|_, cx| h.panel.read(cx).table.clone().unwrap());
    h.vcx.update(|_, cx| {
        table.update(cx, |table, _| {
            table.delegate_mut().resort(0, ColumnSort::Descending)
        })
    });
    h.vcx.update(|window, _| window.refresh());
    h.vcx.run_until_parked();
    assert_eq!(row_names(&mut h), ["node-c", "node-b", "node-a"]);
}

/// Spec: "A kind the user cannot list" - the panel draws the server's refusal in
/// place of rows: no table is built, so there's no empty table to misread.
#[gpui_kit::test]
async fn a_refused_kind_shows_the_refusal_instead_of_a_table(cx: &mut TestAppContext) {
    let mut h = harness_with(
        cx,
        deployments(),
        Vec::new(),
        Some("deployments.apps is forbidden"),
    );
    let built = h.vcx.update(|_, cx| h.panel.read(cx).table.is_some());
    assert!(!built, "no table for a refused kind");

    let mut h = harness(cx, deployments(), Vec::new());
    let built = h.vcx.update(|_, cx| h.panel.read(cx).table.is_some());
    assert!(built, "an empty but listable kind still gets its table");
}
