//! A context's Helm releases as its release watch sees them (helm-releases
//! D1): every revision Secret's labels, grouped into releases, with the latest
//! revision of each decoded on demand. Rows and history come from the labels
//! alone - `name`, `version`, `status` - so the table never needs a payload to
//! list a release; chart and app version wait for the latest revision's fetch.

use super::decode::{ChartMeta, ReleaseInfo};
use crate::k8s::resource::load_phase::LoadPhase;
use k8s_openapi::api::core::v1::Secret;
use kube::core::{ObjectMeta, PartialObjectMeta};
use kube_runtime::watcher;
use std::collections::{BTreeMap, HashMap, HashSet};

/// One release: Helm's release name in its namespace.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ReleaseKey {
    pub(crate) namespace: String,
    pub(crate) name: String,
}

/// One revision, as its Secret's labels describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Revision {
    pub(crate) version: u32,
    /// `deployed`, `superseded`, `failed`, ... as Helm last labelled it.
    pub(crate) status: String,
    /// The revision Secret's name, `sh.helm.release.v1.<release>.v<version>`.
    pub(crate) secret: String,
    pub(crate) resource_version: String,
}

/// What a release's latest revision says beyond its labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Summary {
    /// Its payload is being fetched.
    Loading,
    Ready {
        chart: ChartMeta,
        info: ReleaseInfo,
    },
    /// Its payload couldn't be fetched or decoded, and why.
    Unreadable(String),
}

/// One release as a list row: its latest revision and what that says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReleaseRow {
    pub(crate) key: ReleaseKey,
    pub(crate) latest: Revision,
    pub(crate) summary: Summary,
}

/// A latest revision to fetch and decode, and where its result goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fetch {
    pub(crate) key: ReleaseKey,
    pub(crate) version: u32,
    pub(crate) resource_version: String,
    pub(crate) secret: String,
}

/// The decoded summary of one release's latest revision, and which revision
/// it is - a newer one replaces it.
struct Decoded {
    version: u32,
    resource_version: String,
    summary: Summary,
}

#[derive(Default)]
pub(crate) struct HelmReleasesTable {
    releases: BTreeMap<ReleaseKey, BTreeMap<u32, Revision>>,
    decoded: HashMap<ReleaseKey, Decoded>,
    /// The revision Secrets a relist has seen so far, by (namespace, name).
    relisting: Option<HashSet<(String, String)>>,
    phase: LoadPhase,
    refused: Option<String>,
}

// UNWIRED(#370): the list and detail panels (helm-releases sections 3-4) read
// rows and history; until then only the watch and its tests use the table.
#[allow(dead_code)]
impl HelmReleasesTable {
    /// Applies one watch event.
    pub(crate) fn apply(&mut self, event: watcher::Event<PartialObjectMeta<Secret>>) {
        match event {
            watcher::Event::Apply(secret) => self.insert(&secret.metadata),
            watcher::Event::InitApply(secret) => {
                if let Some(seen) = &mut self.relisting {
                    seen.insert(secret_id(&secret.metadata));
                }
                self.phase.received();
                self.insert(&secret.metadata);
            }
            watcher::Event::Delete(secret) => self.remove(&secret.metadata),
            watcher::Event::Init => {
                self.refused = None;
                self.relisting = Some(HashSet::new());
                self.phase.init();
            }
            watcher::Event::InitDone => {
                if let Some(seen) = self.relisting.take() {
                    self.keep_only(&seen);
                }
                self.phase.done();
            }
        }
    }

    /// The latest revisions not yet fetched, marked as loading. Each is
    /// returned once per new latest revision - and none until the first list
    /// has completed: while it arrives, each revision is briefly the latest of
    /// its release, and fetching each would read every revision once.
    pub(crate) fn take_fetches(&mut self) -> Vec<Fetch> {
        let mut fetches = Vec::new();
        if !self.phase.has_loaded() {
            return fetches;
        }
        for (key, revisions) in &self.releases {
            let Some(latest) = revisions.values().next_back() else {
                continue;
            };
            let current = self.decoded.get(key).is_some_and(|decoded| {
                decoded.version == latest.version
                    && decoded.resource_version == latest.resource_version
            });
            if current {
                continue;
            }
            self.decoded.insert(
                key.clone(),
                Decoded {
                    version: latest.version,
                    resource_version: latest.resource_version.clone(),
                    summary: Summary::Loading,
                },
            );
            fetches.push(Fetch {
                key: key.clone(),
                version: latest.version,
                resource_version: latest.resource_version.clone(),
                secret: latest.secret.clone(),
            });
        }
        fetches
    }

