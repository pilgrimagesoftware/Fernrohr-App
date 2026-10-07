//! Network kinds' sections - Service, Ingress, Endpoints, EndpointSlice and
//! NetworkPolicy: how traffic reaches them and where it goes. An Ingress's
//! backend Services and TLS Secrets, and the Pods behind Endpoints and
//! EndpointSlices, are references.

use super::super::model::{FieldValue, ObjectField, ObjectSection};
use super::common::{non_empty, selector_chips};
use crate::k8s::object_ref::ObjectRef;
use k8s_openapi::api::core::v1::{EndpointPort, Endpoints, ObjectReference, Service, ServicePort};
use k8s_openapi::api::discovery::v1::EndpointSlice;
use k8s_openapi::api::networking::v1::{
    Ingress, IngressServiceBackend, NetworkPolicy, NetworkPolicyPeer, NetworkPolicyPort,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

/// `8080` or `http`, as a port names its target.
fn int_or_string(value: &IntOrString) -> String {
    match value {
        IntOrString::Int(number) => number.to_string(),
        IntOrString::String(name) => name.clone(),
    }
}

/// `name 80/TCP`, leaving out what the API left out.
fn port_label(name: Option<&str>, port: impl std::fmt::Display, protocol: Option<&str>) -> String {
    let protocol = non_empty(protocol).unwrap_or("TCP");
    match non_empty(name) {
        Some(name) => format!("{name} {port}/{protocol}"),
        None => format!("{port}/{protocol}"),
    }
}

/// `http 80/TCP → 8080, node 30080`.
fn service_port(port: &ServicePort) -> String {
    let mut line = port_label(port.name.as_deref(), port.port, port.protocol.as_deref());
    if let Some(target) = &port.target_port {
        line.push_str(&format!(" → {}", int_or_string(target)));
    }
    if let Some(node_port) = port.node_port {
        line.push_str(&format!(", node {node_port}"));
    }
    line
}

/// A target reference - a Pod, almost always - in `namespace` unless it names
/// its own. `None` for one with no kind or name to link to.
fn target(reference: &ObjectReference, namespace: &str) -> Option<ObjectRef> {
    let kind = non_empty(reference.kind.as_deref())?;
    let name = non_empty(reference.name.as_deref())?;
    let namespace = non_empty(reference.namespace.as_deref()).unwrap_or(namespace);
    Some(ObjectRef::core(kind, namespace, name))
}

/// `values` with duplicates dropped, first occurrence kept - several paths or
/// addresses can name the same object or port.
fn unique<T: PartialEq>(values: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut seen = Vec::new();
    for value in values {
        if !seen.contains(&value) {
            seen.push(value);
        }
    }
    seen
}

fn lines(fields: &mut Vec<ObjectField>, label: &str, lines: Vec<String>) {
    if !lines.is_empty() {
        fields.push(ObjectField::new(label, FieldValue::Lines(lines)));
    }
}

fn references(
    fields: &mut Vec<ObjectField>,
    label: &str,
    targets: Vec<ObjectRef>,
    qualified: bool,
) {
    if !targets.is_empty() {
        fields.push(ObjectField::references(label, targets, qualified));
    }
}

pub(super) fn service(service: &Service) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    let spec = service.spec.as_ref();

    if let Some(type_) = non_empty(spec.and_then(|spec| spec.type_.as_deref())) {
        fields.push(ObjectField::text("Type", type_));
    }
    let cluster_ips: Vec<&str> = match spec.and_then(|spec| spec.cluster_ips.as_ref()) {
        Some(ips) if !ips.is_empty() => ips.iter().map(String::as_str).collect(),
        _ => spec
            .and_then(|spec| non_empty(spec.cluster_ip.as_deref()))
            .into_iter()
            .collect(),
    };
    if !cluster_ips.is_empty() {
        fields.push(ObjectField::text("Cluster IPs", cluster_ips.join(", ")));
    }
    let external_ips = spec
        .and_then(|spec| spec.external_ips.as_ref())
        .filter(|ips| !ips.is_empty());
    if let Some(ips) = external_ips {
        fields.push(ObjectField::text("External IPs", ips.join(", ")));
    }
    let ingress: Vec<String> = service
        .status
        .as_ref()
        .and_then(|status| status.load_balancer.as_ref()?.ingress.as_ref())
        .into_iter()
        .flatten()
        .filter_map(|ingress| {
            non_empty(ingress.ip.as_deref())
                .or(non_empty(ingress.hostname.as_deref()))
                .map(str::to_string)
        })
        .collect();
    lines(&mut fields, "Load Balancer", ingress);
    let ports: Vec<String> = spec
        .and_then(|spec| spec.ports.as_ref())
        .into_iter()
        .flatten()
        .map(service_port)
        .collect();
    lines(&mut fields, "Ports", ports);
    let selector: Vec<String> = spec
        .and_then(|spec| spec.selector.as_ref())
        .into_iter()
        .flatten()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    if !selector.is_empty() {
        fields.push(ObjectField::new("Selector", FieldValue::Chips(selector)));
    }
    if let Some(affinity) = non_empty(spec.and_then(|spec| spec.session_affinity.as_deref())) {
        fields.push(ObjectField::text("Session Affinity", affinity));
    }

    vec![ObjectSection::new("Service", fields)]
}

