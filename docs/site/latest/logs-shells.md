---
title: Logs and shells
version: latest
---

# Logs and shells

Fernrohr streams a pod's container logs into a Logs panel and opens a shell in a running container
as a terminal panel. Both start from a pod, in the Pods panel or in the pod's detail panel.

## Open a pod's logs

Select a pod and open its logs. The panel shows the recent log history and keeps adding new lines as
the container writes them.

- **Keyboard:** press <kbd>L</kbd> in the Pods panel or in a pod's detail panel. The **Show Logs**
  command (<kbd>⌘2</kbd>) also brings up the Logs panel, and every command is in the command palette
  (<kbd>⌘⇧P</kbd>).
- **Mouse:** open the pod's row menu in the Pods panel and choose its logs entry.

By default each pod's logs open in a panel of their own, so you can keep several pods' logs side by
side. Opening the same pod's logs again focuses its panel instead of adding a second one, and
opening another container of that pod switches its panel to that container. A pod's panel stays on
its pod when you select other pods later, and it comes back showing that pod when you load a saved
layout.

### One shared panel instead

You can have every pod's logs open in one shared Logs panel that follows your selection instead.

- Choose **Open Every Pod's Logs in One Reused Panel** in the command palette, or set **Pod logs** to
  **One shared panel** in Settings. **Open Each Pod's Logs in Its Own Panel** and **Own panel per
  pod** switch it back.
- The choice applies at once and is kept across restarts.

Whichever way you have it set, <kbd>⇧L</kbd> opens a pod's logs the other way, for that one open.
With the default setting it opens the shared panel. The pod's row menu and the command palette offer
the same flipped open, as **Show Logs (Other Panel Mode)** and **Pod Detail: View Logs (Other Panel
Mode)**.

## Follow and scroll back

While follow is on, the view stays pinned to the newest line. Scroll up to read earlier output and
following pauses. It resumes when you scroll back to the bottom, or when you turn follow back on
with <kbd>S</kbd> (**Logs: Toggle Follow**, also in the palette).

If the pod is deleted or its container exits while you watch, the panel says the stream has ended
and keeps the lines already shown. If the log request is rejected, for example because the container
hasn't started, the panel shows the reason instead of an empty view.

<kbd>P</kbd> (**Logs: Toggle Previous Container Logs**) switches between the container's current
logs and those of its previous instance.

## Choose a container

A pod with one container streams that container's logs with no choice to make. For a pod with more
than one container, the Logs panel streams the first container to start with, and you can switch it
to another. Opening another container's logs of a pod that already has a Logs panel switches that
panel to the container.

## Follow pods by label

Press <kbd>⌘⇧L</kbd>, or choose **Open Logs by Label…** in the Window menu or the command palette,
to open a Logs panel that streams every pod a label selector picks. Type a selector in `kubectl -l`
form, such as `app=web` or `tier in (front, back)`, and press <kbd>Enter</kbd>. Each line is tagged with its pod, in a colour of
its own. Pods that start or stop matching join and leave the stream as the cluster changes.

Each time you run Open Logs by Label…, it opens a new, empty panel, so you can follow several
selectors side by side. For example, follow `app=web` in one panel and `app=worker` in another, in
the same context or in different ones. Each panel keeps its own selector, namespaces, container
filter and context, and its tab is named by its selector, such as **Logs: app: web**.

The panel opens in the context and namespaces of the list that had focus. Its bar shows where it
searches, and you can change that without leaving the panel:

| What | Keyboard | Mouse |
| --- | --- | --- |
| Edit the selector | <kbd>/</kbd> | Click the selector field |
| Choose the namespaces | <kbd>N</kbd> | Click the namespace button |
| Stream only one container from each pod | <kbd>C</kbd> cycles through the containers | Click the container button |
| Choose the context | <kbd>X</kbd>, then <kbd>↓</kbd> or <kbd>↑</kbd> and <kbd>Enter</kbd> | Click the context button |
| Start over | <kbd>⌃L</kbd>, also from the selector field | Click the clear button beside the selector |

Starting over stops every stream the panel holds, even one still connecting, and empties both the
log lines and the selector field. The panel is then as it opened, with the field focused, ready for a
new selector. Its namespaces and context stay as they were.

