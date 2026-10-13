---
title: MCP tools reference
version: v0.10
---

# MCP tools reference

This list matches the tools registered in `app/src/mcp/tools.rs`. There are 17 tools. To connect an agent and for how approval works, see [Work with coding agents]({{ '/v0.10/agents/' | relative_url }}).

Every tool takes named arguments, and an unknown argument is rejected. Arguments marked required must be present. Each tool returns a structured result. A result cut short to stay under the reply size limit is marked `truncated`.

Tools fall into three kinds:

- **Read-only.** Nothing changes in the cluster or in the app, and no approval is asked. Fernrohr stays in the background.
- **Changes what the app shows.** Opens a panel, loads a layout or connects a context. These bring Fernrohr to the front and focus the window they acted on. They change no cluster object. `connect_context` asks for approval first, unless the window already holds the context.
- **Acts after in-app approval.** Changes a cluster object, only after you allow the exact action in Fernrohr.

Arguments named `context` are a context name as `list_contexts` reports it. Arguments named `kind` accept a kind's name (`Deployment`), plural (`deployments`) or singular (`deployment`) as `list_resource_kinds` reports it.

## Read-only tools

### list_contexts

Lists the kubeconfig contexts Fernrohr knows and whether each is open and connected in the app. It takes no arguments.

Returns `contexts`: a list of objects with `name` and `status`.

### list_resource_kinds

Lists the resource kinds a connected context's API discovery reports, with each one's group, version, plural, scope and supported verbs.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context to read. |

Returns `context` and `kinds`.

### list_resources

Lists objects of one kind in a connected context, in one namespace or in all of them. A page too large to return whole is cut short and marked `truncated`, with no `continue` token. Ask again with a smaller `limit`. Secret values are replaced by their sizes.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context to read. |
| `kind` | string | required | The resource kind. |
| `group` | string | optional | The API group, needed only when two groups have a kind of that name: `apps`, or `core` for the core group. |
| `namespace` | string | optional | The namespace to list. Omit it for every namespace. Only for namespaced kinds. |
| `label_selector` | string | optional | A label selector as `kubectl get -l` takes it, such as `app=web,tier!=db`. |
| `field_selector` | string | optional | A field selector as `kubectl get --field-selector` takes it, such as `status.phase=Running`. |
| `limit` | integer | optional | The most objects in one page, 1 to 500. 100 when omitted. |
| `continue` | string | optional | The `continue` token of the previous page, to fetch the next one. |

Returns `context`, `kind`, `namespace`, `count`, `page_size`, `items` and `continue` (the token for the next page, or null).

### get_resource

Returns one object's current representation from a connected context. Secret values are replaced by their sizes.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context to read. |
| `kind` | string | required | The resource kind. |
| `group` | string | optional | The API group, needed only when two groups have a kind of that name. |
| `namespace` | string | optional | The object's namespace. Required for a namespaced kind, and absent for a cluster-scoped one. |
| `name` | string | required | The object's name. |

Returns `context` and `object`.

### get_pod_logs

Returns the newest log lines of one container in a Pod, as a snapshot rather than a stream. When older output was dropped to fit, the result is marked `truncated`.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context to read. |
| `namespace` | string | required | The Pod's namespace. |
| `pod` | string | required | The Pod's name. |
| `container` | string | optional | The container to read. Needed only when the Pod has several. |
| `previous` | boolean | optional | Read the previous, terminated instance of the container, after a crash or restart. |
| `tail_lines` | integer | optional | How many of the newest lines to read, 1 to 5000. 200 when omitted. |
| `limit_bytes` | integer | optional | The most bytes of logs to return, 1 to 1048576. 262144 when omitted. |

Returns `context`, `namespace`, `pod`, `container`, `previous` and `logs`.

### list_layouts

Lists your saved layouts by name, with the contexts each was saved with. It takes no arguments. It never takes focus. The tools cannot save, rename or delete layouts.

Returns `layouts`: a list of objects with `name` and `contexts`.

## Tools that change what the app shows

### open_panel

Opens a resource panel in the frontmost window connected to the context, or focuses it if that window already shows it. Without `name` it opens the kind's list. With `name` it opens that object's detail panel. It never starts a connection.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context. A Fernrohr window must hold it. |
| `kind` | string | required | The resource kind. |
| `group` | string | optional | The API group, needed only when two groups have a kind of that name. |
| `namespace` | string | optional | Scope a namespaced list to this namespace. Omit it for the context's default scope. Required with `name` for a namespaced kind. |
| `name` | string | optional | Open this object's detail panel instead of the kind's list. |

Returns `panel_id` (an opaque ID, stable while the panel stays open) and `created` (whether a new panel was opened rather than an existing one focused).

### load_layout

Loads a saved layout into the frontmost Fernrohr window, as the Load Layout command does. A panel for a context the window isn't connected to restores as a placeholder. No context is connected.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `name` | string | required | The saved layout's name, as `list_layouts` gives it. Case doesn't matter. |
| `mode` | string | required | `replace` swaps the window's arrangement for the layout's. `add` opens the layout's panels beside the ones already open. |

