//! `standard-resource-panels` 5.1 and 5.2 through a real window: a Services list
//! in the dock, driven with the app's own keymap - Down from no selection, then
//! `d` and `y` on the selected row opening its detail through the window's open
//! path.

use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_list::{ObjectListPanel, ObjectsTable};
use crate::k8s::resource::pod_detail::DetailView;
use crate::ui::nav::{NavTarget, ObjectTarget, OpenedPanel};
use crate::ui::panel_title::PanelScope;
use crate::util::shell::panels::{OpenPanel, PanelKey};
use crate::util::shell::test_support::{press, workspace};
use crate::util::shell::{MainWindow, WindowMode};
use gpui_kit::WindowHandle;
use gpui_kit::component::dock::{DockPlacement, PanelId, panel_handle};
use gpui_kit::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};
use kube::api::{DynamicObject, ObjectMeta};
use kube::core::GroupVersionKind;
use kube_runtime::watcher;

fn services() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
    }
}

fn service(name: &str) -> DynamicObject {
    DynamicObject {
        types: None,
        metadata: ObjectMeta {
            uid: Some(format!("uid-{name}")),
            name: Some(name.into()),
            namespace: Some("default".into()),
            ..Default::default()
        },
        data: serde_json::Value::Null,
    }
}

fn target(name: &str) -> NavTarget {
    NavTarget::Object(ObjectTarget {
        kind: services(),
        namespace: Some("default".into()),
        name: name.into(),
    })
}

struct Harness {
    window: WindowHandle<MainWindow>,
    vcx: VisualTestContext,
    list: Entity<ObjectListPanel>,
}

/// A workspace with a Services list docked beside Pods over two listed services,
/// `api` and `web`, filed as the window's own panel and focused - nothing selected.
fn harness(cx: &mut TestAppContext) -> Harness {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let client = {
        let handle = vcx.update(|_, cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    };
    let list = window
        .update(&mut vcx, |main_window, window, cx| {
            let objects = cx.new(|_| {
                let mut table = ObjectsTable::default();
                for name in ["api", "web"] {
                    table.apply(watcher::Event::Apply(service(name)));
                }
                table
            });
            let scope = PanelScope::new(NavTarget::Kind(services()), "kind-dev".into());
            let list = cx.new(|cx| {
                ObjectListPanel::with_table(services(), scope.clone(), objects, client, cx)
            });
            let WindowMode::Workspace {
                dock_area,
                open_panels,
                ..
            } = &mut main_window.mode
            else {
                panic!("a connected window is in workspace mode")
            };
            dock_area.update(cx, |area, cx| {
                area.add_panel_view(
                    panel_handle(list.clone()),
                    DockPlacement::Center,
                    None,
                    window,
                    cx,
                )
            });
            open_panels.push(OpenPanel {
                key: PanelKey::from(&scope),
                id: PanelId::from(list.entity_id()),
                panel: Some(OpenedPanel::ObjectList(list.clone())),
                group: None,
                _focus_watch: None,
            });
            list.read(cx).focus_handle(cx).focus(window, cx);
            list
        })
        .unwrap();
    vcx.run_until_parked();
    Harness { window, vcx, list }
}

fn detail_view(h: &mut Harness, name: &str) -> Option<DetailView> {
    let target = target(name);
    h.window
        .update(&mut h.vcx, |main_window, _, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                return None;
            };
            open_panels.iter().find_map(|open| match &open.panel {
                Some(OpenedPanel::ObjectDetail(panel)) if open.key.target == target => {
                    Some(panel.read(cx).view())
                }
                _ => None,
            })
        })
        .unwrap()
}

/// Shows the list again, as picking it in the Resource panel would.
fn show_list(h: &mut Harness) {
    h.window
        .update(&mut h.vcx, |main_window, window, cx| {
            main_window.open_target(NavTarget::Kind(services()), window, cx)
        })
        .unwrap();
    h.vcx.run_until_parked();
}

fn selected(h: &mut Harness) -> Option<usize> {
    h.vcx.update(|_, cx| h.list.read(cx).test_selected_row(cx))
}

/// 5.2 then 5.1: Down on the freshly focused list selects its first row, `d`
/// describes it; back on the list, `y` switches that same detail to its YAML.
#[gpui_kit::test]
async fn down_then_d_describes_the_first_row_and_y_shows_its_yaml(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    assert_eq!(selected(&mut h), None);

    press(&mut h.vcx, "down");
    assert_eq!(selected(&mut h), Some(0), "Down selects the first row");
    press(&mut h.vcx, "d");
    assert_eq!(detail_view(&mut h, "api"), Some(DetailView::Structured));

    show_list(&mut h);
    press(&mut h.vcx, "y");
    assert_eq!(
        detail_view(&mut h, "api"),
        Some(DetailView::Yaml),
        "the open detail switches to its YAML"
    );
}

/// 5.2 then 5.1: Up on the freshly focused list selects its last row, and `y`
/// opens that object's detail straight on its YAML.
#[gpui_kit::test]
async fn up_then_y_opens_the_last_rows_yaml(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(&mut h.vcx, "up");
    assert_eq!(selected(&mut h), Some(1), "Up selects the last row");
    press(&mut h.vcx, "y");
    assert_eq!(detail_view(&mut h, "web"), Some(DetailView::Yaml));
    assert_eq!(detail_view(&mut h, "api"), None);
}

/// Both are palette commands scoped to the list, filed under Navigate like the
/// Pods table's.
#[test]
fn describe_and_yaml_are_list_scoped_navigate_commands() {
    use crate::command::CommandRegistry;
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    for (id, key) in [("object_list.describe", "d"), ("object_list.yaml", "y")] {
        let command = registry
            .get(id)
            .unwrap_or_else(|| panic!("{id} registered"));
        assert_eq!(command.default_binding, key);
        assert_eq!(
            command.context,
            Some(crate::k8s::resource::object_list::LIST_KEY_CONTEXT)
        );
        // Panel-scoped, so out of the menu bar (`menu-organization`).
        assert_eq!(command.menu, None);
    }
}
