//! API discovery: which resource kinds a cluster offers.
//!
//! [`discover_kinds`] is the Resource panel's only source of truth - section
//! 8.1 of the `cluster-picker-and-navigation` change renders one row per kind
//! it returns, CRDs included, rather than a hard-coded list.

use futures_util::future::join_all;
use kube::Client;
use kube::core::GroupVersion;
use kube::core::GroupVersionKind;
use kube::discovery::{ApiGroup, Scope, oneshot, verbs};
use std::cmp::Ordering;
use std::collections::BTreeSet;

/// One resource kind a cluster's API discovery reports.
///
/// Group and version are kept rather than just the kind name: a cluster can
/// expose same-named kinds under several groups (`Event` in core and in
/// `events.k8s.io`), and a bare name would collapse them into a single row.
/// The Resource panel keys its rows on the whole `GroupVersionKind`, and the
/// panels it opens need it to address the resource.
#[derive(Clone, Debug)]
pub struct DiscoveredKind {
    pub gvk: GroupVersionKind,
    /// Plural resource name (`pods`) - what API requests address.
    pub plural: String,
    /// Whether objects of this kind live in a namespace. Decides whether its
    /// panel offers a namespace picker (section 10.2).
    pub namespaced: bool,
    /// What the API server lets a client do with the kind, as discovery
    /// reported it. Not part of the kind's identity: a kind restored from a
    /// saved layout, which records no verbs, is the same kind discovery
    /// reports, so equality, hashing and ordering leave it out.
    pub verbs: KindVerbs,
}

/// The verbs the panels care about. A kind that can be listed but not
/// watched (`componentstatuses`) is polled instead; one that can't be listed
/// at all says so rather than showing an empty table. Only a kind that can be
/// deleted offers Delete, and only one that can be patched offers Edit
/// (`k9s-remaining-keybindings`) - a read-only kind, like an aggregated
/// metrics kind, offers neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindVerbs {
    pub list: bool,
    pub watch: bool,
    pub delete: bool,
    pub patch: bool,
}

/// All of them, as every kind but a handful supports - what a kind built
/// without discovery (a restored layout, a test) assumes.
impl Default for KindVerbs {
    fn default() -> Self {
        Self {
            list: true,
            watch: true,
            delete: true,
            patch: true,
        }
    }
}

/// Identity: group, version, kind, plural and scope - not [`KindVerbs`].
impl PartialEq for DiscoveredKind {
    fn eq(&self, other: &Self) -> bool {
        self.gvk == other.gvk && self.plural == other.plural && self.namespaced == other.namespaced
    }
}

impl Eq for DiscoveredKind {}

impl std::hash::Hash for DiscoveredKind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.gvk.hash(state);
        self.plural.hash(state);
        self.namespaced.hash(state);
    }
}

/// Ordered by group, then kind, then version - the core group first (its group
/// name is the empty string), alphabetical within a group. Spelled out here
/// because `GroupVersionKind` is only `Eq`/`Hash`, not `Ord`.
impl Ord for DiscoveredKind {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.gvk.group.as_str(),
            self.gvk.kind.as_str(),
            self.gvk.version.as_str(),
        )
            .cmp(&(
                other.gvk.group.as_str(),
                other.gvk.kind.as_str(),
                other.gvk.version.as_str(),
            ))
            .then_with(|| self.plural.cmp(&other.plural))
            .then_with(|| self.namespaced.cmp(&other.namespaced))
    }
}

impl PartialOrd for DiscoveredKind {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl DiscoveredKind {
    /// What a Resource panel row reads. Core-group kinds are the common case
    /// and need no qualifier; anything else carries its group, so two groups'
    /// same-named kinds stay tellable apart at a glance.
    pub fn label(&self) -> String {
        if self.gvk.group.is_empty() {
            self.gvk.kind.clone()
        } else {
            format!("{} · {}", self.gvk.kind, self.gvk.group)
        }
    }

    /// What a *list* panel of this kind titles itself - the plural form
    /// (`"Pods"`), as opposed to [`label`]'s singular Kind name (`"Pod"`),
    /// which is right for a single item but wrong for a panel showing many.
    pub fn plural_label(&self) -> String {
        let plural = self.plural_name();
        if self.gvk.group.is_empty() {
            plural
        } else {
            format!("{} · {}", plural, self.gvk.group)
        }
    }

    /// The capitalized plural alone (`"Certificates"`), with no group
    /// qualifier - what a custom resource's list tab reads, its group going
    /// to the tab's tooltip instead.
    pub fn plural_name(&self) -> String {
        let mut chars = self.plural.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }

    /// The core `v1` `Pod` kind, for the callers that mean "Pods" without
    /// having run discovery: the `nav.show_pods` command, and the panel a
    /// freshly connected window lands on.
    /// Whether this is the built-in core `Pod` kind - the one kind that keeps its
    /// own typed list (the Pods panel) rather than the generic one. A CRD named
    /// `Pod` in its own group is not.
    pub fn is_core_pod(&self) -> bool {
        self.gvk.group.is_empty() && self.gvk.kind == "Pod"
    }

    pub fn pods() -> Self {
        Self {
            gvk: GroupVersionKind::gvk("", "v1", "Pod"),
            plural: "pods".to_string(),
            namespaced: true,
            verbs: Default::default(),
        }
    }