The context control appears only in a window holding more than one context. Choosing another
context keeps your selector and container filter, searches all of that context's namespaces, and
streams its pods in place of the old ones. The panel moves even when another label panel already
searches that context, and it stays in its tab with focus. A saved layout restores every label
panel, each in the context it last searched with its own selector. For example,
with `my-cluster` and `my-other-cluster` in one window, press <kbd>X</kbd> in a panel following
`app=web` on `my-cluster` and choose `my-other-cluster` to see the same selector's pods there.

### Label selectors

A selector is one or more requirements separated by commas, and a pod must meet all of them.
Hover the selector field to see these forms:

| Requirement | Picks pods whose labels | Shown as |
| --- | --- | --- |
| `app=web` | have `app` set to `web` | app: web |
| `app!=web` | don't have `app` set to `web` | app: not web |
| `tier in (front, back)` | have `tier` set to `front` or `back` | tier: back, front |
| `tier notin (front, back)` | don't have `tier` set to `front` or `back` | tier: not back, front |
| `canary` | have a `canary` label, whatever its value | has canary |
| `!canary` | have no `canary` label | no canary |

For example, `app=web, !canary` follows the `web` pods that aren't canaries.

Where Fernrohr shows a selector rather than letting you type it, it uses the **Shown as** form,
with requirements separated by a dot. That covers the tab of a panel you typed a selector into, and
the bar of a panel following a workload's pods, such as a Deployment's. The workload's bar keeps
the selector as typed, `kubectl -l` form, in a tooltip.

## Open a shell in a container

Select a running pod in the Pods panel, or open its detail panel, and open a shell.

- **Keyboard:** press <kbd>S</kbd>. The command is **Pods: Shell into Selected Pod** in the Pods
  panel and **Pod Detail: Shell into Pod** in the detail panel.
- **Palette:** run the same command from the command palette (<kbd>⌘⇧P</kbd>) while the Pods panel or the detail panel has focus.

When the pod runs more than one container, Fernrohr asks which one to open the shell in.

To run a saved command instead of a shell, use the exec library. See
{{ '/latest/exec-library/' | relative_url }}.

The shell is a real terminal (a Kubernetes exec with a TTY). Full-screen programs like `top` and
`vi`, Tab completion and <kbd>Ctrl</kbd>+<kbd>C</kbd> work as they would locally. ANSI colors show,
and the terminal resizes with its panel, so splitting the panel group makes a running full-screen
program redraw to the new width. The shell uses the app's code font, text size and colors.

## Keys inside a shell

While the terminal panel has focus and its session is running, it takes every key press, including
<kbd>Ctrl</kbd> keys, <kbd>Tab</kbd>, <kbd>Esc</kbd>, arrow keys and function keys. Letters that are
shortcuts elsewhere, such as <kbd>D</kbd>, <kbd>Y</kbd>, <kbd>S</kbd> and <kbd>L</kbd>, go to the
container too and run no Fernrohr command.

A few keys keep their app meaning:

| Action | macOS | Linux |
| --- | --- | --- |
| Copy | <kbd>⌘C</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> |
| Paste | <kbd>⌘V</kbd> | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> |
| Command palette | <kbd>⌘⇧P</kbd> | |
| Panel and tab navigation | The usual <kbd>⌘</kbd> shortcuts | |

The panel's hint row lists these keys while a session is running.

> Pasted text reaches programs that enable bracketed paste wrapped in bracketed-paste markers, so
> an editor in the container can tell a paste from typing.

To read earlier output, scroll the mouse wheel or trackpad. The terminal keeps at least 10,000 lines
of scrollback. Select text with the mouse and copy it with the copy shortcut. The copied text has no
terminal control codes. If the program in the container turns on mouse reporting, the mouse goes to
that program instead.

## When a shell ends

The session ends when you type `exit`, the container stops or the connection drops. The panel stops
accepting input and shows that the session ended, with the exit status when it is known. Its final
screen and scrollback stay on view, so you can still read, select and copy them.

*Summarizes the capability specs: `pod-logs`, `selector-logs`, `exec-terminal`.*
