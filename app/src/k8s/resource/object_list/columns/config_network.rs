//! Config and Network kinds' list columns (`standard-resource-panels` 2.3), as
//! `kubectl get` shows them: ConfigMap, Secret, Service, Ingress, Endpoints,
//! EndpointSlice and NetworkPolicy.

use super::{Cell, ColumnDef, KindColumns, typed_cells};
use k8s_openapi::api::core::v1::{Endpoints, Service};
use k8s_openapi::api::discovery::v1::EndpointSlice;
use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::DynamicObject;

const fn column(id: &'static str, title: &'static str, width: f32) -> ColumnDef {
    ColumnDef { id, title, width }
}

const DATA: ColumnDef = column("data", "Data", 60.);

/// How many addresses an endpoints column lists before "+ N more", as
/// `kubectl get endpoints` truncates them.
const SHOWN_ADDRESSES: usize = 3;

/// The number of keys under `data` and `binaryData`. Read from the object's
/// JSON rather than its typed form: a count needs no decoding, and a Secret
/// value that isn't valid base64 still counts.
fn key_count(object: &DynamicObject) -> Cell {
    let keys = |field: &str| {
        object
            .data
            .get(field)
            .and_then(serde_json::Value::as_object)
            .map_or(0, serde_json::Map::len)
    };
    Cell::Number((keys("data") + keys("binaryData")) as i64)
}

/// `a, b, c + 2 more`.
fn truncated(addresses: Vec<String>) -> Cell {
    let hidden = addresses.len().saturating_sub(SHOWN_ADDRESSES);
    let mut shown = addresses
        .into_iter()
        .take(SHOWN_ADDRESSES)
        .collect::<Vec<_>>()
        .join(", ");
    if hidden > 0 {
        shown.push_str(&format!(" + {hidden} more"));
    }
    Cell::text(shown)
}

pub(super) static CONFIG_MAP: KindColumns = KindColumns {
    columns: &[DATA],
    cells: |object| vec![key_count(object)],
};

pub(super) static SECRET: KindColumns = KindColumns {
    columns: &[column("type", "Type", 220.), DATA],
    cells: |object| {
        let type_ = object
            .data
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Opaque");
        vec![Cell::text(type_), key_count(object)]
    },
};

pub(super) static SERVICE: KindColumns = KindColumns {
    columns: &[
        column("type", "Type", 100.),
        column("cluster_ip", "Cluster IP", 120.),
        column("external_ip", "External IP", 140.),
        column("ports", "Ports", 160.),
    ],
    cells: |object| {
        typed_cells::<Service>(object, |service| {
            let spec = service.spec.as_ref();
            let type_ = spec
                .and_then(|spec| spec.type_.as_deref())
                .unwrap_or("ClusterIP");
            let ingress: Vec<String> = service
                .status
                .as_ref()
                .and_then(|status| status.load_balancer.as_ref()?.ingress.as_ref())
                .into_iter()
                .flatten()
                .filter_map(|ingress| ingress.ip.clone().or_else(|| ingress.hostname.clone()))
                .collect();
            let external: Vec<String> = spec
                .and_then(|spec| spec.external_ips.clone())
                .unwrap_or_default()
                .into_iter()
                .chain(ingress)
                .collect();
            // A LoadBalancer the cloud hasn't given an address yet reads as
            // pending, as `kubectl` shows it.
            let external = if external.is_empty() && type_ == "LoadBalancer" {
                Cell::text("<pending>")
            } else {
                Cell::text(external.join(", "))
            };
            let ports: Vec<String> = spec
                .and_then(|spec| spec.ports.as_ref())
                .into_iter()
                .flatten()
                .map(|port| {
                    let protocol = port.protocol.as_deref().unwrap_or("TCP");
                    match port.node_port {
                        Some(node) => format!("{}:{node}/{protocol}", port.port),
                        None => format!("{}/{protocol}", port.port),
                    }
                })
                .collect();
            vec![
                Cell::text(type_),
                Cell::text(
                    spec.and_then(|spec| spec.cluster_ip.clone())
                        .unwrap_or_default(),
                ),
                external,
                Cell::text(ports.join(", ")),
            ]
        })
    },
};

