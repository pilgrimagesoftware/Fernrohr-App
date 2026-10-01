//! A container's expanded detail: env, volume mounts, probes, command/args
//! and security context, read off the same `Container` the summary card is
//! built from.

use super::format::non_empty;
use super::model::{ContainerDetail, EnvValue, EnvVarRow, chip};
use k8s_openapi::api::core::v1::{
    Container, EnvVar, EnvVarSource, Probe, SecurityContext, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

/// The API's default `periodSeconds` when a probe leaves it unset.
const DEFAULT_PROBE_PERIOD_SECONDS: i32 = 10;

pub(super) fn container_detail(container: &Container) -> ContainerDetail {
    let probes = [
        ("Liveness", &container.liveness_probe),
        ("Readiness", &container.readiness_probe),
        ("Startup", &container.startup_probe),
    ]
    .into_iter()
    .filter_map(|(kind, probe)| {
        probe
            .as_ref()
            .map(|probe| format!("{kind}: {}", format_probe(probe)))
    })
    .collect();

    ContainerDetail {
        env: container.env.iter().flatten().map(env_var_row).collect(),
        volume_mounts: container
            .volume_mounts
            .iter()
            .flatten()
            .map(format_volume_mount)
            .collect(),
        probes,
        command: container.command.clone().unwrap_or_default(),
        args: container.args.clone().unwrap_or_default(),
        security_context: container
            .security_context
            .as_ref()
            .map(security_context_chips)
            .unwrap_or_default(),
    }
}

/// A literal value verbatim, or a `valueFrom` reference described but never
/// resolved. `value_from` wins when both are set, since that is the source
/// the kubelet uses, and a literal shown next to it would be wrong.
fn env_var_row(var: &EnvVar) -> EnvVarRow {
    let value = match &var.value_from {
        Some(source) => describe_env_source(source),
        None => EnvValue::Literal(var.value.clone().unwrap_or_default()),
    };
    EnvVarRow {
        name: var.name.clone(),
        value,
    }
}

fn describe_env_source(source: &EnvVarSource) -> EnvValue {
    if let Some(secret) = &source.secret_key_ref {
        return EnvValue::SecretReference(format!(
            "from Secret {} key {}",
            secret.name, secret.key
        ));
    }
    EnvValue::Reference(if let Some(config_map) = &source.config_map_key_ref {
        format!("from ConfigMap {} key {}", config_map.name, config_map.key)
    } else if let Some(field) = &source.field_ref {
        format!("from field {}", field.field_path)
    } else if let Some(resource) = &source.resource_field_ref {
        match non_empty(&resource.container_name) {
            Some(container) => format!("from resource {} of {container}", resource.resource),
            None => format!("from resource {}", resource.resource),
        }
    } else if let Some(file) = &source.file_key_ref {
        format!(
            "from file {} in volume {} key {}",
            file.path, file.volume_name, file.key
        )
    } else {
        "from an unknown source".to_string()
    })
}

fn format_volume_mount(mount: &VolumeMount) -> String {
    let mut line = format!("{} -> {}", mount.name, mount.mount_path);
    if let Some(sub_path) = non_empty(&mount.sub_path).or(non_empty(&mount.sub_path_expr)) {
        line.push_str(&format!(" (subPath {sub_path})"));
    }
    if mount.read_only == Some(true) {
        line.push_str(" (ro)");
    }
    line
}

/// One readable line for a probe's handler and timing, rather than the raw
/// `Probe` union field by field - the same treatment
/// `format::format_container_state` gives `ContainerState`.
fn format_probe(probe: &Probe) -> String {
    let handler = if let Some(http) = &probe.http_get {
        let method = match http.scheme.as_deref() {
            Some("HTTPS") => "HTTPS GET",
            _ => "HTTP GET",
        };
        let path = non_empty(&http.path).unwrap_or("/");
        format!("{method} {path}:{}", port_text(&http.port))
    } else if let Some(tcp) = &probe.tcp_socket {
        format!("TCP :{}", port_text(&tcp.port))
    } else if let Some(grpc) = &probe.grpc {
        match non_empty(&grpc.service) {
            Some(service) => format!("gRPC {service}:{}", grpc.port),
            None => format!("gRPC :{}", grpc.port),
        }
    } else if let Some(exec) = &probe.exec {
        format!(
            "exec {}",
            exec.command
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        )
    } else {
        "no handler".to_string()
    };

    let period = probe.period_seconds.unwrap_or(DEFAULT_PROBE_PERIOD_SECONDS);
    match probe.initial_delay_seconds.filter(|delay| *delay > 0) {
        Some(delay) => format!("{handler} every {period}s after {delay}s"),
        None => format!("{handler} every {period}s"),
    }
}

fn port_text(port: &IntOrString) -> String {
    match port {
        IntOrString::Int(number) => number.to_string(),
        IntOrString::String(name) => name.clone(),
    }
}

/// The fields of a container `securityContext` a reader checks first,
/// left out when unset.
fn security_context_chips(context: &SecurityContext) -> Vec<String> {
    let mut chips = Vec::new();
    let mut flag = |key: &str, value: Option<String>| {
        if let Some(value) = value {
            chips.push(chip(key, &value));
        }
    };
    flag("privileged", context.privileged.map(|v| v.to_string()));
    flag("runAsUser", context.run_as_user.map(|v| v.to_string()));
    flag("runAsGroup", context.run_as_group.map(|v| v.to_string()));
    flag(
        "runAsNonRoot",
        context.run_as_non_root.map(|v| v.to_string()),
    );
    flag(
        "readOnlyRootFilesystem",
        context.read_only_root_filesystem.map(|v| v.to_string()),
    );
    flag(
        "allowPrivilegeEscalation",
        context.allow_privilege_escalation.map(|v| v.to_string()),
    );
    let capabilities = context.capabilities.as_ref();
    let joined = |list: Option<&Vec<String>>| {
        list.filter(|list| !list.is_empty())
            .map(|list| list.join(","))
    };
    flag(
        "capabilities.add",
        joined(capabilities.and_then(|c| c.add.as_ref())),
    );
    flag(
        "capabilities.drop",
        joined(capabilities.and_then(|c| c.drop.as_ref())),
    );
    flag(
        "seccompProfile",
        context.seccomp_profile.as_ref().map(|p| p.type_.clone()),
    );
    chips
}
