---
title: Helm releases
version: latest
---

# Helm releases

Fernrohr lists a cluster's Helm releases by reading the Secrets Helm 3 and Helm 4 store them in,
so you don't need the `helm` CLI to see them. The list stays live: an install, upgrade, rollback or
uninstall shows up without refreshing.

## Requirements

Listing releases and reading their history, values and manifests need nothing beyond the cluster
connection. Rolling back or uninstalling a release runs `helm` itself, so those actions need Helm 3
or 4 installed. Fernrohr uses the `helm` on your login shell's PATH, or the one you name in
**Settings → Tools** under **helm path**. Until it finds a usable `helm`, Rollback and Uninstall are
shown disabled, with the reason. Settings → Tools also says which `helm` it found. See
{{ '/latest/settings/' | relative_url }}.

`helm` reaches the cluster the way the context does, through its tunnel if it has one, and signs in
with the context's own credentials. Your kubeconfig isn't changed.

## Open the list

- **Keyboard:** run **Helm Releases** from the command palette (<kbd>⌘⇧P</kbd>), or focus the
  Resource panel (<kbd>⌘0</kbd>), move to **Helm Releases** (last in Workloads) and press
  <kbd>Enter</kbd>. Typing `helm` in the Resource panel's filter finds it.
- **Mouse:** double-click **Helm Releases** in the Resource panel's Workloads section.

The list opens for the window's active context, on its default namespace once **Warp All to
Namespace** has set one, and on every namespace otherwise. Choose namespaces with the panel's
namespace picker (<kbd>N</kbd>), as in any list.

## What it shows

One row per release, not per revision:

| Column | What it shows |
| --- | --- |
| Name | The release name. |
| Namespace | The namespace the release is installed in. |
| Revision | The latest revision number. |
| Status | `deployed`, `failed`, `pending-install`, `pending-upgrade`, `pending-rollback`, `uninstalling` or `superseded`. |
| Chart | The chart name and version, such as `my-chart-1.4.2`. |
| App Version | The chart's app version. |
| Updated | How long ago the latest revision was deployed. |

A failed release is drawn in the error color with an alert icon, and one in progress in the
warning color with a progress icon, so a status reads without color too. If Fernrohr can't read a
release's latest revision, its Chart column says so and the other releases still list.

Search with <kbd>/</kbd>: it matches the name, namespace, chart and status. Sort by any column, as in
other lists.

### Uninstalled releases

A release uninstalled with `helm uninstall --keep-history` keeps its history, and its latest status
is `uninstalled`. Like `helm list`, the list hides these. Run **Helm Releases: Show Uninstalled
Releases** from the palette to show them, and again to hide them. The panel remembers the choice.

## A release's details

Open a release from its row: press <kbd>Enter</kbd> or <kbd>D</kbd>, or double-click it. With no row
selected, <kbd>↓</kbd> selects the first. The release's panel opens in the same context and follows
the release live: a new revision updates every tab, and if the release is uninstalled the panel
says it no longer exists.

| Tab | Key | What it shows |
| --- | --- | --- |
| Overview | <kbd>1</kbd> | Name, namespace, status, revision, chart and app version, first and last deployed, and the last operation's description. |
| History | <kbd>2</kbd> | Every revision, newest first, with its status, when it was deployed, its chart and app version, and its description. The deployed revision is marked. Move between revisions with <kbd>↓</kbd> and <kbd>↑</kbd>, or click one. |
| Notes | <kbd>5</kbd> | The chart's notes, as `helm get notes` prints them. |

Click a tab to show it too. Each tab is also in the command palette while the panel has focus.

## Access

Helm keeps releases in Secrets, so the list needs permission to list Secrets. If listing them in
every namespace is refused, the list says so and suggests choosing a namespace you may read with the
namespace picker. Scoped to some namespaces, the list reads each one on its own, so access to
those namespaces is enough.

Fernrohr reads only the Secrets' labels to list releases, and fetches each release's latest
revision once to show its chart. It never shows a release's stored values from the list.
