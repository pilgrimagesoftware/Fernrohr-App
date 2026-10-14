---
title: Browse resources
version: latest
---

# Browse resources

The Resource panel lists every kind of object a cluster serves. Opening a kind
opens a live list of its objects. This page covers finding a kind, reading a
list, choosing namespaces, and browsing events.

## Open a resource list

The Resource panel lists every kind the cluster reports, built-in kinds and
custom resources alike. To reach it from the keyboard, run **Focus Resources**
(<kbd>⌘0</kbd>).

| Action | Keyboard | Mouse |
| --- | --- | --- |
| Move between kinds | <kbd>↓</kbd> and <kbd>↑</kbd> | Click a kind |
| Open the selected kind | <kbd>Enter</kbd> | Double-click it, or choose Open from its context menu |

Opening a kind that already has a panel for the same cluster and namespace
scope focuses that panel instead of opening a second one. A list updates live:
rows appear, change and disappear as the cluster changes, with no refresh.

If you aren't allowed to list a kind, the panel shows the API server's refusal
where the rows would be.

## Move between groups and kinds

Kinds sit under sections in a fixed order: Workloads, Config, Network, Storage,
Cluster, Access Control and Custom Resources. A section with no kinds in this
cluster isn't shown. Each header shows how many kinds it holds.

| Action | Keyboard | Mouse |
| --- | --- | --- |
| Collapse a section | <kbd>←</kbd> | Click its header |
| Expand a section | <kbd>→</kbd> | Click its header |
| Focus the filter | <kbd>/</kbd> | Click the filter box at the bottom of the panel |
| Clear the filter and return to the list | <kbd>Esc</kbd> | Clear the box |

Custom Resources is split into subgroups headed by API group, in alphabetical
order. Every subgroup starts collapsed, and so does a group the cluster adds
later.

| Command | Default key |
| --- | --- |
| Toggle Resource Group | <kbd>Space</kbd> |
| Collapse All Resource Groups | <kbd>⇧←</kbd> |
| Expand All Resource Groups | <kbd>⇧→</kbd> |

Toggling a group from a kind inside it collapses the group and moves focus to
its header. These keys don't act while the filter box has focus, and they leave
the top-level sections alone.

The filter matches a kind's label, name, plural and API group, ignoring case,
so `batch` finds the kinds in the `batch` group. A matching kind in a collapsed
section or subgroup is shown anyway, and clearing the filter puts everything
back as it was. Collapse state belongs to the window and isn't saved, so a new
window starts with sections expanded and subgroups collapsed.

If the window has more than one cluster connection, a dropdown at the top of the
panel chooses which cluster's kinds it shows.

## Read a list

A list always has Name and Age, plus Namespace for a namespaced kind. Many kinds
add their own columns. A Services list, for example, adds type, cluster IP,
external IP and ports, and a Deployments list adds replica counts. A kind with
nothing extra to show has only the base columns.

Any other kind, including custom resources such as Flux's HelmRelease and
Kustomization, gains two columns once one of its objects reports
`status.conditions`:

- **Ready** shows the `Ready` condition's status (True, False or Unknown) beside
  a dot in its tone. It appears only if some object has a `Ready` condition.
  Sorting ascending puts False first, so failures surface at the top.
- **Message** shows the `Ready` condition's message, coloured green for True,
  red for False and amber for anything else. Without a `Ready` condition, it
  shows the most recently changed condition's message, uncoloured.

A long message is cut off in its cell. Select the row and press <kbd>D</kbd>, or
double-click it, to read the whole message in the object's detail panel.

- **Sort.** Click a column header to sort ascending, again for descending, and a
  third time to return to the default sort, which is the first column of the
  kind's default order. An arrow on the header shows the current sort. Numeric
  columns sort as numbers.
- **Filter.** Press <kbd>/</kbd> and type to narrow the rows by name.
- **Resize and reorder.** Drag a column boundary to resize it, or drag a header
  to move it.
- **Move the selection.** <kbd>↓</kbd> on a list with no selection picks the
  first row, and <kbd>↑</kbd> picks the last. Hovering never moves the
  selection.

Each tab is titled with the kind's plural name, such as "Pods". A custom
resource tab shows the kind only, and hovering the tab shows its API group.

### Open an object

| Action | Keyboard | Mouse |
| --- | --- | --- |
| Open the object's detail | <kbd>Enter</kbd> or <kbd>D</kbd> | Double-click the row |
| Open it on its YAML | <kbd>Y</kbd> | Choose it from the command palette |
| Open it as an inactive tab | <kbd>⌘Enter</kbd> | <kbd>⌘</kbd>-click or middle-click the row |
| Open it in a new panel, even if it's open | <kbd>⇧Enter</kbd> | <kbd>⌥</kbd>-click the row, or choose **Open in New Panel** from its menu |

Opening in the background leaves focus and the selection in the list. See
{{ '/latest/details/' | relative_url }} for what the detail panel shows.

A Pods list adds a few more routes. <kbd>Space</kbd> (**Pods: Quick Look**) opens
a popover on the selected pod showing its phase, ready containers, restarts,
age, node, IP, owner, each container's image and state, and its latest warning.
<kbd>↓</kbd> and <kbd>↑</kbd> move to the next or previous pod without closing
it, <kbd>Enter</kbd> opens the pod's detail, and <kbd>Esc</kbd> closes it.
Right-clicking a row selects it and opens a menu with Quick Look, Open Details, Open in New Panel,
Logs and YAML.