    /// Records `fetch`'s result, unless a newer revision has replaced it since.
    pub(crate) fn finish(&mut self, fetch: &Fetch, summary: Summary) {
        if let Some(decoded) = self.decoded.get_mut(&fetch.key)
            && decoded.version == fetch.version
            && decoded.resource_version == fetch.resource_version
        {
            decoded.summary = summary;
        }
    }

    /// One row per release, by namespace then name.
    pub(crate) fn rows(&self) -> Vec<ReleaseRow> {
        self.releases
            .iter()
            .filter_map(|(key, revisions)| {
                let latest = revisions.values().next_back()?.clone();
                let summary = self
                    .decoded
                    .get(key)
                    .filter(|decoded| decoded.version == latest.version)
                    .map_or(Summary::Loading, |decoded| decoded.summary.clone());
                Some(ReleaseRow {
                    key: key.clone(),
                    latest,
                    summary,
                })
            })
            .collect()
    }

    /// `key`'s revisions, newest first; empty for a release that's gone.
    pub(crate) fn history(&self, key: &ReleaseKey) -> Vec<Revision> {
        self.releases
            .get(key)
            .map(|revisions| revisions.values().rev().cloned().collect())
            .unwrap_or_default()
    }

    /// Whether the first list has completed.
    pub(crate) fn loaded(&self) -> bool {
        self.phase.has_loaded()
    }

    /// Where the table is in its lists (`list-loading-indicator`).
    pub(crate) fn phase(&self) -> LoadPhase {
        self.phase
    }

    /// The API server's message when it refused the watch.
    pub(crate) fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    pub(crate) fn set_refused(&mut self, message: String) {
        self.refused = Some(message);
    }

    fn insert(&mut self, meta: &ObjectMeta) {
        let Some((key, revision)) = revision_of(meta) else {
            return;
        };
        self.releases
            .entry(key)
            .or_default()
            .insert(revision.version, revision);
    }

    fn remove(&mut self, meta: &ObjectMeta) {
        let Some((key, revision)) = revision_of(meta) else {
            return;
        };
        if let Some(revisions) = self.releases.get_mut(&key) {
            revisions.remove(&revision.version);
            if revisions.is_empty() {
                self.releases.remove(&key);
                self.decoded.remove(&key);
            }
        }
    }

    /// Drops every revision a relist didn't see: deleted while unwatched.
    fn keep_only(&mut self, seen: &HashSet<(String, String)>) {
        for (key, revisions) in &mut self.releases {
            revisions.retain(|_, revision| {
                seen.contains(&(key.namespace.clone(), revision.secret.clone()))
            });
        }
        let gone: Vec<ReleaseKey> = self
            .releases
            .iter()
            .filter(|(_, revisions)| revisions.is_empty())
            .map(|(key, _)| key.clone())
            .collect();
        for key in gone {
            self.releases.remove(&key);
            self.decoded.remove(&key);
        }
    }
}

fn secret_id(meta: &ObjectMeta) -> (String, String) {
    (
        meta.namespace.clone().unwrap_or_default(),
        meta.name.clone().unwrap_or_default(),
    )
}

/// The release and revision a Helm storage Secret's labels name; `None` for
/// one without Helm's `name` and `version` labels.
fn revision_of(meta: &ObjectMeta) -> Option<(ReleaseKey, Revision)> {
    let labels = meta.labels.as_ref()?;
    let version = labels.get("version")?.parse().ok()?;
    let key = ReleaseKey {
        namespace: meta.namespace.clone().unwrap_or_default(),
        name: labels.get("name")?.clone(),
    };
    let revision = Revision {
        version,
        status: labels.get("status").cloned().unwrap_or_default(),
        secret: meta.name.clone().unwrap_or_default(),
        resource_version: meta.resource_version.clone().unwrap_or_default(),
    };
    Some((key, revision))
}

#[cfg(test)]
mod tests;