    /// The core `v1` `Event` kind - what the events browser lists and the
    /// "Show Events" command opens.
    pub fn events() -> Self {
        Self {
            gvk: GroupVersionKind::gvk("", "v1", "Event"),
            plural: "events".to_string(),
            namespaced: true,
            verbs: KindVerbs::default(),
        }
    }

    /// Whether this is the core `v1` `Event` kind, which opens the events
    /// browser rather than the generic list (`events-browser` D4). A CRD named
    /// `Event` in its own group is not, and nor is `events.k8s.io`'s, which
    /// keeps the generic list.
    pub fn is_core_event(&self) -> bool {
        self.gvk.group.is_empty() && self.gvk.kind == "Event"
    }
}

/// What discovery found: every kind from the API groups that answered, and the
/// groups that didn't. A group that fails - typically an aggregated API such as
/// `metrics.k8s.io` whose backend is down and answers 503 - costs only its own
/// kinds, not the whole list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovered {
    pub kinds: Vec<DiscoveredKind>,
    pub unavailable: Vec<UnavailableGroup>,
}

/// Why discovery failed outright - only listing the API groups can: a readable
/// message (the HTTP status and message), and the full technical detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryFailure {
    pub message: String,
    pub detail: String,
}

impl std::fmt::Display for DiscoveryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// One API group discovery couldn't read, and why, readably.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnavailableGroup {
    /// The group's name; empty for the core group.
    pub group: String,
    /// What went wrong: the HTTP status and message, not a parse-failure dump.
    pub reason: String,
}

/// Runs API discovery against `client`: every resource kind the cluster reports -
/// CRDs included - at each group's preferred version, group by group.
///
/// Only listing the groups at all is fatal, and it too gets at most the per-group
/// limit: kube's own retrying of a 503 would otherwise hold the panel for minutes. Each group is then read on its own,
/// concurrently, and one that fails lands in [`Discovered::unavailable`] while the
/// rest list.
///
/// Subresources (`pods/log`, `deployments/scale`) are left out by `kube`'s
/// discovery parser before this sees them: they are not kinds of their own, so
/// they would otherwise be listed alongside the resource they hang off.
pub async fn discover_kinds(client: Client) -> Result<Discovered, DiscoveryFailure> {
    discover_kinds_within(client, crate::consts::DISCOVERY_GROUP_TIMEOUT).await
}

/// [`discover_kinds`], waiting at most `per_group` for each group.
pub(crate) async fn discover_kinds_within(
    client: Client,
    per_group: std::time::Duration,
) -> Result<Discovered, DiscoveryFailure> {
    let groups = match tokio::time::timeout(per_group, client.list_api_groups()).await {
        Ok(Ok(groups)) => groups,
        Ok(Err(error)) => {
            return Err(DiscoveryFailure {
                message: crate::k8s::error::describe(&error),
                detail: crate::k8s::error::detail(&error),
            });
        }
        Err(_) => {
            let message = format!(
                "The API server didn't list its API groups within {}s.",
                per_group.as_secs_f32()
            );
            return Err(DiscoveryFailure {
                detail: message.clone(),
                message,
            });
        }
    };
    let core = within(
        String::new(),
        per_group,
        oneshot::group(&client, ApiGroup::CORE_GROUP),
    );
    let named = groups.groups.into_iter().map(|group| {
        let client = client.clone();
        let version = group
            .preferred_version
            .as_ref()
            .or(group.versions.first())
            .map(|version| version.version.clone())
            .unwrap_or_default();
        let gv = GroupVersion::gv(&group.name, &version);
        within(group.name, per_group, async move {
            oneshot::pinned_group(&client, &gv).await
        })
    });
    let (core, named) = futures_util::future::join(core, join_all(named)).await;

    let mut kinds: BTreeSet<DiscoveredKind> = BTreeSet::new();
    let mut unavailable = Vec::new();
    for (name, result) in std::iter::once(core).chain(named) {
        match result {
            Ok(group) => {
                for (resource, capabilities) in group.recommended_resources() {
                    kinds.insert(DiscoveredKind {
                        gvk: GroupVersionKind::gvk(
                            &resource.group,
                            &resource.version,
                            &resource.kind,
                        ),
                        plural: resource.plural,
                        namespaced: matches!(capabilities.scope, Scope::Namespaced),
                        verbs: KindVerbs {
                            list: capabilities.supports_operation(verbs::LIST),
                            watch: capabilities.supports_operation(verbs::WATCH),
                            delete: capabilities.supports_operation(verbs::DELETE),
                            patch: capabilities.supports_operation(verbs::PATCH),
                        },
                    });
                }
            }
            Err(reason) => unavailable.push(UnavailableGroup {
                group: name,
                reason,
            }),
        }
    }
    Ok(Discovered {
        kinds: kinds.into_iter().collect(),
        unavailable,
    })
}

/// One group's query, given at most `limit`: its group, or why not, readably.
async fn within(
    name: String,
    limit: std::time::Duration,
    query: impl std::future::Future<Output = kube::Result<ApiGroup>>,
) -> (String, Result<ApiGroup, String>) {
    let result = match tokio::time::timeout(limit, query).await {
        Ok(result) => result.map_err(|error| crate::k8s::error::describe(&error)),
        Err(_) => Err(format!("no answer within {}s", limit.as_secs_f32())),
    };
    (name, result)
}

#[cfg(test)]
mod tests;
