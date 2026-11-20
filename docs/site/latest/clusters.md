---
title: Connect to clusters
version: latest
---

# Connect to clusters

Fernrohr lists the contexts in your kubeconfig and connects to the ones you choose. It reads
`$KUBECONFIG`, or `~/.kube/config` when that variable is unset, and never changes the file.

## Choose a context

A window with no connected context shows the cluster picker instead of the panel workspace. On
first launch, with nothing saved, that is the main window. A window that has saved panels for at
least one context opens straight into them, and skips the picker.

The picker lists each context from the kubeconfig.

1. Type in the search field to filter the list, or move through it with <kbd>↑</kbd> and
   <kbd>↓</kbd>.
2. Press <kbd>Enter</kbd> to connect to the highlighted context. With the mouse, click the context
   and then click Connect.

Hovering a row doesn't move the selection. Only a click, or the arrow keys and typing, does.

When the connection succeeds, the picker gives way to the panel workspace, which opens on a
default resource view for that context. Closing panels never disconnects a context or brings the
picker back.

If the kubeconfig has no contexts, the picker says "No kubeconfig contexts are available." If it
can't be read, the picker says "Could not read kubeconfig" with the reason. Check the file and
`$KUBECONFIG`.

If a context needs a tunnel, bind one to it from the row's tunnel selector, or press the
shortcut shown as Tunnel in the picker's hint row. See
[Tunnels and forwards]({{ '/latest/tunnels-forwards/' | relative_url }}).

## Add another context to a window

A window can hold several contexts at once.

- **Mouse:** click the plus button, "Add a context", at the left of the status bar.
- **Keyboard:** open the command palette with <kbd>⌘⇧P</kbd> and run Add Context to Window…. It
  has no default key.

The menu bar has it too, under Context.

It opens the picker in an Add a Context window of its own, listing the contexts this window doesn't
use yet. The window you opened it from stays usable while a context connects, so you can keep
working while a tunnel comes up. Choosing Add Context again brings the open one to the front.

Once the chosen context connects, it is added to your window with a Pods panel, and the Add a
Context window closes. If it fails, the failure stays in the Add a Context window so you can pick
another context or retry. Press <kbd>⎋</kbd> or close the window to add nothing.

A long context name is shortened with an ellipsis, and hovering it shows the full name. Widen the
window to see more of it. The tunnel selector stays at the end of each row.

## Watch the connection

The picker shows a status line while it connects:

| Status line | Meaning |
|---|---|
| Connecting to `my-cluster`... | The client is being built and the API server contacted. |
| Waiting for tunnel to `my-cluster`... | The context is bound to a tunnel, and Fernrohr waits for it to come up first. |
| `my-cluster` is waiting for you to confirm a manual tunnel | Bring the network path up, then choose Proceed. |
| Connected to `my-cluster` | The connection worked. |
| Could not connect to `my-cluster`: reason | The attempt failed. |

After the workspace opens, each context in the window has a capsule in the status bar. Its icon
and label are one of Connected, Waiting for tunnel, Reconnecting, Refreshing credentials,
Connection failed or Awaiting confirmation. States other than Connected also show how long they
have lasted, and a failure shows its reason. A context that needs your attention is filled in the
warning color and sorted to the front.

