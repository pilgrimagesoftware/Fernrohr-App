---
title: Tunnels and port forwards
version: v0.10
---

# Tunnels and port forwards

A kube context can reach its cluster through a tunnel. You define named tunnels, bind each context
to at most one of them, and Fernrohr brings the tunnel up when the context connects. Separately,
you can forward a port of a pod or service to your machine, and Fernrohr shows those forwards where
you're looking.

For connecting contexts in general, see {{ '/v0.10/clusters/' | relative_url }}.

## Tunnel kinds

A tunnel is one of three kinds, chosen in the tunnel editor.

- **SSH tunnel:** Fernrohr runs `ssh -N -L` through a bastion to the context's API server. The
  tunnel describes only the bastion. The target comes from the context, so you don't enter one.
- **Command tunnel:** Fernrohr runs a command you give it and keeps it running while a bound context
  is connected.
- **Manual tunnel:** Fernrohr starts nothing. You bring the network path up yourself and confirm it.

## Manage tunnels

Open the Tunnels window with **Manage Tunnels…** (<kbd>⌘⇧T</kbd>), in the Context menu and the
command palette. For each tunnel it shows how many contexts use it and whether it is running or
idle. It doesn't bind contexts. That happens on the context ([see below](#bind-a-context-to-a-tunnel)).

- **Create:** choose **New Tunnel**, pick the kind and fill in the form. The tunnel then appears in
  every context's tunnel selector.
- **Edit or rename:** choose **Edit Tunnel**. Context bindings follow a renamed tunnel, and they
  survive a change of kind.
- **Test:** choose **Test** to try a tunnel without connecting any context. Testing an SSH tunnel
  opens an SSH session to the bastion without forwarding a port. Testing a command tunnel runs the
  command until it is ready, then stops it. A failure shows the SSH client's error or the command's
  recent output.
- **Delete:** choose **Delete**. If contexts are bound to the tunnel, the confirmation names them,
  and on confirming they go back to connecting directly.

A tunnel that isn't valid doesn't save, and the field at fault says why.

### SSH tunnel settings

An SSH tunnel has a host (a hostname or an alias from your SSH config), a port, a login user, an
authentication method, an optional list of jump hosts and keepalive settings. Authentication is
either your SSH config and agent, or a private key kept in the operating system keychain. Saving
needs a host, a user and a port from 1 to 65535.

### Command tunnel settings

A command tunnel runs a command, such as a vendor CLI that opens an SSH session through an
identity-aware proxy:

```sh
gcloud compute ssh bastion-host --tunnel-through-iap --project my-project \
    -- -N -L{port}:127.0.0.1:8888
```

`{port}` becomes the local port Fernrohr picks. If the command must use a fixed port, write that
port into the command and set it as the tunnel's local port. Saving is refused when the command has
no `{port}` and no fixed port, or has unbalanced quotes.

A command tunnel has these settings:

| Setting | Meaning |
| --- | --- |
| Command | The command line. You can paste it over several lines with trailing backslashes. |
| Mode | **Proxy** (the default) or **Forward**. |
| Local port | Optional fixed port, 1 to 65535. |
| Startup timeout | Seconds to wait for the local port to accept connections. 30 by default. |

The mode sets what the local port offers:

- **Proxy:** an HTTP proxy, as in the example, where port 8888 on the bastion is a proxy. Bound
  contexts keep their API server address and send their traffic through it. Only the bound
  context's connection uses the proxy. TLS is still checked against the API server's own host name.
- **Forward:** the API server itself. Bound contexts connect to the local port instead, and the
  certificate is still checked against the original API server host.

The command runs without a shell, so pipes and `$VARIABLES` don't apply. Fernrohr looks the tool up
on your login shell's `PATH`, even when the app was opened from the Dock. The command must run
unattended, so run it once in a terminal first to answer any host-key or login prompts.

All contexts bound to a command tunnel share one running command, whatever their API servers.
**Test** shows the command's output if it fails.

> The command is stored in plain text in `tunnels.toml`. Leave credentials to the tool's own login,
> not flags.
{: .warning}

### Manual tunnel settings

Use a manual tunnel for a cluster you can only reach after bringing up a network path by hand, such
as a VPN from a menu-bar client with no command line. It has two settings:

- **Instruction** (optional): shown in every prompt, for example "Connect the corporate VPN from
  the menu bar".
- **When the API server already answers:** **Skip the prompt** (the default) makes Fernrohr try a
  quick connection to the context's API server before asking, and go straight through if it
  answers. Choose **Always prompt** for an API server that answers without your VPN too.

### Where settings and secrets are stored

Tunnels are saved in `tunnels.toml` in the app's state folder, with no secret in it. Passwords and
key passphrases live in the operating system keychain, keyed by the tunnel, and are read from it
when the tunnel starts. Deleting a tunnel removes its keychain entry. If the system has no keychain
or Secret Service available, Fernrohr asks for the secret for the current session instead of
failing.

A file written by an earlier version still loads. A tunnel with no kind loads as an SSH tunnel, and
old forward-target fields are ignored and dropped the next time the file is saved.

A short example, with placeholder values:

