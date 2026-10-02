//! API discovery: which resource kinds a cluster offers.
//!
//! [`discover_kinds`] is the Resource panel's only source of truth - section
//! 8.1 of the `cluster-picker-and-navigation` change renders one row per kind
//! it returns, CRDs included, rather than a hard-coded list.

use kube::Client;
use kube::core::GroupVersionKind;
use kube::discovery::{Discovery, Scope};
use std::cmp::Ordering;
use std::collections::BTreeSet;

/// One resource kind a cluster's API discovery reports.
///
/// Group and version are kept rather than just the kind name: a cluster can
/// expose same-named kinds under several groups (`Event` in core and in
/// `events.k8s.io`), and a bare name would collapse them into a single row.
/// The Resource panel keys its rows on the whole `GroupVersionKind`, and the
/// panels it opens need it to address the resource.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DiscoveredKind {
    pub gvk: GroupVersionKind,
    /// Plural resource name (`pods`) - what API requests address.
    pub plural: String,
    /// Whether objects of this kind live in a namespace. Decides whether its
    /// panel offers a namespace picker (section 10.2).
    pub namespaced: bool,
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
        let mut chars = self.plural.chars();
        let plural = match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        };
        if self.gvk.group.is_empty() {
            plural
        } else {
            format!("{} · {}", plural, self.gvk.group)
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
        }
    }
}

