---
title: Connect to clusters
version: v0.13
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
[Tunnels and forwards]({{ '/v0.13/tunnels-forwards/' | relative_url }}).

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
[Report an error from where it happened]({{ '/v0.13/settings/' | relative_url }}#report-an-error-from-where-it-happened).

To retry, press <kbd>Enter</kbd> or click Connect again, on the same context or another. Choosing
a different context needs no reset.

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
that the context stays connected if other windows still use it. Confirm with <kbd>Enter</kbd> or
by clicking Disconnect, or cancel with <kbd>Esc</kbd>.

Connections are shared between windows. Two windows using the same context share one connection,
so disconnecting in one leaves the other receiving live updates. The connection, and any tunnel it
holds, shuts down when the last window using the context disconnects or closes.

*Summarizes the capability specs: `cluster-connection`, `cluster-picker`.*