```toml
[tunnels.my-bastion]
name = "My bastion"
kind = "ssh"
bastion_user = "ops"
bastion_host = "bastion.example.com"
bastion_port = 22
auth = "ssh_config"

[tunnels.my-iap]
name = "My IAP tunnel"
kind = "command"

[tunnels.my-iap.command]
command_line = "gcloud compute ssh bastion-host --tunnel-through-iap --project my-project -- -N -L{port}:127.0.0.1:8888"
mode = "proxy"
startup_timeout_secs = 30

[context_bindings]
my-cluster = "my-bastion"
```

You normally edit tunnels in the Tunnels window, not in the file.

## Bind a context to a tunnel

A context connects directly, or through exactly one tunnel. Many contexts can share a tunnel.

- **Keyboard:** run **Set Tunnel for Context** (<kbd>⌘⇧B</kbd>, in the Context menu and the command
  palette) and pick a tunnel, or Direct to unbind.
- **Mouse:** use the tunnel selector on the context's row in the cluster picker.

The picker shows each bound context's tunnel name, and says that an unbound context connects
directly. The binding is saved across restarts. A change applies at the next connection and leaves
a live connection alone.

When a bound context connects, Fernrohr starts its tunnel, shows a waiting-for-tunnel state, and
builds the connection once the tunnel is up. If the tunnel can't start, the connection fails with
the tunnel's reason. When the last connected context using a tunnel disconnects, the tunnel stops.

How the connection is made depends on the kind:

- **SSH tunnel:** Fernrohr forwards to the host and port of the context's API server, using the
  scheme's default port when the URL has none. Contexts with the same API server share one forward,
  and each different API server gets its own. The client connects to a local loopback port and
  still validates the server certificate against the original API server host.
- **Command tunnel:** one command serves every bound context, as described above.
- **Manual tunnel:** once confirmed, the context connects directly to its API server.

If a bound context disappears from your kubeconfig, for example because it was renamed, the Tunnels
window lists the old name as a stale binding, and you can remove it there.

### Confirm a manual tunnel

When a context bound to a manual tunnel connects, Fernrohr holds the connection and asks you to
bring the path up, then waits for your answer with no timeout. It asks in three places:

- a desktop notification;
- the waiting context's capsule in the status bar, drawn in the attention color, with **Proceed**
  and **Cancel** in its menu above Disconnect;
- the cluster picker's status line while it is connecting that context.

Answer with the mouse from the capsule menu or the picker, or from the keyboard with **Proceed with
Manual Tunnel** (<kbd>⌘⌥P</kbd>, <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>P</kbd> off macOS) and **Cancel
Manual Tunnel** (<kbd>⌘⌥C</kbd>, <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>C</kbd>). Both are in the
command palette and can be rebound. When several tunnels are waiting, the command asks which one.

- **Proceed** connects every context waiting on the tunnel. If the API server is still unreachable,
  the connection fails with the usual error and the next attempt asks again.
- **Cancel** fails them, saying you cancelled it. Fernrohr never connects them directly instead.

Every context bound to the same manual tunnel shares one prompt. A context that connects while the
tunnel is already confirmed and in use goes straight through. Once the last context using it
disconnects, or a connection through it fails, the next connection asks again. Other contexts keep
working while one waits.

> On macOS, a development build run with `cargo run` has no app bundle, so the system may not show
> its notifications. The status bar and the picker still ask.

## Port forwards

A port forward makes a pod's or service's port reachable on `127.0.0.1` on your machine. Fernrohr
supervises it: it picks a free local port unless you ask for one, checks that it's still carrying
traffic, and reconnects with backoff, keeping the same port, if the connection drops.

### Start a forward

- **Keyboard:** select a pod and press <kbd>⇧F</kbd> (**Pods: Port-Forward Selected Pod** in the
  Pods panel, **Pod Detail: Port-Forward Pod** in its detail panel, **Services: Port-Forward
  Selected Service** in the Services list). It asks for the port when there are several.
- **Mouse:** in the pod detail panel's Containers tab, click the forward icon next to a declared
  container port. The forward starts with no port prompt.

A forward that can't start shows a notification naming the pod, the port and the reason, for
example when the pod declares no container ports. Success shows only through the indicators below.

### Forward indicators

- **Forwards column:** the Pods list and the Services list have a Forwards column. A row with
  active forwards shows a forward icon with the number of forwards. Hover it to see each one as
  `127.0.0.1:<local port> → <target port>`. The column updates as forwards start and stop.
- **Detail panel strip:** while a pod has active forwards, its detail panel shows a strip above the
  tabs with each forward's local address and target port. Each has a button that copies the local
  address and a button that stops the forward. The strip disappears when the last forward stops.
- **Containers tab:** a port being forwarded shows its local address with copy and stop buttons in
  place of the forward icon.

### Stop a forward

- **Keyboard:** press <kbd>⌃⇧F</kbd> on a pod or service (**Stop Port Forward**). It asks which
  forward when there are several.
- **Mouse:** click the stop icon in the detail strip or the Containers tab, or the **Stop** button
  in Manage Tunnels.

Every route asks for confirmation first. The confirmation names the pod or service, the local
address and the target port. <kbd>⏎</kbd> stops the forward, <kbd>Esc</kbd> cancels and leaves it
running, and <kbd>Tab</kbd> moves between the buttons.

*Summarizes the capability specs: `tunnel-config`, `manual-tunnel`, `context-tunnel-binding`, `managed-forward`, `port-forward-indicators`.*
