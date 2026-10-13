---
title: Logs and shells
version: v0.10
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

## Open a shell in a container

Select a running pod in the Pods panel, or open its detail panel, and open a shell.

- **Keyboard:** press <kbd>S</kbd>. The command is **Pods: Shell into Selected Pod** in the Pods
  panel and **Pod Detail: Shell into Pod** in the detail panel.
- **Palette:** run the same command from the command palette (<kbd>⌘⇧P</kbd>) while the Pods panel or the detail panel has focus.

When the pod runs more than one container, Fernrohr asks which one to open the shell in.

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

*Summarizes the capability specs: `pod-logs`, `exec-terminal`.*
