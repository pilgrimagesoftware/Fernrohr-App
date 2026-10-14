---
title: Inspect resource details
version: latest
---

# Inspect resource details

A detail panel shows one object: its metadata, the sections that matter for its
kind, the events that name it, and its YAML. Detail panels dock like any other
panel and are restored with your layout.

## Open a detail view

From a list, select a row and press <kbd>Enter</kbd> or <kbd>D</kbd>, or
double-click the row. <kbd>Y</kbd> opens the object showing its YAML. To open it
without leaving the list, press <kbd>⌘Enter</kbd> or <kbd>⌘</kbd>-click or
middle-click the row, which adds an inactive tab and keeps focus where it was.
Following a link from another detail panel opens one too, as does the
involved object of an event.

A Pod opens the pod detail panel and every other kind opens the object detail
panel. Asking for an object whose panel is already open in that cluster focuses
the existing panel rather than opening a second one.

Both panels follow the object live. Changes in the cluster show within a few
seconds, and the panel keeps its tab, scroll position and revealed or expanded
state. If the object is deleted while the panel is open, the panel says so and
when, keeps its last known state visible as stale, and stays open until you close
it. A pod recreated under the same name, such as `web-0` in a StatefulSet, takes
over the open panel and the panel notes that it replaced the deleted pod.

### Common keys

| Command | Default key |
| --- | --- |
| Toggle Fields/YAML | <kbd>Y</kbd> |
| Copy Resource Name | <kbd>C</kbd> |
| Copy YAML | <kbd>⇧C</kbd> |
| Fold All YAML | <kbd>Z</kbd> |
| Unfold All YAML | <kbd>⇧Z</kbd> |
| Hide Secret Values | <kbd>H</kbd> |
| Go to Reference… | <kbd>G</kbd> |

In the palette these are prefixed "Pod Detail:" or "Object Detail:" for the panel
that has focus. The YAML view scrolls, and you can fold and unfold each nested
block with the keyboard or the mouse. Hovering a copyable value, such as a
container image, a ConfigMap value or a status message, shows a copy control
that Tab also reaches.

## Object detail

Every object opens with its creation time, name, namespace, labels, annotations
and owners. Labels and annotations are shown as chips. A value longer than 100
characters or spanning several lines shows its first 20 characters and an
ellipsis, and hovering the chip shows the whole value. The YAML view always
shows every value in full.

Beyond that, these kinds have sections of their own: those in the table, plus
Job and ServiceAccount. Any other kind whose object reports `status.conditions`,
such as a Flux HelmRelease or Kustomization, gets a Status section instead. It
shows the object's Ready status, its status message and every condition as a
badge. Any other kind shows metadata only.

| Kind | What it shows |
| --- | --- |
| Node | Addresses, capacity and allocatable resources, conditions as coloured badges, node info, taints |
| ConfigMap | Each key with its value |
| Secret | Its type and each key with its size; see below |
| PersistentVolumeClaim | Status, capacity, access modes, and its volume and storage class |
| PersistentVolume | Capacity, access modes, reclaim policy, phase, volume source, claim and storage class |
| StorageClass | Provisioner, parameters, reclaim policy, binding mode, whether expansion is allowed and whether it is the default |
| Deployment, StatefulSet, DaemonSet, ReplicaSet | Desired, ready and available replicas, selector, conditions |
| CronJob | Schedule, suspend flag, concurrency policy, last schedule and last success, and each active Job |
| Service | Type, cluster and external IPs, each port with protocol, target and node port, selector, session affinity |
| Ingress | Class, each rule's host and paths, TLS hosts, and each backend Service and TLS Secret |
| Endpoints, EndpointSlice | Each address with its ready state and ports, and each target Pod |
| NetworkPolicy | Pod selector, policy types, and each rule's peers and ports |
| Namespace | Phase and conditions |
| Role, ClusterRole | Each rule's API groups, resources, resource names and verbs |
| RoleBinding, ClusterRoleBinding | The role and each subject |
| Event | Type as a coloured badge, reason, full message, count, first and last seen, reporting component, action, and the involved and related objects |

The status message is the `Ready` condition's message, coloured by its status:
green for True, red for False and amber for anything else. Without a `Ready`
condition, it's the most recently changed condition's message, uncoloured. The
kinds above that show conditions as badges show this message right before them.
The list's Message column shows the same text.

The panel also lists the events that name the object, newest first. If you can
read the object but not list events, the object still loads and the events
section says why it's empty. For objects that select pods, such as workloads and
Services, **Object Detail: Logs of Its Pods** (<kbd>L</kbd>) opens their logs.

### Secrets

A Secret's value is never shown until you ask. The structured view lists each
key with its size and a reveal control. Activate one to show that value alone. It
hides again when you hide it, close the panel, or run **Object Detail: Hide
Secret Values** (<kbd>H</kbd>). The YAML view always replaces `data` and
`stringData` values, and any last-applied-configuration annotation, with a
placeholder giving the size, even while a value is revealed. Values are never
saved to disk, logged, or put in a title, tab name, palette entry or notification.