Returns `name` and `placeholders`: a list of objects with the `context` each placeholder stands for.

### connect_context

Connects a kubeconfig context in the frontmost window, as the status bar's add-context control does, after you approve it in the app. It opens a window for the context when none is open. A context the window already holds is brought forward without a second connection and without asking. A name that isn't in the kubeconfig is an unknown-context error, and nothing is asked.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The kubeconfig context to connect. |

After approval it waits up to 20 seconds for the connection to settle. Returns `context`, `state`, `already_open`, and `reason` when the connection failed. `state` is one of:

- `connected`
- `waiting_for_tunnel`
- `awaiting_confirmation`, when you must confirm a manual tunnel
- `failed`, with a `reason` that has anything credential-like removed
- `connecting`, when the wait ended first. Check later with `list_contexts`.

A denied, timed-out or abandoned request connects nothing.

## Tools that act after approval

Each action tool opens a confirmation in Fernrohr naming the context, namespace, kind, every target and the parameters. The result's `outcome` field reports `approved` when you allowed it. A denied or timed-out request sends nothing to the cluster. The tool takes typed fields, so nothing outside the action's scope changes.

Every action tool requires `context` (string) and `namespace` (string) except where noted.

### set_configmap_value

Sets one key in a ConfigMap's `data`, or removes it when `value` is omitted. No other key or field changes. Removing a key that isn't there is rejected.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context. |
| `namespace` | string | required | The ConfigMap's namespace. |
| `name` | string | required | The ConfigMap's name. |
| `key` | string | required | The key in `data` to set or remove. |
| `value` | string | optional | The new value. Omit it, or pass null, to remove the key. |

Returns `outcome`, `context`, `namespace`, `name`, `key` and `removed`.

### scale_workload

Sets a workload's replica count through its scale subresource. The confirmation shows the current and requested counts.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context. |
| `namespace` | string | required | The workload's namespace. |
| `kind` | string | required | `Deployment`, `StatefulSet` or `ReplicaSet`. |
| `name` | string | required | The workload's name. |
| `replicas` | integer | required | The replica count to set. |

Returns `outcome`, `context`, `namespace`, `kind`, `name`, `previous_replicas` and `replicas`.

### restart_workload

Restarts a workload's pods by a rolling update, as `kubectl rollout restart` does.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context. |
| `namespace` | string | required | The workload's namespace. |
| `kind` | string | required | `Deployment`, `StatefulSet` or `DaemonSet`. |
| `name` | string | required | The workload's name. |

Returns `outcome`, `context`, `namespace`, `kind`, `name` and `restarted_at`.

### rollback_workload

Rolls a workload back to its previous revision, as `kubectl rollout undo` does. The confirmation shows the revision change, opens on Cancel and can't be undone.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context. |
| `namespace` | string | required | The workload's namespace. |
| `kind` | string | required | `Deployment`, `StatefulSet` or `DaemonSet`. |
| `name` | string | required | The workload's name. |

Returns `outcome`, `context`, `namespace`, `kind`, `name`, `from_revision` and `to_revision`.

### set_rollout_paused

Pauses or resumes a Deployment's rollout by setting `spec.paused`.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context. |
| `namespace` | string | required | The Deployment's namespace. |
| `name` | string | required | The Deployment's name. |
| `paused` | boolean | required | `true` to pause the rollout, `false` to resume it. |

Returns `outcome`, `context`, `namespace`, `kind`, `name` and `paused`.

### delete_pods

Deletes 1 to 10 Pods, named explicitly, in one namespace. Owning controllers recreate them. The confirmation lists each Pod, opens on Cancel and can't be undone. A request for 11 or more Pods is rejected without asking.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context. |
| `namespace` | string | required | The namespace all the Pods are in. |
| `names` | list of strings | required | The Pods to delete: 1 to 10 names, no repeats. |

Returns `outcome`, `context`, `namespace`, `deleted` (the names removed) and `failed` (a list with each Pod's `name` and `error`).

### trigger_cronjob

Creates a Job from a CronJob's template now, as `kubectl create job --from=cronjob/<name>` does. The confirmation shows the new Job's name.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context. |
| `namespace` | string | required | The CronJob's namespace. |
| `name` | string | required | The CronJob's name. |

Returns `outcome`, `context`, `namespace`, `cronjob` and `job`.

### set_cronjob_suspended

Suspends or resumes a CronJob's schedule by setting `spec.suspend`.

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `context` | string | required | The context. |
| `namespace` | string | required | The CronJob's namespace. |
| `name` | string | required | The CronJob's name. |
| `suspended` | boolean | required | `true` to suspend the schedule, `false` to resume it. |

Returns `outcome`, `context`, `namespace`, `name` and `suspended`.

*Summarizes the capability specs: `agent-mcp`; source registry `app/src/mcp/tools.rs`.*
