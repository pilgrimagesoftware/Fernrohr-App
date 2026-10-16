---
title: Exec library
version: latest
---

# Exec library

The exec library keeps commands you run in pods again and again, so you don't retype them in a
shell or hunt for the right pod each time. Each entry runs its command in a shell panel, in the
pod it targets.

## Entries and targets

An entry has a name, a command, an optional container and a target. Names are unique, ignoring
case and surrounding spaces. The target says which pods the entry runs in:

| Target | Runs in |
| --- | --- |
| Any pod | The pod you run it from, with **Run from Library…** on a pod. |
| Pod selector | The Running pods that a label selector matches, in one namespace or, with no namespace, in all of them. The selector uses `kubectl -l` syntax, such as `app.kubernetes.io/name=my-app,tier!=cache`. |
| Workload | The Running pods of a Deployment, StatefulSet, DaemonSet or Service, given by namespace and name, as `kubectl exec deploy/my-app` would pick them. |

The container is the entry's own. If the entry names none, it is the pod's default container
(the `kubectl.kubernetes.io/default-container` annotation), and otherwise its first container.
An entry that names a container the pod doesn't have reports an error and runs nothing.

### Commands run without a shell

A command is split into words the way a POSIX shell splits them, quotes included, and is run
directly, as `kubectl exec -- …` does. It doesn't go through a shell, so pipes, `>` and `$VARS`
aren't expanded. To use them, run a shell yourself:

```sh
sh -c 'ls /data | wc -l'
```

A command with a quote that is never closed can't be saved.

## How a pod is picked

When an entry's target matches exactly one Running pod, the command runs there. When it matches
several, a list of them opens, sorted by namespace and then name, with the first one selected:
press <kbd>Enter</kbd> to run in it, choose another with the arrow keys or a click, or press
<kbd>Esc</kbd> to run nothing. When it matches none, an error names the entry and its scope.
Pending pods never count.

The command runs in a shell panel with a terminal attached, so interactive and colored output
work as in a shell. The panel's title is the entry's name and the pod. When the command exits, the
panel stays open with its output and exit status, as a shell's does. A command panel restored
after a restart shows that it ended. It never runs the command again.

## Run an entry from the library

Open the Exec Library window with **Open Exec Library** from the command palette or the Context
menu. Type to filter the entries by name, command or target, move with <kbd>↑</kbd> and
<kbd>↓</kbd>, and press <kbd>Enter</kbd> to run the selected entry.

The entry runs in the context of the cluster panel you opened the window from. With no cluster
panel behind it, Fernrohr asks which connected context to use.

Press <kbd>Tab</kbd> to move from the filter to the list. These keys then act on the selected
entry, and the window's hint row shows them:

| Action | Key | Command |
| --- | --- | --- |
| Run | <kbd>Enter</kbd> | **Exec Library: Run** |
| New entry | <kbd>N</kbd> | **Exec Library: New Entry…** |
| Edit entry | <kbd>E</kbd> | **Exec Library: Edit Entry…** |
| Duplicate entry | <kbd>D</kbd> | **Exec Library: Duplicate Entry** |
| Delete entry | <kbd>⌘⌫</kbd> | **Exec Library: Delete Entry…** |

The toolbar's icon buttons do the same. Hover one to see what it does and its key. Delete asks
first, with focus on Cancel, so <kbd>Enter</kbd> keeps the entry. Confirm with <kbd>⌘⌫</kbd>, or
<kbd>Tab</kbd> to Delete.

## Run or save from a resource

Pods, Deployments, StatefulSets, DaemonSets and Services offer two commands, in their row's
right-click menu and in the command palette while their row or detail panel has focus.

- **Run from Library…** lists only the entries that apply to the resource, and runs the one you
  choose there. On a pod, those are any-pod entries, selector entries that match the pod, and
  workload entries whose workload selects it. On a workload or Service, they are any-pod entries,
  entries for that workload, and selector entries that match one of its pods.
- **Save to Library…** opens a new entry prefilled from the resource. A pod starts as a pod
  selector: its namespace, and its controller's selector, or its own labels without the
  per-revision labels such as `pod-template-hash`. A workload or Service starts as that workload.

Fernrohr decides what applies from what its open lists already hold, and never fetches more to
decide. A workload entry is offered on a pod only while some panel lists that workload's kind. The
entry still runs from the library and from the workload itself.

## The entry dialog

The dialog has Name, Command, the target - Any pod, Pod selector or Workload, each a button you can
reach with <kbd>Tab</kbd> - its fields, and Container. It opens on the first empty required field.
<kbd>Tab</kbd> and <kbd>⇧Tab</kbd> move between fields and buttons.

| Action | Key |
| --- | --- |
| Save | <kbd>Enter</kbd> |
| Save and run | <kbd>⌘Enter</kbd> |
| Cancel | <kbd>Esc</kbd> |

A field that can't be saved shows why under it: a taken name, an empty command or one that doesn't
split, a selector that doesn't parse, or a workload with no namespace or name. Save and Run saves
the entry and runs it where you opened the dialog: in the library's context, or on the resource you
saved it from.

## The library file

The library is `exec-library.toml` in Fernrohr's preferences folder,
`~/Library/Preferences/com.pilgrimagesoftware.fernrohr/` on macOS and `~/.config/fernrohr/` on Linux.
It is saved on every change and keeps your entries in order. You can edit it by hand:

```toml
[[entries]]
name = "Stream info"
command = "nats stream info my-stream"
container = "nats-box"

[entries.target]
kind = "selector"
namespace = "my-namespace"
selector = "app.kubernetes.io/component=nats-box"

[[entries]]
name = "App shell"
command = "sh"

[entries.target]
kind = "workload"
namespace = "my-namespace"
workload_kind = "Deployment"
name = "my-app"
```

If the file can't be read, Fernrohr leaves it as it is. The library shows as empty and read-only,
with the file and the error in a banner, until you fix it.

> The file is plain text. Don't put passwords, tokens or other secrets in commands.
{: .warning}

*Summarizes the capability specs: `exec-command-library`.*