pub(super) static INGRESS: KindColumns = KindColumns {
    columns: &[
        column("class", "Class", 90.),
        column("hosts", "Hosts", 200.),
        column("address", "Address", 140.),
        column("ports", "Ports", 70.),
    ],
    cells: |object| {
        typed_cells::<Ingress>(object, |ingress| {
            let spec = ingress.spec.as_ref();
            let hosts: Vec<String> = spec
                .and_then(|spec| spec.rules.as_ref())
                .into_iter()
                .flatten()
                .map(|rule| rule.host.clone().unwrap_or_else(|| "*".into()))
                .collect();
            let address: Vec<String> = ingress
                .status
                .as_ref()
                .and_then(|status| status.load_balancer.as_ref()?.ingress.as_ref())
                .into_iter()
                .flatten()
                .filter_map(|ingress| ingress.ip.clone().or_else(|| ingress.hostname.clone()))
                .collect();
            let tls = spec
                .and_then(|spec| spec.tls.as_ref())
                .is_some_and(|tls| !tls.is_empty());
            vec![
                Cell::text(
                    spec.and_then(|spec| spec.ingress_class_name.clone())
                        .unwrap_or_default(),
                ),
                Cell::text(if hosts.is_empty() {
                    "*".to_string()
                } else {
                    hosts.join(", ")
                }),
                Cell::text(address.join(", ")),
                Cell::text(if tls { "80, 443" } else { "80" }),
            ]
        })
    },
};

pub(super) static ENDPOINTS: KindColumns = KindColumns {
    columns: &[column("endpoints", "Endpoints", 280.)],
    cells: |object| {
        typed_cells::<Endpoints>(object, |endpoints| {
            // `ip:port` for every ready address and port, as `kubectl` lists them.
            let addresses: Vec<String> = endpoints
                .subsets
                .iter()
                .flatten()
                .flat_map(|subset| {
                    let ports: Vec<i32> = subset
                        .ports
                        .iter()
                        .flatten()
                        .map(|port| port.port)
                        .collect();
                    subset
                        .addresses
                        .iter()
                        .flatten()
                        .flat_map(move |address| {
                            if ports.is_empty() {
                                vec![address.ip.clone()]
                            } else {
                                ports
                                    .iter()
                                    .map(|port| format!("{}:{port}", address.ip))
                                    .collect()
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            vec![truncated(addresses)]
        })
    },
};

pub(super) static ENDPOINT_SLICE: KindColumns = KindColumns {
    columns: &[
        column("address_type", "Address type", 100.),
        column("ports", "Ports", 100.),
        column("endpoints", "Endpoints", 240.),
    ],
    cells: |object| {
        typed_cells::<EndpointSlice>(object, |slice| {
            let ports: Vec<String> = slice
                .ports
                .iter()
                .flatten()
                .filter_map(|port| port.port.map(|port| port.to_string()))
                .collect();
            let addresses: Vec<String> = slice
                .endpoints
                .iter()
                .flatten()
                .flat_map(|endpoint| endpoint.addresses.iter().cloned())
                .collect();
            vec![
                Cell::text(slice.address_type.clone()),
                Cell::text(ports.join(", ")),
                truncated(addresses),
            ]
        })
    },
};

/// A selector as `kubectl` prints it: `key=value` and `key op (values)`,
/// comma-separated, or `<none>` for one that selects everything.
fn selector_text(selector: Option<&LabelSelector>) -> String {
    let labels = selector
        .and_then(|selector| selector.match_labels.as_ref())
        .into_iter()
        .flatten()
        .map(|(key, value)| format!("{key}={value}"));
    let expressions = selector
        .and_then(|selector| selector.match_expressions.as_ref())
        .into_iter()
        .flatten()
        .map(|expression| {
            let values = expression.values.as_deref().unwrap_or_default().join(", ");
            format!("{} {} ({values})", expression.key, expression.operator)
        });
    let parts: Vec<String> = labels.chain(expressions).collect();
    if parts.is_empty() {
        "<none>".into()
    } else {
        parts.join(", ")
    }
}

pub(super) static NETWORK_POLICY: KindColumns = KindColumns {
    columns: &[column("pod_selector", "Pod selector", 200.)],
    cells: |object| {
        typed_cells::<NetworkPolicy>(object, |policy| {
            let selector = policy
                .spec
                .as_ref()
                .and_then(|spec| spec.pod_selector.as_ref());
            vec![Cell::text(selector_text(selector))]
        })
    },
};

#[cfg(test)]
mod tests;
