---
title: Work with coding agents
version: latest
---

# Work with coding agents

Fernrohr is an [MCP](https://modelcontextprotocol.io) server, so a coding agent such as Claude Code can use the clusters you have open in the app. The agent works through Fernrohr's own connections, tunnels and permissions. It never sees your kubeconfig, credentials or tunnel secrets, and it can only reach a context that a Fernrohr window has open, or one you let it connect.

Fernrohr has to be running for the agent to get an answer. It doesn't start the app on the agent's behalf. If the app isn't running, the agent gets an "unavailable" error and no window opens.

## Set up an agent

Open Settings and go to Agent Access. It shows a ready-to-run command for Claude Code, Codex, Gemini CLI and OpenCode, each with a copy button. From the keyboard, press <kbd>⌘⇧P</kbd> to open the command palette, run **Copy MCP Setup Command** and pick the agent.

Each command registers Fernrohr for your user, so it works in every project. It runs this installation's `fernrohr mcp` by its full path:

```sh
claude mcp add --scope user fernrohr -- '/path/to/fernrohr' mcp
codex mcp add fernrohr -- '/path/to/fernrohr' mcp
gemini mcp add --scope user fernrohr '/path/to/fernrohr' mcp
opencode mcp add fernrohr --global -- '/path/to/fernrohr' mcp
```

Older OpenCode releases lack `mcp add`, so the section also offers a config entry to paste into `opencode.json`. Put it under `mcp.fernrohr` in OpenCode 1.x, or under `mcp.servers.fernrohr` in 2.x:

```json
{
  "type": "local",
  "command": ["/path/to/fernrohr", "mcp"]
}
```

Copy the command from the running app rather than typing it, because the path in it is the one the app is running from. If that path won't last, the section explains why instead of offering a command:

- On macOS, an app opened straight from Downloads or a disk image runs from a temporary copy. Move Fernrohr to Applications and open it from there.
- A development build runs from Cargo's `target` directory, which the next build or `cargo clean` replaces. Install Fernrohr and open the installed app.

The connection between `fernrohr mcp` and the app is a Unix-domain socket in your per-user runtime directory, readable only by you and checked against a token that changes every launch. It is never a network port. Windows has no agent access yet, and the section says so.

To see what the command does, ask it for help. Neither of these opens the app or touches a running one:

```sh
'/path/to/fernrohr' --help
'/path/to/fernrohr' mcp --help
```

`fernrohr mcp` ends when the agent's client closes it, stops it, or exits, so it doesn't stay running after the agent session. Any argument Fernrohr doesn't recognize prints an error and the usage instead of starting the app.

## What an agent can do

An agent can only use the clusters Fernrohr already has open, through the same sessions the app uses. It never gets Kubernetes credentials, kubeconfig contents, authorization headers or tunnel secrets, and error messages are cleaned of them.

- **Read.** List contexts and whether each is connected, list a context's resource kinds, list and get resources, and read a pod's logs. Secret values come back as their sizes, never their contents. Large results are cut short and say so.
- **Navigate.** Open or focus a resource list or an object's detail panel in the window holding its context, list your saved layouts, and load one in Add or Replace mode. A layout panel for a context the window isn't connected to comes back as a placeholder, and loading a layout never connects a context. Opening a panel or loading a layout brings Fernrohr to the front with that window focused, so you see what the agent is showing you. Reads and listing layouts never take focus.
- **Connect.** Connect a context from your kubeconfig, only with your approval. Fernrohr adds it to the frontmost window the way the status bar's add-context control does, or opens a window for it when none is open, and brings that window to the front. The agent is told within 20 seconds whether the context connected, is waiting for its tunnel, is waiting for you to confirm a manual tunnel, or failed. Otherwise it is told the context is still connecting. A context the window already has is brought to the front without asking.
- **Act**, only with your approval, on this fixed list: set or remove one ConfigMap key, scale a Deployment, StatefulSet or ReplicaSet, restart or roll back a Deployment, StatefulSet or DaemonSet, pause or resume a Deployment's rollout, delete 1 to 10 named Pods, trigger a Job from a CronJob, and suspend or resume a CronJob.

The agent can't create, edit, patch or delete anything else, can't edit resource YAML or call raw Kubernetes API paths, and can't write Secrets. A request outside the list, such as scaling a DaemonSet or deleting 11 Pods in one call, is rejected without asking you.

The [MCP tools reference]({{ '/latest/reference/mcp-tools/' | relative_url }}) lists every tool with its parameters and results.

## Approve actions and connections

Every action opens a confirmation in Fernrohr. It names the action, context, namespace, kind and every object the action touches, with the values it changes, such as the replica count before and after, a ConfigMap key's old and new value, or the revision a rollback returns to. To show those, the action reads the objects first. Nothing is written until you allow it.

- Rolling back and deleting Pods can't be undone, so their confirmation opens on Cancel and <kbd>Enter</kbd> cancels. Allow them by clicking the button, by tabbing to it, or with <kbd>⌘⌫</kbd> (<kbd>Ctrl</kbd><kbd>Backspace</kbd> off macOS). Other actions can be allowed with <kbd>Enter</kbd>.
- <kbd>Esc</kbd> denies. A request you don't answer within 2 minutes is denied, and its dialog closes. It also closes if the agent stops waiting.
- The agent is told whether you allowed the action, denied it, or it timed out. A denied or timed-out action sends nothing to the cluster.
- Requests are asked one at a time. A second waits until you answer the first.

Connecting a context asks too. The prompt names the context and the tunnel it is bound to, with the tunnel's kind, and says the agent will be able to read it. Connecting changes nothing in the cluster and disconnecting undoes it, so <kbd>Enter</kbd> allows it. A denied or unanswered request connects nothing and starts no tunnel or sign-in.

Reads and navigation need no approval.

*Summarizes the capability specs: `agent-mcp`.*