/// `web:80` or `web:http`, as an Ingress names a backend.
fn backend_label(backend: &IngressServiceBackend) -> String {
    let port = backend.port.as_ref().and_then(|port| {
        port.number
            .map(|number| number.to_string())
            .or_else(|| non_empty(port.name.as_deref()).map(str::to_string))
    });
    match port {
        Some(port) => format!("{}:{port}", backend.name),
        None => backend.name.clone(),
    }
}

pub(super) fn ingress(ingress: &Ingress, namespace: &str) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    let spec = ingress.spec.as_ref();
    let service =
        |backend: &IngressServiceBackend| ObjectRef::core("Service", namespace, &backend.name);

    if let Some(class) = non_empty(spec.and_then(|spec| spec.ingress_class_name.as_deref())) {
        fields.push(ObjectField::text("Class", class));
    }
    let default = spec
        .and_then(|spec| spec.default_backend.as_ref())
        .and_then(|backend| backend.service.as_ref());
    if let Some(backend) = default {
        fields.push(ObjectField::text("Default Backend", backend_label(backend)));
    }
    let rules = spec
        .and_then(|spec| spec.rules.as_ref())
        .into_iter()
        .flatten();
    let mut rule_lines = Vec::new();
    let mut backends: Vec<ObjectRef> = default.map(service).into_iter().collect();
    for rule in rules {
        let host = non_empty(rule.host.as_deref()).unwrap_or("*");
        for path in rule.http.iter().flat_map(|http| &http.paths) {
            let route = non_empty(path.path.as_deref()).unwrap_or("/");
            let line = match path.backend.service.as_ref() {
                Some(backend) => {
                    backends.push(service(backend));
                    format!("{host}{route} → {}", backend_label(backend))
                }
                None => format!("{host}{route}"),
            };
            rule_lines.push(line);
        }
    }
    lines(&mut fields, "Rules", rule_lines);
    let urls = host_urls(ingress);
    if !urls.is_empty() {
        fields.push(ObjectField::new("Open", FieldValue::Urls(urls)));
    }
    references(&mut fields, "Backends", unique(backends), false);
    let tls = spec
        .and_then(|spec| spec.tls.as_ref())
        .into_iter()
        .flatten();
    let mut tls_hosts = Vec::new();
    let mut tls_secrets = Vec::new();
    for entry in tls {
        tls_hosts.extend(entry.hosts.iter().flatten().cloned());
        if let Some(secret) = non_empty(entry.secret_name.as_deref()) {
            tls_secrets.push(ObjectRef::core("Secret", namespace, secret));
        }
    }
    lines(&mut fields, "TLS Hosts", tls_hosts);
    references(&mut fields, "TLS Secrets", unique(tls_secrets), false);

    vec![ObjectSection::new("Ingress", fields)]
}

