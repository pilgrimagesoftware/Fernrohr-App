---
title: Fernrohr v0.10 documentation
version: v0.10
---

<img class="landing-logo" src="{{ '/assets/images/fernrohr-logo.png' | relative_url }}" alt="Fernrohr" width="320">

# Fernrohr v0.10 documentation

Fernrohr is a native desktop client for Kubernetes, built for the keyboard. It
shows several clusters side by side in docked panels, follows pod logs, opens
shells in containers, and reaches private clusters through SSH and command
tunnels. Coding agents can work with the clusters you have open through
Fernrohr's MCP server.

This is the documentation for the Fernrohr v0.10 release. For the current
release or an earlier one, pick it in the version selector.

Every action is a command, so the command palette (<kbd>⌘⇧P</kbd>) lists all of
them with their shortcuts.

## Getting started

- [Install Fernrohr]({{ '/v0.10/install/' | relative_url }}) on macOS or Linux,
  including the first-launch gotchas on each platform.
- [Connect to clusters]({{ '/v0.10/clusters/' | relative_url }}) from your
  kubeconfig with the cluster picker.

## Using Fernrohr

- [Windows, panels and layouts]({{ '/v0.10/windows-panels/' | relative_url }}):
  arrange panels and tabs, use the menu and palette, and save layouts.
- [Browse resources]({{ '/v0.10/resources/' | relative_url }}): resource lists,
  groups, namespaces and events.
- [Inspect resource details]({{ '/v0.10/details/' | relative_url }}): detail
  views and links between related resources.
- [Logs and shells]({{ '/v0.10/logs-shells/' | relative_url }}): follow pod logs
  and open a shell in a container.
- [Tunnels and port forwards]({{ '/v0.10/tunnels-forwards/' | relative_url }}):
  reach private clusters and forward ports.
- [Work with coding agents]({{ '/v0.10/agents/' | relative_url }}): connect an
  agent to Fernrohr's MCP server and approve what it does.
- [Settings and app windows]({{ '/v0.10/settings/' | relative_url }}): text
  size, theme, notifications, and the About and Report Issue windows.

## Reference

- [Keybindings and commands]({{ '/v0.10/reference/keybindings/' | relative_url }})
  lists every command with its default shortcut.
- [MCP tools]({{ '/v0.10/reference/mcp-tools/' | relative_url }}) lists every
  tool an agent can call.

*This site follows the capability specs: `documentation-authoring`,
`documentation-publishing`, `documentation-versioning`.*