/// Runs API discovery against `client` and returns every resource kind the
/// cluster reports - CRDs included - at each group's recommended version.
///
/// Subresources (`pods/log`, `deployments/scale`) are left out by `kube`'s
/// discovery parser before this sees them: they are not kinds of their own, so
/// they would otherwise be listed alongside the resource they hang off.
pub async fn discover_kinds(client: Client) -> kube::Result<Vec<DiscoveredKind>> {
    let discovery = Discovery::new(client).run().await?;
    let mut kinds: BTreeSet<DiscoveredKind> = BTreeSet::new();
    for group in discovery.groups() {
        for (resource, capabilities) in group.recommended_resources() {
            kinds.insert(DiscoveredKind {
                gvk: GroupVersionKind::gvk(&resource.group, &resource.version, &resource.kind),
                plural: resource.plural,
                namespaced: matches!(capabilities.scope, Scope::Namespaced),
            });
        }
    }
    Ok(kinds.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::{Client, Config};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    /// Serves fixed JSON responses by request path until the listener (and
    /// every clone of it) is dropped, i.e. for the lifetime of the test.
    async fn serve_fixtures(routes: HashMap<&'static str, &'static str>) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                tokio::spawn(handle_connection(stream, routes));
            }
        });
        addr
    }

    async fn handle_connection(
        stream: tokio::net::TcpStream,
        routes: Arc<HashMap<&'static str, &'static str>>,
    ) {
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).await.unwrap_or(0) == 0 {
            return;
        }
        // Drain headers up to the blank line.
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
        }

        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .to_string();
        let stream = reader.into_inner();
        respond(stream, routes.get(path.as_str()).copied()).await;
    }

    async fn respond(mut stream: tokio::net::TcpStream, body: Option<&str>) {
        let (status, body) = match body {
            Some(body) => ("200 OK", body),
            None => ("404 Not Found", "{}"),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    }

    fn client_for(addr: std::net::SocketAddr) -> Client {
        let mut config = Config::new(format!("http://{addr}").parse().unwrap());
        config.connect_timeout = Some(std::time::Duration::from_millis(500));
        Client::try_from(config).unwrap()
    }

    #[tokio::test]
    async fn pod_is_present_after_discovery() {
        let routes = HashMap::from([
            (
                "/apis",
                r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[]}"#,
            ),
            (
                "/api",
                r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#,
            ),
            (
                "/api/v1",
                r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
                    {"name":"pods","singularName":"pod","namespaced":true,"kind":"Pod","verbs":["get","list","watch"]}
                ]}"#,
            ),
        ]);
        let addr = serve_fixtures(routes).await;

        let kinds = discover_kinds(client_for(addr)).await.unwrap();

        let pod = kinds
            .iter()
            .find(|kind| kind.gvk.kind == "Pod")
            .expect("Pod is discovered");
        assert_eq!(pod.gvk.group, "", "core group has an empty group name");
        assert_eq!(pod.gvk.version, "v1");
        assert_eq!(pod.plural, "pods");
        assert!(pod.namespaced, "Pods are namespaced");
    }

    /// A cluster serving a CRD alongside its built-in resources: the CRD's kind
    /// comes back from discovery like any other, which is what lets the
    /// Resource panel list it instead of a fixed subset of built-ins.
    #[tokio::test]
    async fn a_crd_kind_is_discovered_alongside_the_built_in_resources() {
        let routes = HashMap::from([
            (
                "/apis",
                r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[{
                    "name":"ferns.example.com",
                    "versions":[{"groupVersion":"ferns.example.com/v1","version":"v1"}],
                    "preferredVersion":{"groupVersion":"ferns.example.com/v1","version":"v1"}
                }]}"#,
            ),
            (
                "/api",
                r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#,
            ),
            (
                "/api/v1",
                r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
                    {"name":"pods","singularName":"pod","namespaced":true,"kind":"Pod","verbs":["get","list","watch"]}
                ]}"#,
            ),
            (
                "/apis/ferns.example.com/v1",
                r#"{"kind":"APIResourceList","apiVersion":"v1","groupVersion":"ferns.example.com/v1","resources":[
                    {"name":"ferns","singularName":"fern","namespaced":true,"kind":"Fern","verbs":["get","list","watch"]}
                ]}"#,
            ),
        ]);
        let addr = serve_fixtures(routes).await;

        let kinds = discover_kinds(client_for(addr)).await.unwrap();

        let fern = kinds
            .iter()
            .find(|kind| kind.gvk.kind == "Fern")
            .expect("the CRD's kind is discovered");
        assert_eq!(fern.gvk.group, "ferns.example.com");
        assert_eq!(fern.gvk.version, "v1");
        assert_eq!(fern.plural, "ferns");
        assert!(fern.namespaced);

        // The built-in resources are still there - the CRD is additive, not a
        // replacement for the core group.
        assert!(
            kinds.iter().any(|kind| kind.gvk.kind == "Pod"),
            "kinds: {kinds:?}"
        );
    }

    /// Same-named kinds under different groups stay separate rows, and each is
    /// labelled so a reader can tell which is which - the reason a discovered
    /// kind carries its group rather than just its name.
    #[tokio::test]
    async fn same_named_kinds_in_different_groups_stay_distinct() {
        let routes = HashMap::from([
            (
                "/apis",
                r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[{
                    "name":"events.k8s.io",
                    "versions":[{"groupVersion":"events.k8s.io/v1","version":"v1"}],
                    "preferredVersion":{"groupVersion":"events.k8s.io/v1","version":"v1"}
                }]}"#,
            ),
            (
                "/api",
                r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#,
            ),
            (
                "/api/v1",
                r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
                    {"name":"events","singularName":"event","namespaced":true,"kind":"Event","verbs":["get","list","watch"]}
                ]}"#,
            ),
            (
                "/apis/events.k8s.io/v1",
                r#"{"kind":"APIResourceList","apiVersion":"v1","groupVersion":"events.k8s.io/v1","resources":[
                    {"name":"events","singularName":"event","namespaced":true,"kind":"Event","verbs":["get","list","watch"]}
                ]}"#,
            ),
        ]);
        let addr = serve_fixtures(routes).await;

        let kinds = discover_kinds(client_for(addr)).await.unwrap();

        let events: Vec<&DiscoveredKind> = kinds
            .iter()
            .filter(|kind| kind.gvk.kind == "Event")
            .collect();
        assert_eq!(events.len(), 2, "one row per group, not one per name");
        assert_eq!(events[0].label(), "Event", "core needs no qualifier");
        assert_eq!(events[1].label(), "Event · events.k8s.io");
    }

    /// A resource that only supports `get` is still a discovered kind, so it
    /// still gets a row - the Resource panel lists what the cluster reports
    /// rather than filtering down to what it can render today.
    #[tokio::test]
    async fn a_kind_without_list_verb_is_still_listed() {
        let routes = HashMap::from([
            (
                "/apis",
                r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[]}"#,
            ),
            (
                "/api",
                r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#,
            ),
            (
                "/api/v1",
                r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
                    {"name":"componentstatuses","singularName":"","namespaced":false,"kind":"ComponentStatus","verbs":["get"]}
                ]}"#,
            ),
        ]);
        let addr = serve_fixtures(routes).await;

        let kinds = discover_kinds(client_for(addr)).await.unwrap();

        let status = kinds
            .iter()
            .find(|kind| kind.gvk.kind == "ComponentStatus")
            .expect("discovered regardless of its verbs");
        assert!(!status.namespaced, "ComponentStatus is cluster-scoped");
    }

    #[test]
    fn kinds_sort_by_group_then_kind_with_core_first() {
        let kinds = vec![
            DiscoveredKind {
                gvk: GroupVersionKind::gvk("apps", "v1", "Deployment"),
                plural: "deployments".into(),
                namespaced: true,
            },
            DiscoveredKind {
                gvk: GroupVersionKind::gvk("", "v1", "Service"),
                plural: "services".into(),
                namespaced: true,
            },
            DiscoveredKind {
                gvk: GroupVersionKind::gvk("", "v1", "Pod"),
                plural: "pods".into(),
                namespaced: true,
            },
        ];
        let mut sorted = kinds.clone();
        sorted.sort();

        let labels: Vec<String> = sorted.iter().map(DiscoveredKind::label).collect();
        assert_eq!(labels, vec!["Pod", "Service", "Deployment · apps"]);
    }

    #[test]
    fn plural_label_capitalizes_the_plural_and_keeps_the_group_qualifier() {
        let pod = DiscoveredKind {
            gvk: GroupVersionKind::gvk("", "v1", "Pod"),
            plural: "pods".into(),
            namespaced: true,
        };
        assert_eq!(pod.plural_label(), "Pods");

        let widget = DiscoveredKind {
            gvk: GroupVersionKind::gvk("example.com", "v1", "Widget"),
            plural: "widgets".into(),
            namespaced: true,
        };
        assert_eq!(widget.plural_label(), "Widgets · example.com");
    }
}
