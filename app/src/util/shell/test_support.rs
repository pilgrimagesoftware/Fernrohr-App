//! Helpers the shell's tests share: a connected window, scratch paths, and
//! fixture kinds.

// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::util::shell::{MainWindow, PanelDescriptor, WindowMode};
use gpui_kit::component::dock::{self, PanelView as _};
use gpui_kit::{App, AppContext as _, Entity, SharedString, TestAppContext, Window};
use kube::core::GroupVersionKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) static COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn temp_workspace_path() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("fernrohr-shell-test-{n}.toml"))
}

pub(super) fn temp_tunnels_path() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("fernrohr-shell-set-tunnel-test-{n}.toml"));
    let _ = std::fs::remove_file(&path);
    path
}

/// A connected window on `context_name`, for the panel-opening tests.
///
/// `enter_workspace` opens a real Pods panel (`nav::add_panel` always
/// builds one via `PodsPanel::new`, not the `with_stubs` seam
/// `pods::tests` uses), which starts a genuine `ClusterConnection::connect`,
/// a tokio task that resolves a kubeconfig and probes a server, then
/// wakes its GPUI observer from that tokio thread. `allow_parking` is the
/// same seam `cluster::session`'s and `cluster::connection`'s own tests
/// use for this exact reason: without it, the wakeup races the test
/// scheduler's thread-confinement check non-deterministically, since it
/// depends on real wall-clock I/O timing rather than anything these tests
/// control.
pub(super) async fn connected_window(
    cx: &mut TestAppContext,
    context_name: &str,
) -> gpui_kit::WindowHandle<MainWindow> {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec![context_name.to_string()], window, cx);
        main_window
    })
}

/// A kind with no concrete panel, as discovery would report a CRD's.
pub(super) fn crd_kind() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("ferns.example.com", "v1", "Fern"),
        plural: "ferns".to_string(),
        namespaced: true,
    }
}

/// A cluster-scoped CRD, the kind 10.2 says must not grow a namespace picker.
pub(super) fn cluster_scoped_kind() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("widgets.example.com", "v1", "Widget"),
        plural: "widgets".to_string(),
        namespaced: false,
    }
}

/// The title bar the dock builds for `panel`, read back the way the dock
/// reads it: through `PanelView`, which is the object-safe face of `Panel`
/// and takes no panel context.
///
/// Returns the tab name, whether a namespace picker is on the bar, and how
/// many controls sit at its trailing end.
/// The namespace picker moved out of the title bar into each panel's own
/// body (Paul's feedback: a picker shared across a tab group's title bar
/// was ambiguous about which tab it scoped), so this reads only what
/// still lives in the dock's title bar: the name and the toolbar
/// controls.
pub(super) fn title_bar_of<T: dock::Panel>(
    panel: &Entity<T>,
    window: &mut Window,
    cx: &mut App,
) -> (Option<SharedString>, usize) {
    let name = panel.tab_name(cx);
    let controls = panel
        .toolbar_buttons(window, cx)
        .map_or(0, |buttons| buttons.len());
    (name, controls)
}

pub(super) fn pods_panel_descriptor(cluster_context: &str) -> PanelDescriptor {
    PanelDescriptor::Pods {
        cluster_context: cluster_context.to_string(),
        namespace: crate::config::workspace::NamespaceScope::All,
        filter: String::new(),
        sort: crate::config::workspace::SortState {
            column: "name".into(),
            ascending: true,
        },
    }
}
