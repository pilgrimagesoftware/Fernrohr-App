---
title: Windows, panels and layouts
version: v0.15
---

# Windows, panels and layouts

Fernrohr shows your work in panels. Panels sit in a dock inside a window, you can open as many windows as you like, and you can save a window's arrangement under a name and bring it back later. Everything here works from the keyboard and the mouse. Keys are shown for macOS. On other platforms <kbd>⌘</kbd> is <kbd>Ctrl</kbd>.

## Windows

The first launch opens one window with the cluster picker. See {{ '/v0.15/clusters/' | relative_url }} for connecting. Once a window is connected, it shows its panels, the Resource panel at one edge, and a status bar along the bottom with one capsule per cluster context.

The window title names the context the window shows. With two or more contexts it states how many, and before you connect it is just the app name.

**Open another window.** Choose Window > New Window, run **New Window** from the command palette, or press <kbd>⌘N</kbd>. The new window has its own empty workspace, and changes in one window don't touch the other.

**Switch windows.** The Window menu lists every open window by its title, so two windows on different clusters are easy to tell apart. Choose one to bring it forward.

**Close a window.** Choose Window > Close Window or press <kbd>⌘W</kbd>. Close Window closes the focused panel first, and closes the window only when no panel is on screen. If closing would stop a tunnel or discard an unsaved edit, Fernrohr asks first. On Linux, closing the last window quits Fernrohr after it saves its state. On macOS it keeps running in the menu bar; see [Quiet mode](#quiet-mode).

Fernrohr remembers each window's size and position, the contexts it used, and its panels and their arrangement. On the next launch it reopens the windows the same way and reconnects their contexts. If the saved state file can't be read, Fernrohr starts with one default window and leaves the unreadable file in place.

### Quiet mode

On macOS, closing the last window doesn't quit Fernrohr. It disconnects every cluster, as quitting
would, and keeps running with its icon in the menu bar. Its tunnels, port-forwards, shells and log
streams stop, and it saves your windows' state.

The icon's menu brings Fernrohr back:

| Menu item | What it does |
| --- | --- |
| Contexts | Lists your kubeconfig's contexts. Choose one to open a window connected to it. |
| Settings… | Opens Settings. |
| MCP… | Opens Settings at Agent Access, with the MCP server's setting. |
| Tunnel Manager… | Opens the Tunnel Manager. |
| Quit Fernrohr | Quits. |

Clicking Fernrohr's Dock icon with no window open reopens the window you closed last, connected to
the same contexts. Quit Fernrohr, in the menu or with <kbd>⌘Q</kbd>, is the only way to quit.

By default the MCP server keeps serving while Fernrohr is quiet, so an agent can still reach it.
See [Work with coding agents]({{ '/v0.15/agents/' | relative_url }}#when-no-window-is-open).

## Panels and tabs

Panels dock into groups. A group shows one panel at a time and lists the others as tabs. Each tab has its own close control.

### Open a panel

Open panels from the Resource panel or the command palette. The new panel becomes the active tab and takes keyboard focus, so its own keys work at once. Opening a panel that is already open brings it to the front of its tab group and focuses it.

A background open adds the panel as an inactive tab and leaves focus where it was. Use a modifier-click or a middle-click, or a panel's own background command, such as **Open in Background** in the Pods panel. See {{ '/v0.15/resources/' | relative_url }} for opening resources.

### Open a panel again

To keep two panels on the same resource, such as two Deployments lists scoped to different namespaces or a pod's detail beside the same pod's YAML, open another instance:

- **Open in New Panel** (<kbd>⇧⌘D</kbd>, also in the View menu and the command palette) opens another instance of the focused panel, on the same resource, context and namespaces, in its own tab with focus.
- In a list, <kbd>⇧Enter</kbd>, an <kbd>⌥</kbd>-click on a row, or **Open in New Panel** in the row's menu opens that row's resource in a new panel, even when its panel is already open.

Instances after the first end their title and tab with their number, such as **Pod: web-1 (2)**. A new instance takes the lowest number that's free, so closing **(2)** frees it for the next one. Each instance keeps its own namespaces, sort and filters. Opening the resource the usual way still focuses the first instance. On an **Open Logs by Label** panel, Open in New Panel opens a new empty label panel.

### Move focus and switch tabs

Exactly one panel has focus, and it is drawn differently from the others. Click a panel or tab to focus it, or use these commands.

| Command | Keys |
| --- | --- |
| Focus Next Panel | <kbd>⌘]</kbd> |
| Focus Previous Panel | <kbd>⌘[</kbd> |
| Focus Resources | <kbd>⌘0</kbd> |
| Next Tab | <kbd>⇧⌘]</kbd> |
| Previous Tab | <kbd>⇧⌘[</kbd> |
| Select Tab 1 to Select Tab 9 | <kbd>⌃1</kbd> to <kbd>⌃9</kbd> |

Focus wraps from the last panel to the first, and panels in a collapsed dock or behind a maximized panel are skipped. Next Tab and Previous Tab wrap inside the group. Select Tab 9 picks the group's last tab. A position past the last tab does nothing. These commands act on the focused tab group, or on the first group when focus is in the Resource panel.

### Split, move, merge and close groups

Drag a separator between two groups to resize them. From the keyboard, the dock commands work while a panel group has focus and you are not typing in a text field. Each one starts with <kbd>⌘K</kbd>, then a second key.

| Command | Keys |
| --- | --- |
| Panels: Split Group Left, Right, Up, Down | <kbd>⌘K</kbd> then an arrow key |
| Panels: Move Panel Left, Right, Up, Down | <kbd>⌘K</kbd> then <kbd>⇧</kbd> and an arrow key |
| Panels: Merge Group Left, Right, Up, Down | <kbd>⌘K</kbd> then <kbd>⌥</kbd> and an arrow key |
| Panels: Close Group | <kbd>⌘K</kbd> then <kbd>W</kbd> |

- **Split** opens a new pane in that direction holding a copy of the focused panel, and focuses it. The copy is another instance, numbered as Open in New Panel numbers one.
- **Move Panel** takes the focused panel out of its group and into the adjacent group. If its old group ends up empty, that pane goes away and the rest reflow.
- **Merge Group** moves every tab of the focused group into the adjacent group, in order, and removes the emptied pane.
- **Close Group** closes every panel in the group. If a panel would lose something, such as a running shell or an unsaved edit, one confirmation lists what is lost. Cancel it and nothing closes.

Move and merge do nothing, and show no error, when there is no group in that direction.

### Maximize a panel

Run **Maximize/Restore Panel** from the command palette or the View menu. It has no default key, but you can bind one in `keymap.toml`. The panel fills the workspace except for the Resource panel, and the other panels are hidden until you restore it. One panel per window can be maximized, and maximizing a second restores the first.

### Close a panel

Click the close control on a tab, or press <kbd>⌘W</kbd> (the Close Window command, which closes the focused panel's displayed tab before it closes a window). A running shell or an unsaved edit asks before the panel closes. A tab's close control closes that tab's panel, whichever panel has focus. You can close the last panel too. The window stays connected, shows the Resource panel beside an empty panel area, and moves focus to the Resource panel.

Closing the last panel does not return the window to the cluster picker. Disconnecting the window's last context does. That is in the context capsule in the status bar.

### The tab menu

Right-click a tab, or press <kbd>⌘K</kbd> then <kbd>M</kbd> (**Open Tab Menu**) for the focused tab, to open its menu. Move through it with <kbd>↑</kbd> and <kbd>↓</kbd>, press <kbd>Enter</kbd> to run an item, or <kbd>Esc</kbd> to close it. The menu acts on the tab it was opened for, even if that isn't the focused one.

| Item | What it does |
| --- | --- |
| Close Tab | Closes this tab. |
| Close Other Tabs | Closes every other unpinned tab in this tab's group. |
| Close Tabs to the Right | Closes the unpinned tabs to the right of this one. |
| Close All Tabs | Closes every unpinned tab in the group, as **Panels: Close Group** does. |
| Move to New Window | Moves this tab to a new window on its context, as that window's only panel, with its namespaces, filter and sort. |
| Pin Tab, or Unpin Tab | Pins the tab, or unpins it. |

Each item is also a command in the command palette, and you can bind a key to it in `keymap.toml`. Items that would close nothing are disabled. If a close would end a running shell or discard an unsaved edit, Fernrohr asks once first.

Pinned tabs sort first in their group, and the bulk closes leave them open. A pinned tab stays pinned when you restart, load a saved layout, or move it to a new window. A running shell can't move to another window, so moving a shell's tab ends the shell, and Fernrohr asks first.

### The Resource panel

The Resource panel sits at one edge of the window.

| Command | Keys |
| --- | --- |
| Collapse/Expand Resource Panel | <kbd>⌘B</kbd> |
| Move Resource Panel to Other Side | <kbd>⌥⌘B</kbd> |
| Focus Resources | <kbd>⌘0</kbd> |
| Make Resource Panel's Side the Default | none |

Moving the panel changes only the current window. **Make Resource Panel's Side the Default** saves the side as the starting edge for new windows. Collapsing the panel gives its space back to the other panels.

## The command palette

Press <kbd>⇧⌘P</kbd>, choose View > Command Palette, or run **Command Palette**. Every user-facing action is in the palette, with its key binding shown. Type to filter, move with <kbd>↑</kbd> and <kbd>↓</kbd>, and press <kbd>Enter</kbd> to run the highlighted command. Commands that belong to one panel, such as the pod commands, appear only while that panel has focus.

Hovering over a row with the mouse doesn't move the keyboard selection. You can change any key in `keymap.toml`. See {{ '/v0.15/settings/' | relative_url }} and {{ '/v0.15/reference/keybindings/' | relative_url }}.

## The application menu

The menu bar has seven menus, in this order: App, Context, Edit, View, Navigate, Window and Help. Each item runs the same command as the palette and shows its current key.

| Menu | What it holds |
| --- | --- |
| App | About Fernrohr, Settings…, and the platform items Hide, Hide Others, Show All and Quit |
| Context | Opening the cluster picker and switching, adding and disconnecting contexts, then every tunnel action after a separator |
| Edit | The platform's text-editing items. Fernrohr adds none |
| View | Command Palette, then theme and text size, then the Resource panel items, then Maximize/Restore Panel, then table columns |
| Navigate | Focus Next Panel, Focus Previous Panel, Focus Resources, the tab commands, and a Select Tab submenu |
| Window | One entry per open window, plus New Window, Save Panel Layout…, Saved Layouts…, Open Logs by Label…, Minimize, Zoom and Close Window |
| Help | Report Issue |

Navigate holds only commands that work whichever panel has focus. A command for one panel type, such as describing the selected pod, stays out of the menu. It remains in the palette, in `keymap.toml` and in that panel's hint row.

## Saved panel layouts

Fernrohr already restores a window's arrangement when you relaunch. A saved layout is separate. You name it, keep as many as you want, and load one whenever you like.

A saved layout holds:

- the dock arrangement: splits, panel sizes, tab order, active tab and maximized state;
- the Resource panel's width and whether it is visible;
- each panel's content, meaning its kind, object, namespace selection and cluster context;
- each list panel's filter text and sort column and direction;
- the window's size.

A Secret value you have revealed is never saved.

### Save a layout

1. Run **Save Panel Layout…** with <kbd>⇧⌘S</kbd>, from the command palette, or from the Window menu. It needs a connected window, so it is unavailable from the cluster picker.
2. Type a name and confirm.

If the name is already used, Fernrohr asks before it replaces that layout. Names that differ only in case count as the same name.

### Load, rename and delete layouts

Run **Saved Layouts…** with <kbd>⇧⌘O</kbd>, from the palette, or from the Window menu. The picker lists every saved layout by name, or says so when there are none. It works from the cluster picker too.

Move the highlight with <kbd>↑</kbd> and <kbd>↓</kbd>, or click a row.

| Action | Keys |
| --- | --- |
| Load (Replace) | <kbd>Enter</kbd> |
| Load (Add) | <kbd>⌘Enter</kbd> |
| Rename | <kbd>R</kbd> |
| Delete | <kbd>Backspace</kbd> |

- **Load (Replace)** swaps the window's whole arrangement for the saved one, including the Resource panel state and the window size.
- **Load (Add)** opens the saved panels next to the ones already open. The Resource panel and the window size stay as they are, and a panel that is already open is not duplicated. Each saved instance of a panel counts separately, so a layout holding a panel and its second instance adds the second next to an open first one.

Neither route opens a new window or moves the current one.

Renaming to a name another layout already uses is refused, and both layouts keep their names. Deleting can't be undone, so the confirmation opens with Cancel focused. Pressing <kbd>Enter</kbd> cancels. Delete only by clicking the destructive control or by tabbing to it and activating it.

You can also list and remove layouts in Settings, in the Layouts section. Run **Settings: Show Layouts** from the palette while the Settings window has focus, or pick Layouts in its sidebar.

### When a panel can't be restored

Loading never drops a panel silently. A panel gets a placeholder in its original position, and the rest of the layout loads normally, when:

- its cluster context isn't one of the window's contexts. The placeholder says the window isn't connected to it, and Fernrohr does not connect it for you;
- its namespace or object no longer exists;
- this version of Fernrohr doesn't recognize its kind.

### Where layouts are stored

Each layout is its own file, written in a way that never leaves a half-written file, in a `layouts/` folder inside Fernrohr's state directory. On macOS that is `~/Library/Application Support/com.pilgrimagesoftware.fernrohr/layouts/`. On Linux it is `~/.local/share/fernrohr/layouts/`. The folder is created when you save your first layout.

The layout's name is stored inside the file, so the file name can differ from it. A file that can't be read is skipped and reported by file name, and it is left untouched. Your other layouts still load. Agents can list and load layouts too. See {{ '/v0.15/agents/' | relative_url }}.

*Summarizes the capability specs: `app-shell`, `application-menu`, `saved-panel-layouts`.*
