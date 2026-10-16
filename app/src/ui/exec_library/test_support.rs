//! A real main window on `demo` over a fake cluster - three Running `web` pods
//! of a `web` Deployment, one Running `api` pod - with the Exec Library opened
//! from it by a key (`exec_library.open` has none by default, so a test
//! `keymap.toml` binds F12), for the window's and the dialog's keystroke tests.

use super::store::ExecLibraryStore;
use crate::config::exec_library::{Entry, ExecLibrary, Target};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::exec::ExecTarget;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::util::shell::MainWindow;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use serde_json::json;

pub(crate) const OPEN_KEY: &str = "f12";
/// Run from Library… and Save to Library…, which have no keys by default.
pub(crate) const RUN_FROM_KEY: &str = "f11";
pub(crate) const SAVE_FROM_KEY: &str = "f10";

pub(crate) struct Harness {
    pub(crate) main: Entity<MainWindow>,
    pub(crate) vcx: VisualTestContext,
}

pub(crate) fn entry(name: &str, command: &str, target: Target) -> Entry {
    Entry {
        name: name.into(),
        command: command.into(),
        container: None,
        target,
    }
}

pub(crate) fn selector(selector: &str) -> Target {
    Target::Selector {
        namespace: Some("shop".into()),
        selector: selector.into(),
    }
}

/// The workspace, its Pods panel focused, the library holding `entries`.
pub(crate) fn open(cx: &mut TestAppContext, entries: Vec<Entry>) -> Harness {
    cx.executor().allow_parking();
    let keymap = crate::util::test_paths::temp_path("exec-library-keymap");
    std::fs::write(
        &keymap,
        format!(
            "[bindings]\n\"exec_library.open\" = \"{OPEN_KEY}\"\n\
             \"exec_library.run_from\" = \"{RUN_FROM_KEY}\"\n\
             \"exec_library.save_from\" = \"{SAVE_FROM_KEY}\"\n"
        ),
    )
    .expect("the test keymap is written");
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(
            cx,
            crate::util::test_paths::temp_path("exec-library-workspace"),
            &keymap,
        );
        ExecLibraryStore::set_for_test(ExecLibrary { entries }, cx);
    });
    let (cluster, client) = FakeCluster::start(cx);
    for (name, app) in [
        ("web-2", "web"),
        ("web-1", "web"),
        ("web-3", "web"),
        ("api-1", "api"),
    ] {
        cluster.apply(
            "/api/v1",
            "pods",
            json!({ "apiVersion": "v1", "kind": "Pod",
                "metadata": { "name": name, "namespace": "shop", "uid": name,
                    "labels": { "app": app } },
                "spec": { "containers": [{ "name": "app" }] },
                "status": { "phase": "Running" } }),
        );
    }
    cluster.apply(
        "/apis/apps/v1",
        "deployments",
        json!({ "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": { "name": "web", "namespace": "shop", "uid": "deploy-web" },
            "spec": { "selector": { "matchLabels": { "app": "web" } } } }),
    );
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connected(client));
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("built");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.activate_window();
        let pods = main
            .read(cx)
            .test_panel_view_for(&NavTarget::pods(), cx)
            .expect("the workspace lands on Pods");
        let focus = pods.focus_handle(cx);
        window.focus(&focus, cx);
    });
    Harness { main, vcx }
}

impl Harness {
    pub(crate) fn settle(&mut self) {
        for _ in 0..20 {
            self.vcx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Opens the library by its key, from the focused Pods panel, and returns
    /// a context for its window.
    pub(crate) fn open_library(&mut self) -> VisualTestContext {
        self.vcx.simulate_keystrokes(OPEN_KEY);
        self.settle();
        let handle = self
            .vcx
            .update(|_, cx| super::window::test_handle(cx))
            .expect("the library window opened");
        let library = VisualTestContext::from_window(handle.into(), &self.vcx);
        library.run_until_parked();
        library
    }

    /// The library command sessions the main window has open.
    pub(crate) fn sessions(&mut self) -> Vec<ExecTarget> {
        self.vcx.update(|_, cx| {
            self.main
                .read(cx)
                .test_open_targets()
                .into_iter()
                .filter_map(|target| match target {
                    NavTarget::Exec(exec) => Some(exec),
                    _ => None,
                })
                .collect()
        })
    }

    pub(crate) fn names(&mut self) -> Vec<String> {
        self.vcx.update(|_, cx| {
            ExecLibraryStore::get(cx)
                .entries
                .iter()
                .map(|entry| entry.name.clone())
                .collect()
        })
    }
}

/// Presses `keys` in `vcx`'s window and lets it settle.
pub(crate) fn press(vcx: &mut VisualTestContext, keys: &str) {
    vcx.simulate_keystrokes(keys);
    for _ in 0..10 {
        vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

pub(crate) fn dialog_open(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.has_active_dialog(cx))
}