## Icons and status

Every kind has a full-colour icon from the Kubernetes community icon set. It
appears beside the kind's name in panel tabs and title bars, the Resource panel,
detail headers and links. A kind the set doesn't cover gets a fallback icon: one
for containers, one for custom resource instances and a generic one for anything
else. Icons are decorative. They never take keyboard focus and grow with the
text size. The About window credits the icon set.

Status columns show their text and are also coloured by severity, so a state
reads without relying on colour. A pod's phase and each container's state use
the success colour for Running and Succeeded, the info colour for Pending and
Waiting, and the danger colour for Failed, Error and CrashLoopBackOff.

## Choose namespaces

A namespaced list shows a namespace picker in its title bar, and its button
reads the current scope: a single name, a count such as `2 namespaces`, or "All
namespaces". The scope is saved with the panel. A new panel starts on its
context's default namespace.

| Action | Keyboard | Mouse |
| --- | --- | --- |
| Open the picker | <kbd>N</kbd> | Click the namespace button |
| Narrow the list | Type; the filter has focus when the picker opens | Click the filter box |
| Toggle the highlighted namespace | <kbd>↓</kbd>, then <kbd>Enter</kbd> | Click it |
| Clear the filter, then close | <kbd>Esc</kbd>, then <kbd>Esc</kbd> again | Click away |

The picker stays open while you toggle several namespaces, and the filter is
empty each time it reopens. "All namespaces" is always listed first.

In a Pods list, <kbd>W</kbd> (**Pods: Filter to Selected Pod's Namespace**) scopes
that panel to the selected pod's namespace. <kbd>⇧W</kbd> (**Pods: Warp All to
Selected Pod's Namespace**) does the same for every namespaced panel in the
context and makes it the context's default, so panels opened later start there.
Cluster-scoped panels such as Nodes are never changed. Other lists bind <kbd>W</kbd>
(**List: Filter to Selected Object's Namespace**).

### Namespace sets

A namespace set is a named group of namespaces that you keep across launches.

| Command | Default key |
| --- | --- |
| Create Namespace Set… | <kbd>⌘⌥N</kbd> |
| Edit Namespace Set… | <kbd>⌘⌥E</kbd> |
| Switch Namespace Set… | <kbd>⌘⇧N</kbd> |
| Apply Namespace Set to Context… | <kbd>⌘⌥⇧N</kbd> |
| Remove Namespace Set… | No default key; use the command palette |

Creating a set starts it with the namespaces the focused panel is scoped to, so
you can name what is already on screen. A set needs a name that no other set
uses and at least one namespace. Dismissing the editor saves nothing. The editor
lists the cluster's namespaces, filtered as you type, and marks the ones in the
set. A namespace the cluster doesn't have can stay in a set; it matches nothing
here. Removing a set asks for confirmation first.

**Switch Namespace Set…** opens a picker on the focused namespaced list. Sets
appear in the order you created them, with digits <kbd>1</kbd> to <kbd>9</kbd>
beside the first nine. Press a digit to apply that set, or use the arrows and
<kbd>Enter</kbd>. The tenth and later sets have no digit. <kbd>Esc</kbd> closes
the picker without changing anything. From a panel with no namespace scope, the
picker reports that it needs a namespaced list.

**Apply Namespace Set to Context…** applies a set to every namespaced panel in
the window's active context and makes it that context's default. Panels in other
contexts and cluster-scoped panels don't change.

A panel's namespace button shows the set's name while its scope exactly matches
that set. Editing or removing a set doesn't change panels already switched to it,
and a panel that no longer matches any set shows a count instead.

## Browse events

The events browser lists every event the cluster retains that you may list,
newest first by last seen time. Its columns are last seen, type, reason,
involved object, message, count and source. It updates live and reads events
recorded through either event API.

Open it by opening Events from the Resource panel. The panel opens for that
cluster context rather than as a generic list. While the first list loads it
shows "Loading events…" with a spinner and a running count.

| Action | Keyboard | Mouse |
| --- | --- | --- |
| Search the visible columns, message included | <kbd>/</kbd> | Click the search box |
| Filter by type (Warning or Normal) | <kbd>T</kbd> | Use the type filter |
| Filter by object kind | <kbd>K</kbd> | Use the kind filter |
| Filter by reason | <kbd>R</kbd> | Use the reason filter |
| Clear all filters | <kbd>X</kbd> | Clear them individually or all at once |
| Pick namespaces | <kbd>N</kbd> | Click the namespace button |
| Open the involved object | <kbd>Enter</kbd> | Click the involved object |

Each filter is a multi-select offering the values present in the current
events. Filters combine with the panel's namespace scope and persist with the
panel, so a saved layout reopens with the same filters. Selecting an event shows
its whole message, which you can select and copy.

Sorting works like any list. The browser remembers the last sort you chose,
across contexts, windows and restarts, and a new browser starts with it. Until
you choose one, the sort is age ascending, which puts the newest events first.

*Summarizes the capability specs: `resource-browser`, `resource-group-navigation`, `resource-icons`, `namespace-sets`, `events-browser`.*
