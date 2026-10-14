---
title: Keybindings and commands
version: latest
---

# Keybindings and commands

Every action in Fernrohr is a command. The command palette, opened with <kbd>⌘⇧P</kbd> or **View > Command Palette**, lists the commands that apply where focus is. Type to filter the list, press <kbd>↵</kbd> or click to run one, and see each command's current key beside its title. A command with no key is still in the palette. You can change any command's key in Settings, under Keyboard Shortcuts, or in `keymap.toml`.

This list matches the commands registered in `app/src/command.rs`.

## How to read the tables

- Keys are written with macOS symbols: <kbd>⌘</kbd> Command, <kbd>⇧</kbd> Shift, <kbd>⌥</kbd> Option, <kbd>⌃</kbd> Control. On Linux, the primary modifier is Control rather than Command.
- A shortcut of two keys is written as two groups, such as <kbd>⌘K</kbd> <kbd>←</kbd>. Press the first, release it, then press the second.
- **Context** is where the command applies. `Global` works whichever panel has focus. Anything else is a key context, and the command is only available, and only in the palette, while that panel has focus. A context ending in `!Input` doesn't apply while you're typing in a text field, so a single letter such as <kbd>E</kbd> is still just a letter there.
- **Menu** is the application menu item for the command. A dash means the command has no menu item. Panel-specific commands stay out of the menu bar, so it never depends on which panel has focus.
- **ID** is the stable name of the command, which `keymap.toml` uses.
- A dash in the binding column means the command has no default key.

## Commands

### Application, windows and appearance

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| New Window | <kbd>⌘N</kbd> | Global | Window | `shell.new_window` |
| Command Palette | <kbd>⌘⇧P</kbd> | Global | View | `shell.toggle_command_palette` |
| About Fernrohr | — | Global | — | `app.about` |
| Hide Fernrohr | <kbd>⌘H</kbd> | Global | — | `app.hide` |
| Hide Others | <kbd>⌥⌘H</kbd> | Global | — | `app.hide_others` |
| Show All | — | Global | — | `app.show_all` |
| Quit Fernrohr | <kbd>⌘Q</kbd> | Global | — | `app.quit` |
| Minimize | <kbd>⌘M</kbd> | Global | — | `window.minimize` |
| Zoom | — | Global | — | `window.zoom` |
| Close Window | <kbd>⌘W</kbd> | Global | — | `window.close` |
| Theme: Follow System | — | Global | View | `theme.system` |
| Theme: Light | — | Global | View | `theme.light` |
| Theme: Dark | — | Global | View | `theme.dark` |
| Settings… | <kbd>⌘,</kbd> | Global | Fernrohr | `settings.open` |
| Increase Text Size | <kbd>⌘=</kbd> | Global | View | `view.increase_text_size` |
| Decrease Text Size | <kbd>⌘-</kbd> | Global | View | `view.decrease_text_size` |
| Reset Text Size | <kbd>⌘⇧0</kbd> | Global | View | `view.reset_text_size` |
| Increase Shortcut Timeout | — | Global | — | `settings.increase_shortcut_timeout` |
| Decrease Shortcut Timeout | — | Global | — | `settings.decrease_shortcut_timeout` |
| Reset Shortcut Timeout | — | Global | — | `settings.reset_shortcut_timeout` |
| Open Each Pod's Logs in Its Own Panel | — | Global | — | `settings.logs_per_pod` |
| Open Every Pod's Logs in One Reused Panel | — | Global | — | `settings.logs_reuse_panel` |
| Report Issue | <kbd>⌘⇧/</kbd> | Global | Help | `help.report_issue` |
| Copy MCP Setup Command | — | Global | — | `mcp.copy_setup_command` |

