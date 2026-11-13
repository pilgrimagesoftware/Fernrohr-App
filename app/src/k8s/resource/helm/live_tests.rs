//! helm-releases 9: the Helm code paths against a real cluster and a real
//! `helm`, run by `scripts/helm-integration.sh` on a throwaway minikube
//! profile it creates and deletes. `#[ignore]`d: `cargo test` never runs it.
//! Without the script's variables it skips (CI runs `--include-ignored`), and
//! it refuses any context outside the script's profile, so it can't reach the
//! user's own clusters.
//!
//! The script installs release `it-web` in `fernrohr-it` from the chart
//! `helm create` scaffolds, then upgrades it twice with `replicaCount` 1, 2
//! and 3. This test lists it, reads its history and values, checks its
//! resources, rolls it back to revision 1 and uninstalls it - through the
//! app's own watch, panels and `helm` runner.

use crate::command::CommandRegistry;
use crate::helm_cli::{self, HelmAction, HelmStatus};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::helm::ReleaseKey;
use crate::k8s::resource::helm::detail::{HelmReleasePanel, ReleaseTab};
use crate::k8s::resource::helm::list::HelmReleasesPanel;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use kube::config::{KubeConfigOptions, Kubeconfig};
use std::time::Duration;

const NAMESPACE: &str = "fernrohr-it";
const RELEASE: &str = "it-web";

/// The script's profile, and so its context: the test only drives a context
/// with this prefix, so pointing the variables at a real kubeconfig can't roll
/// back or uninstall anything outside the throwaway profile.
const PROFILE_PREFIX: &str = "fernrohr-helm-it";

/// The script's kubeconfig and context, or `None` when the variables aren't
/// set. CI's macOS jobs run with `--include-ignored` (for the keychain tests),
/// so without the script's cluster this test must skip, not fail.
fn cluster() -> Option<(Kubeconfig, String)> {
    let (Ok(path), Ok(context)) = (
        std::env::var("FERNROHR_HELM_IT_KUBECONFIG"),
        std::env::var("FERNROHR_HELM_IT_CONTEXT"),
    ) else {
        return None;
    };
    assert!(
        context.starts_with(PROFILE_PREFIX),
        "refusing context {context:?}: only the script's {PROFILE_PREFIX} profile"
    );
    let kubeconfig = Kubeconfig::read_from(&path).expect("the script's kubeconfig reads");
    Some((kubeconfig, context))
}

/// Removes the runner's scratch directory however the test ends.
struct RunDir(std::path::PathBuf);

