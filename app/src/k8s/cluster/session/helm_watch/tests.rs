//! helm-releases 2.2: the shared Helm release watch over a fake cluster that
//! honors selectors and metadata-only requests - install, upgrade and
//! uninstall followed live, only the latest revision fetched, a shared stream,
//! a corrupt revision, a refusal, and a namespace scope.

// Named imports only: a `use super::*` here would re-glob gpui_kit test macro internals.
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::resource::helm::{HelmReleasesTable, HelmScope, ReleaseRow, Summary};
use crate::k8s::test_cluster::FakeCluster;
use base64::Engine as _;
use gpui_kit::{Entity, TestAppContext};
use serde_json::{Value, json};
use std::io::Write as _;

const CONTEXT: &str = "kind-dev";
const PREFIX: &str = "/api/v1";

/// A release revision's payload as Helm writes it: JSON, gzipped, base64.
fn payload(name: &str, namespace: &str, version: u32, chart_version: &str) -> Vec<u8> {
    let release = json!({
        "name": name, "namespace": namespace, "version": version,
        "info": { "status": "deployed", "description": "Install complete" },
        "chart": { "metadata": { "name": name, "version": chart_version, "appVersion": "2.0" } },
        "config": { "replicaCount": 2 },
        "manifest": "---\nkind: Deployment\n",
    });
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(release.to_string().as_bytes()).unwrap();
    base64::engine::general_purpose::STANDARD
        .encode(gz.finish().unwrap())
        .into_bytes()
}

/// Revision `version` of release `name`, as Helm stores it: the Secret's data
/// is Kubernetes' base64 of Helm's own base64 payload.
fn revision(namespace: &str, name: &str, version: u32, status: &str, release: &[u8]) -> Value {
    json!({
        "apiVersion": "v1", "kind": "Secret", "type": "helm.sh/release.v1",
        "metadata": {
            "name": format!("sh.helm.release.v1.{name}.v{version}"),
            "namespace": namespace,
            "labels": { "owner": "helm", "name": name, "version": version.to_string(), "status": status },
        },
        "data": { "release": base64::engine::general_purpose::STANDARD.encode(release) },
    })
}

fn install(
    cluster: &FakeCluster,
    namespace: &str,
    name: &str,
    version: u32,
    status: &str,
    chart: &str,
) {
    let release = payload(name, namespace, version, chart);
    cluster.apply(
        PREFIX,
        "secrets",
        revision(namespace, name, version, status, &release),
    );
}

fn start(cx: &mut TestAppContext) -> FakeCluster {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    cluster
}

fn subscribe(cx: &mut TestAppContext, scope: HelmScope) -> Entity<HelmReleasesTable> {
    cx.update(|cx| {
        let client = match &ClusterRegistry::connection(cx, CONTEXT).read(cx).state {
            ConnectionState::Connected(client) => client.clone(),
            _ => panic!("the test session is connected"),
        };
        ClusterRegistry::subscribe_helm_releases(cx, CONTEXT, client, scope)
    })
}

