//! The release table over watch events alone: grouping revisions into
//! releases, the latest revision, history, deletes, relists, and which latest
//! revisions to fetch.

use super::{Fetch, HelmReleasesTable, ReleaseKey, Summary};
use crate::k8s::resource::helm::decode::{ChartMeta, ReleaseInfo};
use k8s_openapi::api::core::v1::Secret;
use kube::core::{ObjectMeta, PartialObjectMeta};
use kube_runtime::watcher::Event;
use std::collections::BTreeMap;

fn secret(
    namespace: &str,
    release: &str,
    version: u32,
    status: &str,
    rv: &str,
) -> PartialObjectMeta<Secret> {
    let labels = BTreeMap::from([
        ("owner".to_string(), "helm".to_string()),
        ("name".to_string(), release.to_string()),
        ("version".to_string(), version.to_string()),
        ("status".to_string(), status.to_string()),
    ]);
    PartialObjectMeta {
        types: None,
        metadata: ObjectMeta {
            name: Some(format!("sh.helm.release.v1.{release}.v{version}")),
            namespace: Some(namespace.to_string()),
            labels: Some(labels),
            resource_version: Some(rv.to_string()),
            ..ObjectMeta::default()
        },
        _phantom: Default::default(),
    }
}

fn key(namespace: &str, name: &str) -> ReleaseKey {
    ReleaseKey {
        namespace: namespace.into(),
        name: name.into(),
    }
}

fn listed(secrets: Vec<PartialObjectMeta<Secret>>) -> HelmReleasesTable {
    let mut table = HelmReleasesTable::default();
    table.apply(Event::Init);
    for secret in secrets {
        table.apply(Event::InitApply(secret));
    }
    table.apply(Event::InitDone);
    table
}

#[test]
fn revisions_group_into_one_row_per_release() {
    let table = listed(vec![
        secret("shop", "web", 1, "superseded", "1"),
        secret("shop", "web", 2, "superseded", "2"),
        secret("shop", "web", 3, "deployed", "3"),
        secret("shop", "db", 1, "deployed", "4"),
        secret("ops", "web", 1, "failed", "5"),
    ]);
    assert!(table.loaded());
    let rows: Vec<(String, String, u32, String)> = table
        .rows()
        .into_iter()
        .map(|row| {
            (
                row.key.namespace,
                row.key.name,
                row.latest.version,
                row.latest.status,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("ops".into(), "web".into(), 1, "failed".into()),
            ("shop".into(), "db".into(), 1, "deployed".into()),
            ("shop".into(), "web".into(), 3, "deployed".into()),
        ],
        "a release is its namespace and name; the latest revision leads"
    );
    let history: Vec<u32> = table
        .history(&key("shop", "web"))
        .into_iter()
        .map(|revision| revision.version)
        .collect();
    assert_eq!(history, [3, 2, 1], "newest first");
}

#[test]
fn an_upgrade_and_an_uninstall_follow_the_watch() {
    let mut table = listed(vec![secret("shop", "web", 1, "deployed", "1")]);
    table.apply(Event::Apply(secret("shop", "web", 1, "superseded", "2")));
    table.apply(Event::Apply(secret("shop", "web", 2, "deployed", "3")));
    let rows = table.rows();
    assert_eq!(rows.len(), 1, "an upgrade adds no row");
    assert_eq!(
        (rows[0].latest.version, rows[0].latest.status.as_str()),
        (2, "deployed")
    );

    table.apply(Event::Delete(secret("shop", "web", 1, "superseded", "4")));
    table.apply(Event::Delete(secret("shop", "web", 2, "deployed", "5")));
    assert!(table.rows().is_empty(), "uninstalled without history: gone");
}

#[test]
fn a_relist_drops_what_was_deleted_meanwhile() {
    let mut table = listed(vec![
        secret("shop", "web", 1, "deployed", "1"),
        secret("shop", "db", 1, "deployed", "2"),
    ]);
    table.apply(Event::Init);
    table.apply(Event::InitApply(secret("shop", "web", 1, "deployed", "1")));
    table.apply(Event::InitDone);
    let names: Vec<String> = table.rows().into_iter().map(|row| row.key.name).collect();
    assert_eq!(names, ["web"]);
}

#[test]
fn a_secret_without_helm_labels_is_ignored() {
    let mut stray = secret("shop", "web", 1, "deployed", "1");
    stray.metadata.labels = None;
    assert!(listed(vec![stray]).rows().is_empty());
}

#[test]
fn each_new_latest_revision_is_fetched_once() {
    let mut table = listed(vec![
        secret("shop", "web", 1, "superseded", "1"),
        secret("shop", "web", 2, "deployed", "2"),
    ]);
    let fetches = table.take_fetches();
    assert_eq!(
        fetches,
        [Fetch {
            key: key("shop", "web"),
            version: 2,
            resource_version: "2".into(),
            secret: "sh.helm.release.v1.web.v2".into(),
        }],
        "only the latest revision"
    );
    assert!(table.take_fetches().is_empty(), "once");
    assert_eq!(table.rows()[0].summary, Summary::Loading);

    let ready = Summary::Ready {
        chart: ChartMeta {
            name: "web".into(),
            version: "1.4.2".into(),
            app_version: Some("2.0.1".into()),
        },
        info: ReleaseInfo::default(),
    };
    table.finish(&fetches[0], ready.clone());
    assert_eq!(table.rows()[0].summary, ready);

    table.apply(Event::Apply(secret("shop", "web", 3, "deployed", "3")));
    let next = table.take_fetches();
    assert_eq!(next.len(), 1, "an upgrade fetches its new revision");
    assert_eq!(table.rows()[0].summary, Summary::Loading);
    table.finish(&fetches[0], ready);
    assert_eq!(
        table.rows()[0].summary,
        Summary::Loading,
        "a stale result doesn't land on the newer revision"
    );
}

/// Nothing is fetched while the first list arrives, when each revision is
/// briefly its release's latest; once it completes, only the latest is.
#[test]
fn nothing_is_fetched_before_the_first_list_completes() {
    let mut table = HelmReleasesTable::default();
    table.apply(Event::Init);
    table.apply(Event::InitApply(secret(
        "shop",
        "web",
        1,
        "superseded",
        "1",
    )));
    assert!(table.take_fetches().is_empty());
    table.apply(Event::InitApply(secret("shop", "web", 2, "deployed", "2")));
    assert!(table.take_fetches().is_empty());
    table.apply(Event::InitDone);
    let fetches = table.take_fetches();
    assert_eq!(fetches.len(), 1);
    assert_eq!(fetches[0].version, 2);
}
