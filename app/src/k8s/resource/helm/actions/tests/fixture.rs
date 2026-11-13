//! A fake cluster with `web` in `shop` at revisions 1-3 (3 deployed, its
//! manifest two objects), a fake `helm` that records how it was run, and the
//! app's keymap - what every section 8 window test starts from.

use crate::command::CommandRegistry;
use crate::helm_cli::{HelmBinary, HelmStatus, TestRunEnv, script, set_status, temp_dir};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::{self, KeymapConfig};
use base64::Engine as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use kube::config::Kubeconfig;
use serde_json::{Value, json};
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub(super) const CONTEXT: &str = "kind-dev";

const MANIFEST: &str = "---\n# Source: web/templates/service.yaml\napiVersion: v1\nkind: Service\n\
                        metadata:\n  name: web\n---\n# Source: web/templates/deployment.yaml\n\
                        apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n";

/// Helm's revision Secret for `web` revision `version`.
pub(super) fn revision(version: u32, status: &str) -> Value {
    let release = json!({
        "name": "web", "namespace": "shop", "version": version,
        "info": { "status": status, "description": format!("Upgrade {version} complete") },
        "chart": { "metadata": { "name": "web", "version": format!("1.{version}.0"), "appVersion": "2.0" } },
        "manifest": MANIFEST,
    });
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(release.to_string().as_bytes()).unwrap();
    let helm = base64::engine::general_purpose::STANDARD.encode(gz.finish().unwrap());
    json!({
        "apiVersion": "v1", "kind": "Secret", "type": "helm.sh/release.v1",
        "metadata": {
            "name": format!("sh.helm.release.v1.web.v{version}"),
            "namespace": "shop",
            "labels": { "owner": "helm", "name": "web", "version": version.to_string(), "status": status },
        },
        "data": { "release": base64::engine::general_purpose::STANDARD.encode(helm) },
    })
}

/// What one test's fake `helm` leaves behind.
pub(super) struct FakeHelm {
    /// The file it writes its arguments to, one per line.
    pub(super) seen: PathBuf,
}

impl FakeHelm {
    /// The arguments it ran with, or `None` if it never ran.
    pub(super) fn args(&self) -> Option<Vec<String>> {
        let seen = std::fs::read_to_string(&self.seen).ok()?;
        Some(seen.lines().map(str::to_string).collect())
    }
}

/// The app's keymap, the cluster with `web`, the context connected, and a
/// fake `helm` 3 that records its arguments and then runs `then` (its exit).
pub(super) fn start(cx: &mut TestAppContext, label: &str, then: &str) -> (FakeCluster, FakeHelm) {
    cx.executor().allow_parking();
    let dir = temp_dir(label);
    let seen = dir.join("seen");
    let helm = script(
        &dir,
        "helm",
        // Written whole, then moved into place: the file appears complete.
        &format!(
            "printf '%s\\n' \"$@\" > '{seen}.tmp'\nmv '{seen}.tmp' '{seen}'\n{then}",
            seen = seen.display()
        ),
    );
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::util::shell::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        // Up and Down move a list's selection outside the registry.
        cx.bind_keys(crate::ui::list_keys::bindings(&[
            crate::k8s::resource::helm::list::LIST_KEY_CONTEXT,
        ]));
        set_status(
            HelmStatus::Ready(HelmBinary {
                path: helm.clone(),
                version: "v3.14.2".into(),
            }),
            cx,
        );
        cx.set_global(run_env(&dir));
    });
    let (cluster, client) = FakeCluster::start(cx);
    for (version, status) in [(1, "superseded"), (2, "superseded"), (3, "deployed")] {
        cluster.apply("/api/v1", "secrets", revision(version, status));
    }
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    (cluster, FakeHelm { seen })
}

/// A kubeconfig naming the test context, and a PATH with no login shell.
fn run_env(dir: &Path) -> TestRunEnv {
    let kubeconfig = Kubeconfig::from_yaml(&format!(
        "clusters: [{{name: c, cluster: {{server: 'https://api.example.test'}}}}]\n\
         users: [{{name: u, user: {{token: placeholder}}}}]\n\
         contexts: [{{name: {CONTEXT}, context: {{cluster: c, user: u}}}}]\n"
    ))
    .expect("a kubeconfig");
    TestRunEnv {
        kubeconfig,
        path_env: "/usr/bin:/bin".into(),
        dir: dir.join("kubeconfigs"),
    }
}

/// Waits, drawing, until `done` - or panics naming `what`.
pub(super) fn wait_until(
    vcx: &mut VisualTestContext,
    what: &str,
    done: impl Fn(&mut VisualTestContext) -> bool,
) {
    for _ in 0..400 {
        vcx.run_until_parked();
        if done(vcx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("never {what}");
}

/// What `helm` runs with for `subcommand` on `web`: every flag before `--`,
/// the per-run kubeconfig as `<kubeconfig>` (see [`normalized`]).
pub(super) fn expected(subcommand: &str, positionals: &[&str]) -> Vec<String> {
    let mut args = vec![
        subcommand.to_string(),
        "--namespace=shop".to_string(),
        "<kubeconfig>".to_string(),
        format!("--kube-context={CONTEXT}"),
        "--".to_string(),
    ];
    args.extend(positionals.iter().map(|arg| arg.to_string()));
    args
}

/// `args` with the per-run kubeconfig's path, which differs each run, as
/// `<kubeconfig>`.
pub(super) fn normalized(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .map(|arg| {
            if arg.starts_with("--kubeconfig=") {
                "<kubeconfig>".to_string()
            } else {
                arg
            }
        })
        .collect()
}