### Contexts, tunnels and namespace sets

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Set Tunnel for Context | <kbd>⌘⇧B</kbd> | Global | Context | `context.set_tunnel` |
| Manage Tunnels… | <kbd>⌘⇧T</kbd> | Global | Context | `tunnels.manage` |
| Add Context to Window… | — | Global | Context | `context.add` |
| Disconnect Active Context… | — | Global | Context | `context.disconnect` |
| Add Context: Close | <kbd>⎋</kbd> | `AddContextWindow` | — | `add_context.close` |
| Switch Namespace Set… | <kbd>⌘⇧N</kbd> | Global | — | `namespace_sets.switch` |
| Apply Namespace Set to Context… | <kbd>⌥⌘⇧N</kbd> | Global | — | `namespace_sets.switch_context` |
| Create Namespace Set… | <kbd>⌥⌘N</kbd> | Global | — | `namespace_sets.create_set` |
| Edit Namespace Set… | <kbd>⌥⌘E</kbd> | Global | — | `namespace_sets.edit_set` |
| Remove Namespace Set… | — | Global | — | `namespace_sets.remove_set` |
| Proceed with Manual Tunnel | <kbd>⌥⌘P</kbd> | Global | Context | `tunnel.manual.proceed` |
| Cancel Manual Tunnel | <kbd>⌥⌘C</kbd> | Global | Context | `tunnel.manual.cancel` |