/// The web address of each of an Ingress's rules with a host (#157): `https`
/// when a TLS entry covers the host, else `http`, and the rule's path - each
/// once. A wildcard host (`*.example.com`) is no one address, so it has none.
fn host_urls(ingress: &Ingress) -> Vec<String> {
    let Some(spec) = ingress.spec.as_ref() else {
        return Vec::new();
    };
    let tls_hosts: Vec<&str> = spec
        .tls
        .iter()
        .flatten()
        .flat_map(|tls| tls.hosts.iter().flatten())
        .map(String::as_str)
        .collect();
    let urls = spec.rules.iter().flatten().flat_map(|rule| {
        let host = non_empty(rule.host.as_deref()).filter(|host| !host.contains('*'));
        let scheme = if host.is_some_and(|host| tls_hosts.contains(&host)) {
            "https"
        } else {
            "http"
        };
        let paths: Vec<&str> = rule
            .http
            .iter()
            .flat_map(|http| &http.paths)
            .map(|path| non_empty(path.path.as_deref()).unwrap_or("/"))
            .collect();
        let paths = if paths.is_empty() { vec!["/"] } else { paths };
        host.into_iter()
            .flat_map(move |host| {
                paths
                    .clone()
                    .into_iter()
                    .map(move |path| format!("{scheme}://{host}{path}"))
            })
            .collect::<Vec<_>>()
    });
    unique(urls)
}

fn endpoint_ports(ports: Option<&Vec<EndpointPort>>) -> Vec<String> {
    ports
        .into_iter()
        .flatten()
        .map(|port| port_label(port.name.as_deref(), port.port, port.protocol.as_deref()))
        .collect()
}

pub(super) fn endpoints(endpoints: &Endpoints, namespace: &str) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    let mut addresses = Vec::new();
    let mut ports = Vec::new();
    let mut targets = Vec::new();
    for subset in endpoints.subsets.iter().flatten() {
        let ready = subset
            .addresses
            .iter()
            .flatten()
            .map(|address| (address, "ready"));
        let not_ready = subset
            .not_ready_addresses
            .iter()
            .flatten()
            .map(|address| (address, "not ready"));
        for (address, state) in ready.chain(not_ready) {
            addresses.push(format!("{} ({state})", address.ip));
            targets.extend(
                address
                    .target_ref
                    .as_ref()
                    .and_then(|reference| target(reference, namespace)),
            );
        }
        ports.extend(endpoint_ports(subset.ports.as_ref()));
    }
    lines(&mut fields, "Addresses", addresses);
    lines(&mut fields, "Ports", unique(ports));
    references(&mut fields, "Targets", unique(targets), true);

    vec![ObjectSection::new("Endpoints", fields)]
}

pub(super) fn endpoint_slice(slice: &EndpointSlice, namespace: &str) -> Vec<ObjectSection> {
    let mut fields = vec![ObjectField::text(
        "Address Type",
        slice.address_type.clone(),
    )];
    let mut addresses = Vec::new();
    let mut targets = Vec::new();
    for endpoint in slice.endpoints.iter().flatten() {
        let state = match endpoint
            .conditions
            .as_ref()
            .and_then(|conditions| conditions.ready)
        {
            Some(true) => "ready",
            Some(false) => "not ready",
            None => "ready unknown",
        };
        for address in &endpoint.addresses {
            addresses.push(format!("{address} ({state})"));
        }
        targets.extend(
            endpoint
                .target_ref
                .as_ref()
                .and_then(|reference| target(reference, namespace)),
        );
    }
    lines(&mut fields, "Endpoints", addresses);
    let ports: Vec<String> = slice
        .ports
        .iter()
        .flatten()
        .map(|port| {
            let number = port
                .port
                .map_or_else(|| "*".to_string(), |port| port.to_string());
            port_label(port.name.as_deref(), number, port.protocol.as_deref())
        })
        .collect();
    lines(&mut fields, "Ports", ports);
    references(&mut fields, "Targets", unique(targets), true);

    vec![ObjectSection::new("Endpoint Slice", fields)]
}