/// Waits until `done` holds for the table's rows, or fails naming `what`.
fn wait_for(
    cx: &mut TestAppContext,
    table: &Entity<HelmReleasesTable>,
    what: &str,
    done: impl Fn(&HelmReleasesTable) -> bool,
) {
    for _ in 0..400 {
        cx.run_until_parked();
        if cx.update(|cx| done(table.read(cx))) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("never {what}");
}

fn rows(cx: &mut TestAppContext, table: &Entity<HelmReleasesTable>) -> Vec<ReleaseRow> {
    cx.update(|cx| table.read(cx).rows())
}

fn chart_version(row: &ReleaseRow) -> Option<String> {
    match &row.summary {
        Summary::Ready { chart, .. } => Some(chart.version.clone()),
        _ => None,
    }
}

#[gpui_kit::test]
async fn install_upgrade_and_uninstall_follow_the_cluster(cx: &mut TestAppContext) {
    let cluster = start(cx);
    // Neither is a release: no Helm owner label, or a different type.
    cluster.apply(
        PREFIX,
        "secrets",
        json!({ "apiVersion": "v1", "kind": "Secret",
        "type": "Opaque", "metadata": { "name": "db-password", "namespace": "shop" } }),
    );
    cluster.apply(
        PREFIX,
        "secrets",
        json!({ "apiVersion": "v1", "kind": "Secret",
        "type": "Opaque", "metadata": { "name": "lookalike", "namespace": "shop",
            "labels": { "owner": "helm", "name": "x", "version": "1" } } }),
    );
    install(&cluster, "shop", "web", 1, "deployed", "1.0.0");

    let table = subscribe(cx, HelmScope::All);
    wait_for(cx, &table, "showed web 1.0.0", |table| {
        table.loaded() && table.rows().first().and_then(chart_version).as_deref() == Some("1.0.0")
    });
    assert_eq!(rows(cx, &table).len(), 1, "only Helm release Secrets");

    // An upgrade, in the order Helm writes it: the new revision pending,
    // the old one superseded, then the new one deployed.
    install(&cluster, "shop", "web", 2, "pending-upgrade", "1.1.0");
    install(&cluster, "shop", "web", 1, "superseded", "1.0.0");
    install(&cluster, "shop", "web", 2, "deployed", "1.1.0");
    wait_for(cx, &table, "showed the upgrade", |table| {
        let rows = table.rows();
        rows.len() == 1
            && rows[0].latest.version == 2
            && chart_version(&rows[0]).as_deref() == Some("1.1.0")
    });

    let (metadata, full) = cluster.list_requests();
    assert!(metadata > 0, "the watch lists metadata only");
    assert_eq!(full, 0, "and never whole Secrets");
    assert_eq!(
        cluster.gets(),
        3,
        "v1 once, and v2 per write while it is the latest; superseding v1 fetches nothing"
    );

    cluster.delete(PREFIX, "secrets", "shop", "sh.helm.release.v1.web.v1");
    cluster.delete(PREFIX, "secrets", "shop", "sh.helm.release.v1.web.v2");
    wait_for(cx, &table, "dropped the uninstalled release", |table| {
        table.rows().is_empty()
    });
}

#[gpui_kit::test]
async fn subscribers_share_one_watch(cx: &mut TestAppContext) {
    let _cluster = start(cx);
    let first = subscribe(cx, HelmScope::All);
    let second = subscribe(cx, HelmScope::All);
    assert_eq!(first, second, "one table");
    let key = WatchKey::HelmReleases(HelmScope::All);
    assert_eq!(
        cx.update(|cx| ClusterRegistry::subscribers(cx, CONTEXT, &key)),
        2
    );

    cx.update(|cx| ClusterRegistry::unsubscribe_helm_releases(cx, CONTEXT, &HelmScope::All));
    assert!(cx.update(|cx| ClusterRegistry::helm_watch_running(cx, CONTEXT, &HelmScope::All)));
    cx.update(|cx| ClusterRegistry::unsubscribe_helm_releases(cx, CONTEXT, &HelmScope::All));
    assert!(
        !cx.update(|cx| ClusterRegistry::helm_watch_running(cx, CONTEXT, &HelmScope::All)),
        "the last one out stops it"
    );
}

#[gpui_kit::test]
async fn a_corrupt_revision_marks_only_its_release(cx: &mut TestAppContext) {
    let cluster = start(cx);
    install(&cluster, "shop", "web", 1, "deployed", "1.0.0");
    cluster.apply(
        PREFIX,
        "secrets",
        revision("shop", "bad", 1, "deployed", b"not a release"),
    );

    let table = subscribe(cx, HelmScope::All);
    wait_for(cx, &table, "read both releases", |table| {
        table
            .rows()
            .iter()
            .all(|row| !matches!(row.summary, Summary::Loading))
            && table.rows().len() == 2
    });
    for row in rows(cx, &table) {
        match row.key.name.as_str() {
            "bad" => assert!(matches!(row.summary, Summary::Unreadable(_)), "{row:?}"),
            _ => assert_eq!(chart_version(&row).as_deref(), Some("1.0.0")),
        }
    }
}

#[gpui_kit::test]
async fn a_refused_list_is_reported(cx: &mut TestAppContext) {
    let cluster = start(cx);
    cluster.refuse_lists(
        "403 Forbidden",
        json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure", "code": 403,
            "reason": "Forbidden", "message": "secrets is forbidden at the cluster scope" }),
    );
    let table = subscribe(cx, HelmScope::All);
    wait_for(cx, &table, "reported the refusal", |table| {
        table.refused().is_some()
    });
    assert_eq!(
        cx.update(|cx| table.read(cx).refused().map(str::to_string)),
        Some("secrets is forbidden at the cluster scope".to_string())
    );
}

#[gpui_kit::test]
async fn a_namespace_scope_lists_only_its_namespace(cx: &mut TestAppContext) {
    let cluster = start(cx);
    install(&cluster, "shop", "web", 1, "deployed", "1.0.0");
    install(&cluster, "ops", "monitoring", 1, "deployed", "5.0.0");
    let table = subscribe(cx, HelmScope::Namespace("shop".into()));
    wait_for(cx, &table, "listed shop", |table| {
        table.loaded() && !table.rows().is_empty()
    });
    let names: Vec<String> = rows(cx, &table)
        .into_iter()
        .map(|row| row.key.name)
        .collect();
    assert_eq!(names, ["web"]);
}
