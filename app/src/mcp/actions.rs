//! The allowlisted action tools (`agent-mcp`: Allowlisted cluster actions):
//! the only tools that change a cluster, each one named, typed and approved
//! by the user in the app first (design.md's table, exactly):
//!
//! | Tool | Targets |
//! |---|---|
//! | `set_configmap_value` | ConfigMap |
//! | `scale_workload` | Deployment, StatefulSet, ReplicaSet |
//! | `restart_workload` | Deployment, StatefulSet, DaemonSet |
//! | `rollback_workload` | Deployment, StatefulSet, DaemonSet |
//! | `set_rollout_paused` | Deployment |
//! | `delete_pods` | 1 to 10 named Pods in one namespace |
//! | `trigger_cronjob` | CronJob |
//! | `set_cronjob_suspended` | CronJob |
//!
//! Adding an action means adding a tool here, not widening one. Every tool
//! follows `flow`'s steps; the request each sends is built by its shared
//! `resource_actions` function from typed fields, so nothing outside the
//! action's own fields can change.

mod configmaps;
mod cronjobs;
mod flow;
mod inputs;
mod pods;
mod workloads;

use super::tools::ToolRegistry;

/// Every action tool's name: the allowlist, for tests to hold the registry to.
#[cfg(test)]
pub(super) const ALLOWLIST: [&str; 8] = [
    "delete_pods",
    "restart_workload",
    "rollback_workload",
    "scale_workload",
    "set_configmap_value",
    "set_cronjob_suspended",
    "set_rollout_paused",
    "trigger_cronjob",
];

/// Adds every action tool to `registry`.
pub(super) fn register(registry: &mut ToolRegistry) {
    configmaps::register(registry);
    workloads::register(registry);
    pods::register(registry);
    cronjobs::register(registry);
}

#[cfg(test)]
mod tests;
