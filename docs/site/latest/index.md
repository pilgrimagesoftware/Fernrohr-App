---
title: Fernrohr documentation
version: latest
---

<img class="landing-icon" src="{{ '/assets/icons/fernrohr-icon.png' | relative_url }}" alt="Fernrohr application icon">

# Fernrohr documentation

Fernrohr is a native desktop client for Kubernetes, built for the keyboard. It
shows several clusters side by side in docked panels, follows pod logs, opens
shells in containers, and reaches private clusters through SSH and command
tunnels. Coding agents can work with the clusters you have open through
Fernrohr's MCP server.

This is the documentation for the latest release. To read about an earlier
build, pick its release in the version selector.

Every action is a command, so the command palette (<kbd>⌘⇧P</kbd>) lists all of
them with their shortcuts.

## Getting started

- [Install Fernrohr]({{ '/latest/install/' | relative_url }}) on macOS or Linux,
  including the first-launch gotchas on each platform.
- [Connect to clusters]({{ '/latest/clusters/' | relative_url }}) from your
  kubeconfig with the cluster picker.

## Using Fernrohr

- [Windows, panels and layouts]({{ '/latest/windows-panels/' | relative_url }}):
  arrange panels and tabs, use the menu and palette, and save layouts.
- [Browse resources]({{ '/latest/resources/' | relative_url }}): resource lists,
  groups, namespaces and events.
- [Inspect resource details]({{ '/latest/details/' | relative_url }}): detail
  views and links between related resources.
- [Logs and shells]({{ '/latest/logs-shells/' | relative_url }}): follow pod logs
  and open a shell in a container.
- [Tunnels and port forwards]({{ '/latest/tunnels-forwards/' | relative_url }}):
  reach private clusters and forward ports.
- [Work with coding agents]({{ '/latest/agents/' | relative_url }}): connect an
  agent to Fernrohr's MCP server and approve what it does.
- [Settings and app windows]({{ '/latest/settings/' | relative_url }}): text
  size, theme, notifications, and the About and Report Issue windows.

## Reference

- [Keybindings and commands]({{ '/latest/reference/keybindings/' | relative_url }})
  lists every command with its default shortcut.
- [MCP tools]({{ '/latest/reference/mcp-tools/' | relative_url }}) lists every
  tool an agent can call.

*This site follows the capability specs: `documentation-authoring`,
`documentation-publishing`, `documentation-versioning`.*