> Revealing a value shows it on screen to anyone looking. Hide it when you're done.
{: .warning}

## Pod detail

The pod detail panel opens on a structured view. Press <kbd>Y</kbd> to switch to
the pod's YAML and again to return. The structured view has six tabs, and a
number key selects each one.

| Key | Tab | What it shows |
| --- | --- | --- |
| <kbd>1</kbd> | Overview | Creation time, name, namespace, labels and annotations, owner, status, node, host and pod IPs, service account, QoS class, termination grace period, tolerations, conditions, and recent warnings |
| <kbd>2</kbd> | Containers | A card for each container |
| <kbd>3</kbd> | Configuration | Every ConfigMap and Secret the pod references |
| <kbd>4</kbd> | Volumes | The pod's volumes |
| <kbd>5</kbd> | Events | Events about this pod |
| <kbd>6</kbd> | Managed Fields | One row per field manager |

You can also click a tab. Conditions show as badges coloured by whether their
status is `True`. A pod's phase and each container's state are coloured by
severity and keep their text. A pod being deleted shows as Terminating with its
remaining grace period.

### Containers

Each container has a card with its summary. Expand a card to see its environment
variables, volume mounts, probes, command and args, and security context. An
environment variable taken from a Secret or ConfigMap shows its source kind,
name and key, and never the resolved value. Init containers show their real
state: waiting with the reason and message, running, or terminated with an exit
code and reason. A waiting init container is never shown as completed.

### Configuration

This tab lists each referenced ConfigMap and Secret once, however many ways the
pod uses it. Each entry links to the object, says where the pod uses it, and shows
its contents. A ConfigMap shows its keys with their values, and a Secret shows
keys with their sizes and a reveal control, operated with Tab and
<kbd>Enter</kbd> or <kbd>Space</kbd>. A Secret that doesn't exist, such as an
optional one, says so while the other entries still show. Nothing is read until
you open the tab.

A ConfigMap value longer than 100 characters or spanning several lines starts
collapsed to its first 20 characters, with an expand control beside the key name.
It collapses again each time you return to the tab. A revealed Secret value is
shown in full and hides again when you leave the tab or run **Pod Detail: Hide
Secret Values** (<kbd>H</kbd>).

### Events

The Events tab lists events whose involved object is this pod, newest first, with
age and repeat count, and updates live. It shows only events last seen inside the
time window, and says how many older ones are hidden. If listing events fails,
for example because RBAC forbids it, the tab says so and the pod's other fields
still load.

The window can be 15 minutes, 1 hour, 6 hours, 24 hours or All. Your choice
becomes the default for pod detail panels opened afterwards, including after a
restart.

| Command | Default key |
| --- | --- |
| Pod Detail: Events Window - Shorter | <kbd>[</kbd> |
| Pod Detail: Events Window - Longer | <kbd>]</kbd> |

Each fixed window is also a palette command, such as **Pod Detail: Events Window -
1 hour**. The Overview tab shows the most recent few Warning events inside the
window, with a link to this tab.

### Managed Fields

One row per field manager shows the manager, the operation and the time. Rows
start collapsed. Expand one, by keyboard or mouse, to see that manager's fields.

### Logs

**Pod Detail: View Logs** (<kbd>L</kbd>) opens the Logs panel for this pod, or
focuses it if it's already open. For a pod with several containers, logs start on
the first container and you can choose another. See
{{ '/latest/logs-shells/' | relative_url }}.

## Follow links to related resources

A reference to another object is shown as a link when Fernrohr has a panel for
its kind, and as plain text otherwise. Plain text does nothing when activated.

Detail panels link the references that follow.

- Every object: its namespace and each owner, each as its own link.
- A pod: its node, service account, image pull secrets, and every ConfigMap,
  Secret and PersistentVolumeClaim named by its volumes, `envFrom` or `valueFrom`.
- Kinds with their own sections, as in the table above: the volumes, backends,
  roles, subjects and target pods they name. A ReplicaSet links to its Deployment,
  for example.

A reference by name alone to a namespaced object resolves in the referencing
object's namespace.

| Action | Keyboard | Mouse |
| --- | --- | --- |
| Follow a link | <kbd>G</kbd> (**Go to Reference…**), choose a link, <kbd>Enter</kbd> | Click it |
| Cancel the chooser | <kbd>Esc</kbd> | Click away |
| Open the target as an inactive tab | None | <kbd>⌘</kbd>-click or middle-click |

Following a link opens the target in the same cluster context and focuses it,
and the panel you came from stays open. If the target is already open, its panel
is focused instead. <kbd>⌘</kbd>-click or middle-click opens it in the background
and leaves focus where it was, and does nothing if it's already open. The
chooser doesn't follow the pointer, so hovering an entry doesn't change the
choice. The panel's shortcut hints show the current key for **Go to Reference…**.

If the target was deleted or never existed, such as an optional Secret that isn't
there, its panel opens and says the object doesn't exist.

*Summarizes the capability specs: `object-detail`, `pod-detail`, `resource-links`.*