### Navigation, tabs, layouts and the Resource panel

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Save Panel Layout… | <kbd>⌘⇧S</kbd> | Global | Window | `layouts.save` |
| Saved Layouts… | <kbd>⌘⇧O</kbd> | Global | Window | `layouts.manage` |
| Show Pods | <kbd>⌘1</kbd> | Global | — | `nav.show_pods` |
| Show Logs | <kbd>⌘2</kbd> | Global | — | `nav.show_logs` |
| Show Logs (Other Panel Mode) | — | Global | — | `nav.show_logs_flipped` |
| Show Events | — | Global | — | `nav.show_events` |
| Open Logs by Label… | — | Global | Window | `logs.follow_labels` |
| Fit Columns to Contents | — | Global | View | `table.fit_columns` |
| Focus Resources | <kbd>⌘0</kbd> | Global | Navigate | `resource.focus` |
| Resources: Refresh | <kbd>⌘R</kbd> | Global | View | `resource.refresh` |
| Make Resource Panel's Side the Default | — | Global | View | `resource.save_side` |
| Collapse/Expand Resource Panel | <kbd>⌘B</kbd> | Global | View | `resource.toggle_panel` |
| Move Resource Panel to Other Side | <kbd>⌥⌘B</kbd> | Global | View | `resource.move_panel` |
| Focus Next Panel | <kbd>⌘]</kbd> | Global | Navigate | `panel.focus_next` |
| Focus Previous Panel | <kbd>⌘[</kbd> | Global | Navigate | `panel.focus_previous` |
| Next Tab | <kbd>⌘⇧]</kbd> | Global | Navigate | `tab.next` |
| Previous Tab | <kbd>⌘⇧[</kbd> | Global | Navigate | `tab.previous` |
| Select Tab 1 | <kbd>⌃1</kbd> | Global | Navigate > Select Tab | `tab.select_1` |
| Select Tab 2 | <kbd>⌃2</kbd> | Global | Navigate > Select Tab | `tab.select_2` |
| Select Tab 3 | <kbd>⌃3</kbd> | Global | Navigate > Select Tab | `tab.select_3` |
| Select Tab 4 | <kbd>⌃4</kbd> | Global | Navigate > Select Tab | `tab.select_4` |
| Select Tab 5 | <kbd>⌃5</kbd> | Global | Navigate > Select Tab | `tab.select_5` |
| Select Tab 6 | <kbd>⌃6</kbd> | Global | Navigate > Select Tab | `tab.select_6` |
| Select Tab 7 | <kbd>⌃7</kbd> | Global | Navigate > Select Tab | `tab.select_7` |
| Select Tab 8 | <kbd>⌃8</kbd> | Global | Navigate > Select Tab | `tab.select_8` |
| Select Last Tab | <kbd>⌃9</kbd> | Global | Navigate > Select Tab | `tab.select_9` |
| Open in New Panel | <kbd>⇧⌘D</kbd> | Global | View | `panel.open_new_instance` |
| Maximize/Restore Panel | — | Global | View | `panel.toggle_zoom` |

### Panel groups

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Panels: Split Group Left | <kbd>⌘K</kbd> <kbd>←</kbd> | `Dock && !Input` | — | `panel.split_left` |
| Panels: Split Group Right | <kbd>⌘K</kbd> <kbd>→</kbd> | `Dock && !Input` | — | `panel.split_right` |
| Panels: Split Group Up | <kbd>⌘K</kbd> <kbd>↑</kbd> | `Dock && !Input` | — | `panel.split_up` |
| Panels: Split Group Down | <kbd>⌘K</kbd> <kbd>↓</kbd> | `Dock && !Input` | — | `panel.split_down` |
| Panels: Move Panel Left | <kbd>⌘K</kbd> <kbd>⇧←</kbd> | `Dock && !Input` | — | `panel.move_left` |
| Panels: Move Panel Right | <kbd>⌘K</kbd> <kbd>⇧→</kbd> | `Dock && !Input` | — | `panel.move_right` |
| Panels: Move Panel Up | <kbd>⌘K</kbd> <kbd>⇧↑</kbd> | `Dock && !Input` | — | `panel.move_up` |
| Panels: Move Panel Down | <kbd>⌘K</kbd> <kbd>⇧↓</kbd> | `Dock && !Input` | — | `panel.move_down` |
| Panels: Merge Group Left | <kbd>⌘K</kbd> <kbd>⌥←</kbd> | `Dock && !Input` | — | `panel.merge_left` |
| Panels: Merge Group Right | <kbd>⌘K</kbd> <kbd>⌥→</kbd> | `Dock && !Input` | — | `panel.merge_right` |
| Panels: Merge Group Up | <kbd>⌘K</kbd> <kbd>⌥↑</kbd> | `Dock && !Input` | — | `panel.merge_up` |
| Panels: Merge Group Down | <kbd>⌘K</kbd> <kbd>⌥↓</kbd> | `Dock && !Input` | — | `panel.merge_down` |
| Panels: Close Group | <kbd>⌘K</kbd> <kbd>W</kbd> | `Dock && !Input` | — | `panel.close_group` |

### Resource panel

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Focus Resource Filter | <kbd>/</kbd> | `ResourcePanel` | — | `resource.focus_filter` |
| Open Selected Resource | <kbd>↵</kbd> | `ResourcePanel` | — | `resource.open_selected` |
| Collapse Resource Section | <kbd>←</kbd> | `ResourcePanel` | — | `resource.collapse_section` |
| Expand Resource Section | <kbd>→</kbd> | `ResourcePanel` | — | `resource.expand_section` |
| Toggle Resource Group | <kbd>Space</kbd> | `ResourcePanel && !Input` | — | `resource.toggle_subgroup` |
| Collapse All Resource Groups | <kbd>⇧←</kbd> | `ResourcePanel && !Input` | — | `resource.collapse_all_subgroups` |
| Expand All Resource Groups | <kbd>⇧→</kbd> | `ResourcePanel && !Input` | — | `resource.expand_all_subgroups` |

### Pods panel

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Pods: Close Quick Look | <kbd>⎋</kbd> | `PodQuickLook && !Input` | — | `pods.close_quick_look` |
| Pods: Open Quick Look's Pod Details | <kbd>↵</kbd> | `PodQuickLook && !Input` | — | `pods.quick_look_open_details` |
| Pods: Shell into Selected Pod | <kbd>S</kbd> | `PodShellable && !Input` | — | `pods.shell` |
| Pods: Focus Filter | <kbd>/</kbd> | `PodsPanel && !Input` | — | `pods.focus_filter` |
| Pods: Edit Selected Pod's YAML | <kbd>E</kbd> | `PodsPanel && !Input` | — | `pods.edit` |
| Pods: Quick Look | <kbd>Space</kbd> | `PodsPanel && !Input` | — | `pods.quick_look` |
| Pods: Port-Forward Selected Pod | <kbd>⇧F</kbd> | `PodsPanel && !Input` | — | `pods.port_forward` |
| Pods: Stop Port-Forward of Selected Pod | <kbd>⌃⇧F</kbd> | `PodsPanel && !Input` | — | `pods.stop_port_forward` |
| Pods: Delete Selected Pod | <kbd>⌃D</kbd> | `PodsPanel && !Input` | — | `pods.delete` |
| Pods: Kill Selected Pod (No Grace Period) | <kbd>⌃K</kbd> | `PodsPanel && !Input` | — | `pods.kill` |
| Pods: Filter to Selected Pod's Namespace | <kbd>W</kbd> | `PodsPanel && !Input` | — | `pods.warp_namespace` |
| Pods: Warp All to Selected Pod's Namespace | <kbd>⇧W</kbd> | `PodsPanel && !Input` | — | `pods.warp_all_namespace` |
| Pods: Open Selected Pod in Background | <kbd>⌘↵</kbd> | `PodsPanel && !Input` | — | `pods.open_in_background` |
| Pods: Open Selected Pod in New Panel | <kbd>⇧↵</kbd> | `PodsPanel && !Input` | — | `pods.open_new_panel` |
| Pods: Describe Selected Pod | <kbd>D</kbd> | `PodsPanel && !Input` | — | `pods.describe` |
| Pods: Show Selected Pod's Logs | <kbd>L</kbd> | `PodsPanel && !Input` | — | `pods.logs` |
| Pods: Show Selected Pod's Logs (Other Panel Mode) | <kbd>⇧L</kbd> | `PodsPanel && !Input` | — | `pods.logs_flipped` |
| Pods: Show Selected Pod's YAML | <kbd>Y</kbd> | `PodsPanel && !Input` | — | `pods.yaml` |
| Pods: Fit Columns to Contents | <kbd>=</kbd> | `PodsPanel && !Input` | — | `pods.fit_columns` |
| Pods: Pick Namespaces | <kbd>N</kbd> | `PodsPanel && !Input` | — | `pods.pick_namespaces` |

### Pod detail panel

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Pod Detail: Toggle Fields/YAML | <kbd>Y</kbd> | `PodDetailPanel` | — | `pod_detail.toggle_view` |
| Pod Detail: Overview Tab | <kbd>1</kbd> | `PodDetailPanel` | — | `pod_detail.tab_overview` |
| Pod Detail: Containers Tab | <kbd>2</kbd> | `PodDetailPanel` | — | `pod_detail.tab_containers` |
| Pod Detail: Configuration Tab | <kbd>3</kbd> | `PodDetailPanel` | — | `pod_detail.tab_configuration` |
| Pod Detail: Volumes Tab | <kbd>4</kbd> | `PodDetailPanel` | — | `pod_detail.tab_volumes` |
| Pod Detail: Events Tab | <kbd>5</kbd> | `PodDetailPanel` | — | `pod_detail.tab_events` |
| Pod Detail: Managed Fields Tab | <kbd>6</kbd> | `PodDetailPanel` | — | `pod_detail.tab_managed_fields` |
| Pod Detail: Hide Secret Values | <kbd>H</kbd> | `PodDetailPanel` | — | `pod_detail.hide_secret_values` |
| Pod Detail: Fold All YAML | <kbd>Z</kbd> | `PodDetailPanel` | — | `pod_detail.yaml_fold_all` |
| Pod Detail: Unfold All YAML | <kbd>⇧Z</kbd> | `PodDetailPanel` | — | `pod_detail.yaml_unfold_all` |
| Pod Detail: Copy YAML | <kbd>⇧C</kbd> | `PodDetailPanel` | — | `pod_detail.copy_yaml` |
| Pod Detail: Copy Resource Name | <kbd>C</kbd> | `PodDetailPanel` | — | `pod_detail.copy_name` |
| Pod Detail: View Logs | <kbd>L</kbd> | `PodDetailPanel && !Input` | — | `pod_detail.view_logs` |
| Pod Detail: View Logs (Other Panel Mode) | <kbd>⇧L</kbd> | `PodDetailPanel && !Input` | — | `pod_detail.view_logs_flipped` |
| Pod Detail: Edit YAML | <kbd>E</kbd> | `PodDetailPanel && !Input` | — | `pod_detail.edit` |
| Pod Detail: Delete Pod | <kbd>⌃D</kbd> | `DeletablePod && !Input` | — | `pod_detail.delete` |
| Pod Detail: Kill Pod (No Grace Period) | <kbd>⌃K</kbd> | `DeletablePod && !Input` | — | `pod_detail.kill` |
| Pod Detail: Shell into Pod | <kbd>S</kbd> | `PodDetailShellable && !Input` | — | `pod_detail.shell` |
| Pod Detail: Port-Forward Pod | <kbd>⇧F</kbd> | `PodDetailPanel && !Input` | — | `pod_detail.port_forward` |
| Pod Detail: Stop Port-Forward | <kbd>⌃⇧F</kbd> | `PodDetailPanel && !Input` | — | `pod_detail.stop_port_forward` |
| Pod Detail: Events Window - Shorter | <kbd>[</kbd> | `PodDetailPanel` | — | `pod_detail.events_window_shorter` |
| Pod Detail: Events Window - Longer | <kbd>]</kbd> | `PodDetailPanel` | — | `pod_detail.events_window_longer` |
| Pod Detail: Events Window - 15 minutes | — | `PodDetailPanel` | — | `pod_detail.events_window_15m` |
| Pod Detail: Events Window - 1 hour | — | `PodDetailPanel` | — | `pod_detail.events_window_1h` |
| Pod Detail: Events Window - 6 hours | — | `PodDetailPanel` | — | `pod_detail.events_window_6h` |
| Pod Detail: Events Window - 24 hours | — | `PodDetailPanel` | — | `pod_detail.events_window_24h` |
| Pod Detail: Events Window - All | — | `PodDetailPanel` | — | `pod_detail.events_window_all` |

### Logs panels

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Logs: Toggle Previous Container Logs | <kbd>P</kbd> | `LogsPanel && !Input` | — | `logs.toggle_previous` |
| Logs: Toggle Follow | <kbd>S</kbd> | `LogsPanel && !Input` | — | `logs.toggle_follow` |
| Logs: Next Container Filter | <kbd>C</kbd> | `LabelLogs && !Input` | — | `logs.next_container` |
| Logs: Pick Namespaces | <kbd>N</kbd> | `TypedLabelLogs && !Input` | — | `logs.pick_namespaces` |
| Logs: Edit Label Selector | <kbd>/</kbd> | `TypedLabelLogs && !Input` | — | `logs.focus_selector` |
| Logs: Pick Context… | <kbd>X</kbd> | `TypedLabelLogs && !Input` | — | `logs.pick_context` |

### Object lists

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Namespaces: Show All | <kbd>⌥0</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_all` |
| Namespaces: Jump to 1st | <kbd>⌥1</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_1` |
| Namespaces: Jump to 2nd | <kbd>⌥2</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_2` |
| Namespaces: Jump to 3rd | <kbd>⌥3</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_3` |
| Namespaces: Jump to 4th | <kbd>⌥4</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_4` |
| Namespaces: Jump to 5th | <kbd>⌥5</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_5` |
| Namespaces: Jump to 6th | <kbd>⌥6</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_6` |
| Namespaces: Jump to 7th | <kbd>⌥7</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_7` |
| Namespaces: Jump to 8th | <kbd>⌥8</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_8` |
| Namespaces: Jump to 9th | <kbd>⌥9</kbd> | `NamespacedList && !Input` | — | `namespaces.jump_9` |
| List: Edit Selected Object's YAML | <kbd>E</kbd> | `EditableList && !Input` | — | `object_list.edit` |
| List: Delete Selected Object | <kbd>⌃D</kbd> | `DeletableList && !Input` | — | `object_list.delete` |
| List: Focus Filter | <kbd>/</kbd> | `ObjectListPanel && !Input` | — | `object_list.focus_filter` |
| List: Open Selected Object | <kbd>↵</kbd> | `ObjectListPanel && !Input` | — | `object_list.open` |
| List: Open Selected Object in Background | <kbd>⌘↵</kbd> | `ObjectListPanel && !Input` | — | `object_list.open_in_background` |
| List: Open Selected Object in New Panel | <kbd>⇧↵</kbd> | `ObjectListPanel && !Input` | — | `object_list.open_new_panel` |
| List: Describe Selected Object | <kbd>D</kbd> | `ObjectListPanel && !Input` | — | `object_list.describe` |
| List: Show Selected Object's YAML | <kbd>Y</kbd> | `ObjectListPanel && !Input` | — | `object_list.yaml` |
| List: Filter to Selected Object's Namespace | <kbd>W</kbd> | `ObjectListPanel && !Input` | — | `object_list.warp_namespace` |
| List: Fit Columns to Contents | <kbd>=</kbd> | `ObjectListPanel && !Input` | — | `object_list.fit_columns` |
| List: Refresh | <kbd>R</kbd> | `ObjectListPanel && !Input` | — | `object_list.refresh` |
| List: Pick Namespaces | <kbd>N</kbd> | `ObjectListPanel && !Input` | — | `object_list.pick_namespaces` |
| Sort by Next Column | <kbd>⇧.</kbd> | `SortableList && !Input` | — | `list.sort_next_column` |
| Sort by Previous Column | <kbd>⇧,</kbd> | `SortableList && !Input` | — | `list.sort_previous_column` |
| Cycle Sort | <kbd>O</kbd> | `SortableList && !Input` | — | `list.cycle_sort` |

### Object detail panel

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Object Detail: Toggle Fields/YAML | <kbd>Y</kbd> | `ObjectDetailPanel && !Input` | — | `object_detail.toggle_view` |
| Object Detail: Edit YAML | <kbd>E</kbd> | `EditableObject && !Input` | — | `object_detail.edit` |
| Object Detail: Save YAML Edit | <kbd>⌘S</kbd> | `ObjectYamlEdit` | — | `object_detail.save_edit` |
| Object Detail: Cancel YAML Edit | <kbd>⎋</kbd> | `ObjectYamlEdit` | — | `object_detail.cancel_edit` |
| Object Detail: Delete Object | <kbd>⌃D</kbd> | `DeletableObject && !Input` | — | `object_detail.delete` |
| Object Detail: Logs of Its Pods | <kbd>L</kbd> | `PodSelectingObject && !Input` | — | `object_detail.logs` |
| Object Detail: Hide Secret Values | <kbd>H</kbd> | `ObjectDetailPanel && !Input` | — | `object_detail.hide_secret_values` |
| Object Detail: Copy Resource Name | <kbd>C</kbd> | `ObjectDetailPanel && !Input` | — | `object_detail.copy_name` |
| Object Detail: Fold All YAML | <kbd>Z</kbd> | `ObjectDetailPanel && !Input` | — | `object_detail.yaml_fold_all` |
| Object Detail: Unfold All YAML | <kbd>⇧Z</kbd> | `ObjectDetailPanel && !Input` | — | `object_detail.yaml_unfold_all` |
| Object Detail: Copy YAML | <kbd>⇧C</kbd> | `ObjectDetailPanel && !Input` | — | `object_detail.copy_yaml` |
| Node Pods: Open Selected Pod's Detail | <kbd>↵</kbd> | `NodePodsTable && !Input` | — | `object_detail.node_pods.open_detail` |
| Services: Port-Forward Selected Service | <kbd>⇧F</kbd> | `ServicesList && !Input` | — | `services.port_forward` |
| Services: Stop Port-Forward of Selected Service | <kbd>⌃⇧F</kbd> | `ServicesList && !Input` | — | `services.stop_port_forward` |

### Events and placeholder panels

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Events: Open Involved Object | <kbd>↵</kbd> | `EventsPanel && !Input` | — | `events.open_involved` |
| Events: Search | <kbd>/</kbd> | `EventsPanel && !Input` | — | `events.search` |
| Events: Filter by Type | <kbd>T</kbd> | `EventsPanel && !Input` | — | `events.filter_type` |
| Events: Filter by Object Kind | <kbd>K</kbd> | `EventsPanel && !Input` | — | `events.filter_kind` |
| Events: Filter by Reason | <kbd>R</kbd> | `EventsPanel && !Input` | — | `events.filter_reason` |
| Events: Clear Filters | <kbd>X</kbd> | `EventsPanel && !Input` | — | `events.clear_filters` |
| Events: Pick Namespaces | <kbd>N</kbd> | `EventsPanel` | — | `events.pick_namespaces` |
| Placeholder: Pick Namespaces | <kbd>N</kbd> | `PlaceholderPanel` | — | `placeholder.pick_namespaces` |

### Saved layouts picker

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Rename | <kbd>R</kbd> | `SavedLayoutsPicker && !Input` | — | `saved_layouts.rename_selected` |
| Delete | <kbd>⌫</kbd> | `SavedLayoutsPicker && !Input` | — | `saved_layouts.delete_selected` |
| Load (Replace) | <kbd>↵</kbd> | `SavedLayoutsPicker && !Input` | — | `saved_layouts.load_replace` |
| Load (Add) | <kbd>⌘↵</kbd> | `SavedLayoutsPicker && !Input` | — | `saved_layouts.load_add` |

### Settings window and keyboard shortcuts editor

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Settings: Show Keyboard Shortcuts | — | `SettingsWindow` | — | `settings.show_shortcuts` |
| Settings: Show Appearance | — | `SettingsWindow` | — | `settings.show_appearance` |
| Settings: Show Panels | — | `SettingsWindow` | — | `settings.show_panels` |
| Settings: Show Layouts | — | `SettingsWindow` | — | `settings.show_layouts` |
| Settings: Show Agent Access | — | `SettingsWindow` | — | `settings.show_agent_access` |
| Record Shortcut | <kbd>↵</kbd> | `KeyboardShortcuts` | — | `settings.shortcuts.record` |
| Reset Shortcut to Default | <kbd>⌘⌫</kbd> | `KeyboardShortcuts` | — | `settings.shortcuts.reset` |
| Remove Shortcut | <kbd>⌫</kbd> | `KeyboardShortcuts` | — | `settings.shortcuts.remove` |
| Filter Shortcuts | <kbd>/</kbd> | `KeyboardShortcuts` | — | `settings.shortcuts.filter` |

### Report Issue window

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Report Issue: Report | <kbd>⌘↵</kbd> | `ReportIssueWindow` | — | `report_issue.send` |
| Report Issue: Close | <kbd>⎋</kbd> | `ReportIssueWindow` | — | `report_issue.close` |
| Report Issue: Add Screenshots… | <kbd>⌘⇧A</kbd> | `ReportIssueWindow` | — | `report_issue.add_screenshots` |

### Dialogs, key hints and links

| Command | Default binding | Context | Menu | ID |
| --- | --- | --- | --- | --- |
| Show Key Hints | <kbd>?</kbd> | `!Input` | — | `global.show_key_hints` |
| Go to Reference… | <kbd>G</kbd> | `ReferenceLinks` | — | `links.go_to` |
| Confirm Irreversible Action | <kbd>⌘⌫</kbd> | `Dialog` | — | `dialog.confirm_irreversible` |
## Shortcuts of two keys

Some shortcuts take two keys, such as <kbd>⌘K</kbd> <kbd>←</kbd> to split a panel group. After the first key, the status bar shows the keys so far, such as `⌘K …`. Above it, it lists the keys that would finish a shortcut there, each with what it does. Press one of those keys to run it. Any other key, or a change of focus, cancels the shortcut.

If the keys so far are a shortcut on their own as well as the start of a longer one, Fernrohr waits for the next key before it runs the shorter shortcut. The wait is the shortcut timeout, from 1 to 10 whole seconds, and 3 seconds by default. Change it in Settings, or with **Increase Shortcut Timeout**, **Decrease Shortcut Timeout** and **Reset Shortcut Timeout** from the palette. Or set `shortcut_timeout_secs` in `ui.toml`. A value outside the range loads as the nearest allowed value, and the rest of the file loads normally.

A first key that isn't a shortcut on its own, like <kbd>⌘K</kbd>, waits for its next key however long you take.

## Change a key

You can change a command's key two ways.

- **In Settings.** Open Settings with <kbd>⌘,</kbd>, show Keyboard Shortcuts, pick a command and record a new key. You can also reset a command to its default or remove its key. The editor names a command that already uses the key you record and asks before it applies, and the change takes effect at once. It is saved to `keymap.toml`.
- **In `keymap.toml`.** Edit the file by hand and restart Fernrohr.

### The keymap.toml file

`keymap.toml` is in the preferences folder, next to `ui.toml`:

| Platform | Path |
| --- | --- |
| macOS | `~/Library/Preferences/com.pilgrimagesoftware.fernrohr/keymap.toml` |
| Linux | `~/.config/fernrohr/keymap.toml` |

On first run, Fernrohr writes the file with every command's default key. The file has one `[bindings]` table that maps a command ID to the keys. Use the ID from the tables above. Write keys in lowercase, joined with hyphens, as in `cmd-shift-k`. For a shortcut of two keys, separate the two with a space.

```toml
[bindings]
# Move a command to another key.
"shell.new_window" = "cmd-shift-n"

# Give a command with no default key one.
"panel.toggle_zoom" = "cmd-alt-z"

# A shortcut of two keys.
"panel.split_left" = "cmd-k h"

# An empty value removes the key. The command stays in the palette.
"app.hide_others" = ""
```

Commands that you leave out keep their default keys, including commands added in a later release. An entry that maps a key already used by another command takes that key from the other command's default.

> A key that is bound on its own and also starts a longer shortcut waits for the shortcut timeout before it runs. If you bind a key that is the first key of another shortcut, such as <kbd>⌘K</kbd>, the shorter shortcut is delayed.
{: .warning}

If a command's entry isn't a key Fernrohr can read, that command keeps its default key and the problem is logged. An entry for a command ID that doesn't exist is ignored. If the file can't be parsed at all, Fernrohr uses the default keys. In each case it leaves the file as it was, so you can fix it and restart.

> In the Keyboard Shortcuts editor, a key you record that another command already uses where both are available is named before it applies, and declining leaves both bindings unchanged. A panel command that shares a key with a global command takes the key inside that panel.

## Keys that aren't commands

Moving the selection one row or one item is plain cursor movement, not a command, so it isn't in the palette or in `keymap.toml`. In a list, a table or a tree, <kbd>↑</kbd> and <kbd>↓</kbd> move the selection. In the command palette and the pickers, they move the highlighted entry. <kbd>↵</kbd> runs or opens it, and <kbd>⎋</kbd> closes the palette, a picker or a dialog. Tab and Shift-Tab move between fields in a dialog. Hovering with the mouse never moves the keyboard selection.

*Summarizes the capability specs: `command-system`; source registry `app/src/command.rs`.*
