//! `remembered-list-sort` in a Pods panel, in a real window with the app's own
//! keymap (`util::shell::init`): it starts on its own sort, the remembered
//! Pods sort, or Name ascending, and the keyboard's sort commands cycle it and
//! are remembered under `/Pod`.

use crate::config::workspace::SortState;
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::PodsPanel;
use crate::k8s::resource::pods::test_support::pod_in;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::util::shell::SortDefaults;
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext as _, Entity, Focusable as _, Keystroke, TestAppContext, VisualTestContext,
};
use kube_runtime::watcher;

const CONTEXT: &str = "sort-pods";
const KEY: &str = "/Pod";

struct Harness {
    vcx: VisualTestContext,
    panel: Entity<PodsPanel>,
}

/// A focused Pods panel listing `web` (team-c), `api` (team-b) and `db`
/// (team-a), with `setup` run on it before its table is built.
fn pods(cx: &mut TestAppContext, setup: impl FnOnce(&mut PodsPanel)) -> Harness {
    cx.executor().allow_parking();
    let path = crate::util::test_paths::temp_path("pods-sort");
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, path.clone(), &path);
    });
    let client = {
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    };
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            CONTEXT,
            ConnectionState::Connected(client.clone()),
        )
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let connection =
                cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
            let namespaces = cx.new(|_| NamespaceList::empty());
            let mut panel = PodsPanel::with_connection(
                PanelScope::new(NavTarget::pods(), CONTEXT.to_string()),
                connection,
                namespaces,
                cx,
            );
            setup(&mut panel);
            panel
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        let table = panel.read(cx).table.clone();
        table.update(cx, |table, cx| {
            table.apply(watcher::Event::Init);
            for (namespace, name) in [("team-c", "web"), ("team-b", "api"), ("team-a", "db")] {
                let pod = pod_in(namespace, &format!("uid-{name}"), name, 0);
                table.apply(watcher::Event::InitApply(pod));
            }
            table.apply(watcher::Event::InitDone);
            cx.notify();
        });
        panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    vcx.run_until_parked();
    Harness { vcx, panel }
}

fn press(h: &mut Harness, keys: &str) {
    for key in keys.split(' ') {
        h.vcx
            .simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
    }
    h.vcx.run_until_parked();
}

fn sort_of(h: &mut Harness) -> Option<(String, bool)> {
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
            .sort_state()
            .map(|(column, descending)| (column.to_string(), descending))
    })
}

fn names(h: &mut Harness) -> Vec<String> {
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
            .map(|row| row.selection.name.clone())
            .collect()
    })
}

fn sorted(column: &str, descending: bool) -> Option<(String, bool)> {
    Some((column.to_string(), descending))
}

fn remembered(h: &mut Harness) -> Option<SortState> {
    h.vcx.update(|_, cx| SortDefaults::get(cx, KEY))
}

fn remember(cx: &mut TestAppContext, column: &str, ascending: bool) {
    cx.update(|cx| {
        SortDefaults::record(
            cx,
            KEY,
            SortState {
                column: column.into(),
                ascending,
            },
        )
    });
}

#[gpui_kit::test]
async fn a_pods_panel_opens_on_name_ascending(cx: &mut TestAppContext) {
    let mut h = pods(cx, |_| {});
    assert_eq!(sort_of(&mut h), sorted("name", false));
    assert_eq!(names(&mut h), ["api", "db", "web"]);
    assert_eq!(remembered(&mut h), None);
}

#[gpui_kit::test]
async fn a_pods_panel_opens_with_the_remembered_pods_sort(cx: &mut TestAppContext) {
    remember(cx, "namespace", true);
    let mut h = pods(cx, |_| {});
    assert_eq!(sort_of(&mut h), sorted("namespace", false));
    assert_eq!(names(&mut h), ["db", "api", "web"]);
}

#[gpui_kit::test]
async fn a_restored_pods_panel_keeps_its_own_sort_and_records_nothing(cx: &mut TestAppContext) {
    remember(cx, "namespace", true);
    let mut h = pods(cx, |panel| {
        panel.initial_sort = Some(("restarts".into(), true))
    });
    assert_eq!(sort_of(&mut h), sorted("restarts", true));
    assert_eq!(
        remembered(&mut h),
        Some(SortState {
            column: "namespace".into(),
            ascending: true,
        })
    );
}

#[gpui_kit::test]
async fn the_keyboard_cycles_a_pods_sort_and_remembers_it(cx: &mut TestAppContext) {
    let mut h = pods(cx, |_| {});
    press(&mut h, "shift-.");
    assert_eq!(sort_of(&mut h), sorted("namespace", false));
    press(&mut h, "o");
    assert_eq!(sort_of(&mut h), sorted("namespace", true));
    assert_eq!(names(&mut h), ["web", "api", "db"]);
    assert_eq!(
        remembered(&mut h),
        Some(SortState {
            column: "namespace".into(),
            ascending: false,
        })
    );
    press(&mut h, "o");
    assert_eq!(
        sort_of(&mut h),
        sorted("name", false),
        "back to the default"
    );
}