impl Drop for RunDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs `cx` until `done`, polling for up to `timeout`.
fn wait(
    vcx: &mut VisualTestContext,
    what: &str,
    timeout: Duration,
    done: impl Fn(&mut VisualTestContext) -> bool,
) {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        vcx.run_until_parked();
        if done(vcx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("never {what} within {timeout:?}");
}

#[gpui_kit::test]
#[ignore = "needs scripts/helm-integration.sh's minikube profile"]
async fn helm_live_releases_round_trip(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((kubeconfig, context)) = cluster() else {
        eprintln!("skipping: run through scripts/helm-integration.sh");
        return;
    };
    let handle = cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::runtime::handle(cx)
    });
    let client = handle.block_on(async {
        let options = KubeConfigOptions {
            context: Some(context.clone()),
            ..Default::default()
        };
        let config = kube::Config::from_custom_kubeconfig(kubeconfig.clone(), &options)
            .await
            .expect("the minikube context configures");
        kube::Client::try_from(config).expect("a client")
    });
    let helm = which_helm();
    let run_dir =
        RunDir(std::env::temp_dir().join(format!("fernrohr-helm-it-{}", std::process::id())));
    std::fs::create_dir_all(&run_dir.0).expect("the run directory creates");
    cx.update(|cx| {
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::helm::list::register_commands(&mut registry);
        crate::k8s::resource::helm::detail::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        ClusterRegistry::insert_test_session(cx, &context, ConnectionState::Connected(client));
        DiscoveryRegistry::insert_test(cx, &context, kinds());
        helm_cli::set_status(HelmStatus::Ready(helm), cx);
        cx.set_global(helm_cli::TestRunEnv {
            kubeconfig,
            path_env: std::env::var("PATH").unwrap_or_default(),
            dir: run_dir.0.clone(),
        });
    });

    // The list: one row for it-web at revision 3, its chart read.
    let scope = PanelScope::new(NavTarget::HelmReleases, context.clone())
        .scoped_to(vec![NAMESPACE.to_string()]);
    let (list, mut vcx) = open(cx, |cx| HelmReleasesPanel::new(scope, cx));
    wait(
        &mut vcx,
        "listed it-web at revision 3 with its chart",
        Duration::from_secs(60),
        |vcx| {
            vcx.update(gpui_kit::test::TestWindowExt::render_frame);
            vcx.update(|_, cx| {
                list.read(cx).displayed(cx).iter().any(|row| {
                    row.name == RELEASE && row.revision == 3 && row.chart_text() == "demo-0.1.0"
                })
            })
        },
    );

    // The release: history 3, 2, 1; revision 3's values; its resources present.
    let key = ReleaseKey {
        namespace: NAMESPACE.into(),
        name: RELEASE.into(),
    };
    let detail_scope = PanelScope::new(NavTarget::HelmRelease(key.clone()), context.clone());
    let (detail, mut dvcx) = open(cx, {
        let key = key.clone();
        move |cx| HelmReleasePanel::new(key, detail_scope, cx)
    });
    wait(
        &mut dvcx,
        "read the history",
        Duration::from_secs(30),
        |vcx| {
            vcx.update(|_, cx| {
                let panel = detail.read(cx);
                let versions: Vec<u32> = panel.history(cx).iter().map(|r| r.version).collect();
                versions == [3, 2, 1]
            })
        },
    );
    dvcx.simulate_keystrokes("3");
    dvcx.simulate_keystrokes("r");
    wait(
        &mut dvcx,
        "revealed revision 3's values",
        Duration::from_secs(30),
        |vcx| {
            vcx.update(|_, cx| {
                detail
                    .read(cx)
                    .revealed_text(ReleaseTab::Values, cx)
                    .is_some_and(|values| values.contains("replicaCount: 3"))
            })
        },
    );
    dvcx.simulate_keystrokes("6");
    wait(
        &mut dvcx,
        "found every resource present",
        Duration::from_secs(30),
        |vcx| {
            vcx.update(|_, cx| {
                let rows = detail.read(cx).resource_rows().to_vec();
                let kinds: Vec<String> = rows.iter().map(|row| row.target.kind.clone()).collect();
                ["Deployment", "Service", "ServiceAccount"]
                    .iter()
                    .all(|kind| kinds.iter().any(|found| found == kind))
                    && rows.iter().all(|row| {
                        row.presence == crate::k8s::resource::helm::detail::Presence::Present
                    })
            })
        },
    );

    // Rollback to 1, through the real helm: revision 4 appears, a rollback.
    let release = helm_cli::ReleaseKey {
        context: context.clone(),
        namespace: NAMESPACE.into(),
        release: RELEASE.into(),
    };
    let rolled = dvcx
        .update(|_, cx| helm_cli::start(release.clone(), HelmAction::Rollback { revision: 1 }, cx));
    rolled.await.expect("helm rolled it back");
    wait(
        &mut dvcx,
        "showed revision 4 as a rollback to 1",
        Duration::from_secs(60),
        |vcx| {
            vcx.update(|_, cx| {
                let panel = detail.read(cx);
                panel.history(cx).first().map(|revision| revision.version) == Some(4)
                    && panel
                        .detail(4)
                        .and_then(|detail| detail.info.description.clone())
                        .is_some_and(|description| description.contains("Rollback to 1"))
            })
        },
    );

    // Uninstall: the list drops it, and the release panel says it's gone.
    let removed = dvcx.update(|_, cx| helm_cli::start(release.clone(), HelmAction::Uninstall, cx));
    removed.await.expect("helm uninstalled it");
    wait(
        &mut dvcx,
        "said the release is gone",
        Duration::from_secs(60),
        |vcx| vcx.update(|_, cx| detail.read(cx).gone(cx)),
    );
    wait(
        &mut vcx,
        "dropped it from the list",
        Duration::from_secs(60),
        |vcx| {
            vcx.update(gpui_kit::test::TestWindowExt::render_frame);
            vcx.update(|_, cx| list.read(cx).displayed(cx).is_empty())
        },
    );
}

/// Opens `build`'s panel in a window of its own, focused.
fn open<P: gpui_kit::Render + gpui_kit::Focusable + 'static>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut gpui_kit::Context<P>) -> P,
) -> (Entity<P>, VisualTestContext) {
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(build);
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("built");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let focus = panel.clone();
    vcx.update(|window, cx| focus.read(cx).focus_handle(cx).focus(window, cx));
    vcx.run_until_parked();
    (panel, vcx)
}

/// The `helm` on PATH, as the app would find it.
fn which_helm() -> helm_cli::HelmBinary {
    let path = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("helm"))
        .find(|candidate| candidate.is_file())
        .expect("helm on PATH");
    let version = std::process::Command::new(&path)
        .args(["version", "--short"])
        .output()
        .expect("helm runs");
    helm_cli::HelmBinary {
        path,
        version: String::from_utf8_lossy(&version.stdout).trim().to_string(),
    }
}

/// The kinds the scaffold chart creates, as discovery reports them.
fn kinds() -> Vec<DiscoveredKind> {
    use kube::core::GroupVersionKind;
    let kind = |group: &str, version: &str, name: &str, plural: &str| {
        let mut kind = DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, version, name),
            plural: plural.into(),
            namespaced: true,
            verbs: Default::default(),
        };
        kind.verbs.list = true;
        kind
    };
    vec![
        kind("apps", "v1", "Deployment", "deployments"),
        kind("", "v1", "Service", "services"),
        kind("", "v1", "ServiceAccount", "serviceaccounts"),
    ]
}