When a coding agent also uses the context, the capsule shows an agent icon beside its state. Point
at it to see which agents, named as they report themselves, for example "Also used by an agent
reporting itself as “assistant”". A name is whatever the agent's MCP client says, so treat it as a
label, not proof of who the agent is. The [Connections window](#see-every-connection) lists every
agent's connections, including ones no window shows.

A tunnel dropping, or a credential plugin needing a new sign-in, pauses the connection instead
of ending it. The status bar shows Reconnecting or Refreshing credentials once for the whole
window. Panels stay open with their last data, and resume when the tunnel returns or the
credentials refresh. When a manual tunnel is waiting, answer from the picker, from the capsule
menu, or with the Proceed with Manual Tunnel and Cancel Manual Tunnel commands.

## Errors and retrying

A failed attempt leaves the picker on screen with the reason, such as the unreachable API server's
name, the error output of a credential plugin that exited with an error, or the reason a bound
tunnel didn't come up. You can select the text and copy it. The **Report…** button below a failure
opens Report Issue with the error filled in. See
[Report an error from where it happened]({{ '/latest/settings/' | relative_url }}#report-an-error-from-where-it-happened).

To retry, press <kbd>Enter</kbd> or click Connect again, on the same context or another. Choosing
a different context needs no reset.

A context already in the window can fail too, for example when a restored workspace opens while
its cluster is unreachable. Its capsule shows Connection failed, and its panels say they couldn't
connect. Fix the cause, then try again:

- **Mouse:** open the menu on the context's capsule and choose **Reconnect**. Only a failed
  context's menu has it.
- **Keyboard:** run **Reconnect Failed Contexts** from the command palette or the Context menu. It
  tries every failed context in the window again and leaves the others alone. While none has
  failed, the palette lists it dimmed with the reason. It has no default key. To give it one, bind
  `context.reconnect_failed` in `keymap.toml`.

The context's panels stay open and fill in once the connection succeeds.

### When the connection times out

If the API server doesn't answer within 30 seconds, the reason says what timed out and what to
check, for example:

```text
Timed out after 30s connecting to the API server at api.my-cluster.example:443. Check that this
machine can reach that address: any VPN, tunnel or bastion the cluster needs is connected, and the
kubeconfig's server address is right.
```

When the connection goes through a proxy or a tunnel, the reason names that instead. Fernrohr
uses a proxy the same way `kubectl` does: the cluster's `proxy-url` from the kubeconfig if it has
one, otherwise `HTTPS_PROXY` from the environment, except for hosts that `NO_PROXY` names. A
context bound to an SSH tunnel never sends its tunnel's local port through a proxy. An app opened
from the Dock or Finder doesn't see proxy variables exported in your shell, so set `proxy-url` in
the kubeconfig for a cluster that needs a proxy.

> An unreachable API server or a failing credential plugin is a problem outside Fernrohr. Check
> that the cluster is up, that you can reach it, and that the plugin works in a terminal first.
{: .warning}

## Disconnect

- **Mouse:** open the menu on the context's capsule in the status bar, using the arrow beside its
  name, and choose Disconnect.
- **Keyboard:** open the command palette with <kbd>⌘⇧P</kbd> and run Disconnect Active Context…,
  which acts on the window's active context. It has no default key.

Fernrohr asks you to confirm. The dialog says how many panels for that context will close, and
that the context stays connected if other windows or agents still use it, naming each agent as it
reports itself. Disconnecting from the window leaves an agent's connection open; to end that too,
use the [Connections window](#see-every-connection). Confirm with <kbd>Enter</kbd> or
by clicking Disconnect, or cancel with <kbd>Esc</kbd>.

To disconnect several contexts at once, when the window has more than one:

- **Mouse:** on a capsule's menu, choose **Disconnect Others** to keep only that capsule's context,
  or **Disconnect All**.
- **Keyboard:** run **Disconnect Other Contexts…**, which keeps the active context, or
  **Disconnect All Contexts…** from the command palette or the Context menu. Neither has a
  default key.

One confirmation names every context that will disconnect and how many panels close with them.
Disconnecting every context returns the window to the cluster picker.

Connections are shared between windows. Two windows using the same context share one connection,
so disconnecting in one leaves the other receiving live updates. The connection, and any tunnel it
holds, shuts down when the last window using the context disconnects or closes.

## See every connection

The Connections window lists every connection Fernrohr has open, whichever window or MCP client
holds it. An agent can hold a connection that no window shows, so this is where you see it. Open
it with Window > Connections, with Connections… in the menu bar icon's menu, with
<kbd>⌘⇧C</kbd>, or by running Connections in the command palette. If it is already open, the same
command brings it forward.

Each row shows:

- The context name, then its tunnel in brackets if it has one, then its state with the same icon and
  label as the status bar's capsule. Hover the state for how long it has lasted and a failure's
  reason.
- Each holder. A window is listed by its title. An agent is listed as it named itself, for example
  `reports itself as "my-assistant"`, or as `Unnamed agent` if its client gave no name. The name is
  only what the client reported, so don't treat it as proof of who it is. The agent that opened
  the connection is marked "connected it", even after a window also uses it.

The list updates as connections open, change state and close. A row disappears when its last
holder lets go, and the window says "No context is connected" when none is left. A connection that
only an ended agent session still holds is listed with no holder named, and closes by itself
shortly after the session ended.

Move the selection with <kbd>↑</kbd> and <kbd>↓</kbd> or by clicking a row. Hovering never moves
it. Each row has three icon buttons, and the keys below act on the selected row. A hint row at the
bottom of the window shows the keys.

| Action | Key | What it does |
| --- | --- | --- |
| Open in window | <kbd>↵</kbd> | Adds the context to the frontmost window, with its Pods panel, or opens a window for it if none is open. The window uses the connection that is already open, so nothing is asked and nothing reconnects. |
| Disconnect… | <kbd>D</kbd> | Releases every holder. |
| End agent's hold | <kbd>E</kbd> | Ends one agent's hold without asking. Each agent on a row has its own button. |

Disconnect asks first. The dialog names each window whose panels for the context will close and
each agent that will lose it. Confirm with <kbd>Enter</kbd> or cancel with <kbd>Esc</kbd>.

End agent's hold ends one agent's hold and leaves everyone else's. Click the button beside the
agent you want. With <kbd>E</kbd>, a row with one agent ends that agent's hold at once; when several
agents use it, a dialog lists them, so you choose one. <kbd>Tab</kbd> moves between them,
<kbd>Enter</kbd> ends the focused agent's hold, and <kbd>Esc</kbd> ends nothing. The connection
closes if nothing else holds it, and the agent's next request for that context behaves as if it had never connected it. If
it asks to connect again, you are asked to approve it, as for a new connection.

When an agent connects a context that no window holds, and you approve it, the Connections window
opens with that row selected.

*Summarizes the capability specs: `cluster-connection`, `cluster-picker`, `connections-window`.*