/// One peer of a policy rule: pods by selector (optionally in namespaces by
/// selector), or an IP block with its exceptions.
fn peer(peer: &NetworkPolicyPeer) -> String {
    if let Some(block) = &peer.ip_block {
        let except = block.except.as_deref().unwrap_or_default();
        return if except.is_empty() {
            format!("ipBlock {}", block.cidr)
        } else {
            format!("ipBlock {} except {}", block.cidr, except.join(", "))
        };
    }
    let selected = |label: &str, selector: &LabelSelector| {
        let chips = selector_chips(selector);
        if chips.is_empty() {
            format!("all {label}")
        } else {
            format!("{label} {}", chips.join(", "))
        }
    };
    match (&peer.namespace_selector, &peer.pod_selector) {
        (Some(namespaces), Some(pods)) => {
            format!(
                "{} in {}",
                selected("pods", pods),
                selected("namespaces", namespaces)
            )
        }
        (Some(namespaces), None) => selected("namespaces", namespaces),
        (None, Some(pods)) => selected("pods", pods),
        (None, None) => "anything".to_string(),
    }
}

fn policy_port(port: &NetworkPolicyPort) -> String {
    let protocol = non_empty(port.protocol.as_deref()).unwrap_or("TCP");
    match (&port.port, port.end_port) {
        (Some(start), Some(end)) => format!("{}-{end}/{protocol}", int_or_string(start)),
        (Some(port), None) => format!("{}/{protocol}", int_or_string(port)),
        (None, _) => format!("all/{protocol}"),
    }
}

/// `from pods app=web; ports 80/TCP`: a rule's peers and ports, or "any" for
/// what it leaves open.
fn policy_rule(
    direction: &str,
    peers: Option<&Vec<NetworkPolicyPeer>>,
    ports: Option<&Vec<NetworkPolicyPort>>,
) -> String {
    let peers: Vec<String> = peers.into_iter().flatten().map(peer).collect();
    let ports: Vec<String> = ports.into_iter().flatten().map(policy_port).collect();
    let peers = if peers.is_empty() {
        "anywhere".to_string()
    } else {
        peers.join(" | ")
    };
    let ports = if ports.is_empty() {
        "all".to_string()
    } else {
        ports.join(", ")
    };
    format!("{direction} {peers}; ports {ports}")
}

pub(super) fn network_policy(policy: &NetworkPolicy) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    let spec = policy.spec.as_ref();

    let pods = spec
        .and_then(|spec| spec.pod_selector.as_ref())
        .map(selector_chips)
        .unwrap_or_default();
    fields.push(if pods.is_empty() {
        ObjectField::text("Pod Selector", "all pods in the namespace")
    } else {
        ObjectField::new("Pod Selector", FieldValue::Chips(pods))
    });
    let types = spec
        .and_then(|spec| spec.policy_types.as_ref())
        .filter(|types| !types.is_empty());
    if let Some(types) = types {
        fields.push(ObjectField::text("Policy Types", types.join(", ")));
    }
    let ingress: Vec<String> = spec
        .and_then(|spec| spec.ingress.as_ref())
        .into_iter()
        .flatten()
        .map(|rule| policy_rule("from", rule.from.as_ref(), rule.ports.as_ref()))
        .collect();
    lines(&mut fields, "Ingress Rules", ingress);
    let egress: Vec<String> = spec
        .and_then(|spec| spec.egress.as_ref())
        .into_iter()
        .flatten()
        .map(|rule| policy_rule("to", rule.to.as_ref(), rule.ports.as_ref()))
        .collect();
    lines(&mut fields, "Egress Rules", egress);

    vec![ObjectSection::new("Network Policy", fields)]
}
