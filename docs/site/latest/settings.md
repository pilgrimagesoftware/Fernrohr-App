---
title: Settings and app windows
version: latest
---

# Settings and app windows

This page covers the settings you can change, the notifications Fernrohr sends, and the small windows that help you report a problem. Keys are shown for macOS. On other platforms <kbd>⌘</kbd> is <kbd>Ctrl</kbd>.

## Open Settings

Choose **Settings…** from the App menu, run it from the command palette, or press <kbd>⌘,</kbd>. There is only one Settings window. Opening it again brings the open one to the front.

Tabs across the top of the window show the sections: Keyboard Shortcuts, Appearance, Panels, Layouts and Agent Access, each with its icon. Click a tab, or <kbd>Tab</kbd> to it and press <kbd>Enter</kbd> or <kbd>Space</kbd>. <kbd>⌃Tab</kbd> and <kbd>⌃⇧Tab</kbd> (**Settings: Next Tab** and **Settings: Previous Tab**) step through the tabs from anywhere in the window. Showing a section moves focus into it, so the next <kbd>Tab</kbd> reaches its first control. In a narrow window the tabs scroll sideways.

Each section groups its controls in titled cards, with a short hint under a setting where it helps. While the Settings window has focus, the palette also offers **Settings: Show Keyboard Shortcuts**, **Settings: Show Appearance**, **Settings: Show Panels**, **Settings: Show Layouts** and **Settings: Show Agent Access**. They have no default keys.

This page covers Appearance. For Layouts, see {{ '/latest/windows-panels/' | relative_url }}. For Agent Access, see {{ '/latest/agents/' | relative_url }}. For Keyboard Shortcuts, see {{ '/latest/reference/keybindings/' | relative_url }}.

## Text size

One text-size setting scales interface text, data text and code text together, and panel spacing scales with it. The default is 100%. The steps are 85, 90, 100, 110, 120, 130, 140 and 150 percent. At either end, stepping further does nothing.

| Command | Keys |
| --- | --- |
| Increase Text Size | <kbd>⌘=</kbd> |
| Decrease Text Size | <kbd>⌘-</kbd> |
| Reset Text Size | <kbd>⇧⌘0</kbd> |

Run these from the keyboard, from the command palette, or from the View menu. You can also use the Text Size row in Settings > Appearance. It shows the current percentage with **−**, **+** and **Reset** buttons, and the three keys beside them. The change applies to every open window at once, with no restart, and Fernrohr opens at that size next time.

## Theme and accent

Choose System, Light or Dark in any of these ways:

- the theme switcher at the far end of the status bar;
- the Theme row in Settings > Appearance;
- the palette or the View menu: **Theme: Follow System**, **Theme: Light** and **Theme: Dark**.

The choice applies to every window at once and is remembered. With System, Fernrohr follows the operating system when it switches between light and dark, without a restart. The fonts stay the same in both themes.

There is no accent setting. Links, the selected row, the focused tab, focus rings and primary buttons share one accent colour. It is your system accent colour where the platform provides one, and a blue otherwise. If the system accent is too light to read against the background, Fernrohr darkens it. Text meets a 4.5:1 contrast ratio against its background in both themes, and accent and status colours meet 3:1.

In the Pods table, pod status is coloured by health. Running is the good tone, pending the warning tone, failed or crash-looping the bad tone and completed the neutral tone.

## Fonts

Fernrohr ships its own fonts, so nothing needs to be installed. Interface text is set in Adamina and data in tables and detail views in Manrope. YAML and log output use Monaco when it is installed. Otherwise they use a monospace font that is present on your system.

## Settings files

Settings are saved as you change them, and some are plain TOML files you can edit.

- `ui.toml` holds the theme, text size and other Appearance and Panels settings. It lives in the preferences folder: `~/Library/Preferences/com.pilgrimagesoftware.fernrohr/` on macOS, and `~/.config/fernrohr/` on Linux.
- `keymap.toml` in the same folder holds your key bindings.

```toml
theme = "dark"
text_size = 110
```

The theme is `"system"`, `"light"` or `"dark"`. The text size is a percentage. A hand-edited value between steps, or out of range, loads as the nearest step.

Window state and saved layouts are not meant for hand-editing. They live in Fernrohr's state directory, `~/Library/Application Support/com.pilgrimagesoftware.fernrohr/` on macOS and `~/.local/share/fernrohr/` on Linux.

## Desktop notifications

Fernrohr can post a notification through your operating system's notification center, on macOS and on Linux desktops that have a notification service. It does this when something needs you while you are working in another app. For example, a tunnel waiting for you to bring its network path up and choose Proceed posts one. See {{ '/latest/tunnels-forwards/' | relative_url }}.

There is no setting in Fernrohr to turn notifications on or off. Allow or deny them for Fernrohr in your operating system's notification settings. A notification never stands alone. The same condition stays visible in the app until it is resolved, so if notifications are denied or unavailable, nothing is lost and no error dialog appears.

Where the platform reports a click, clicking a notification brings Fernrohr to the front with the related window focused.

## About Fernrohr

Choose **About Fernrohr** from the App menu, or run it from the palette. It has no default key. The window shows the app icon, its name, the released version and a build identifier with the build date and commit. The identifier says the commit is unknown when Fernrohr was built without git.

Click the version and build text to copy one line to the clipboard with the name, version and build. Paste it into a bug report. Press <kbd>Esc</kbd> to close the window. On platforms other than macOS it also has a **Close** button. Opening About again brings the open window to the front instead of opening a second one.

## Report an issue

Choose **Report Issue** from the Help menu, run it from the palette, or press <kbd>⇧⌘/</kbd>. The
Report Issue window opens beside your other windows, with Subject ready to type in. It doesn't block
them, so you can look up what you want to describe while it's open. There is one at a time: choosing
Report Issue again brings it to the front with your text as it was.

1. Pick **Bug** or **Feature request**. A bug is labelled `bug` on GitHub, and a feature request
   `enhancement`. Switching keeps what you've typed.
2. Fill in **Subject** and **Description**. **Report** is enabled once both have text. Enter in
   Description starts a new line.
3. For a bug, you can check **Attach the application log**. It's off until you turn it on, because
   logs can include context names, hosts and paths, and Fernrohr doesn't remove them. Once it's
   checked, the window shows the log's last 200 lines, and the report carries exactly that text in a
   collapsed section. Read it before you report, and uncheck it if it holds something private.
4. To include screenshots, choose **Add Screenshots…** (<kbd>⇧⌘A</kbd>) and pick up to five PNG,
   JPEG or GIF files. Remove one with the × beside its name.
5. Choose **Report** or press <kbd>⌘↵</kbd>.

The window lists your version, build and platform, which every report carries. The version and build
are the same as in the About window. The build shows the date the app was built and its commit, or
`commit unknown` when the commit wasn't available at build time.

Report files the issue with the GitHub CLI (`gh`) if it is installed and signed in. Otherwise it
opens a prefilled new-issue page in your browser, and the window says which one it will use. If `gh`
fails, Fernrohr opens the browser page instead, and a notification says why. The browser page can't
carry the log's text, so it asks you to attach the log file instead.

GitHub can't take screenshots through `gh` or a prefilled page, so the issue lists their file names.
Once the report is sent, Fernrohr shows the files: it reveals them in Finder on macOS and opens their
folders on Linux. Drag them onto the issue. When `gh` filed it, the new issue opens in your browser
for that.

Choose **Close** or press <kbd>Esc</kbd> to close the window without reporting anything.

> The application log is `fernrohr.log`, in `~/Library/Logs/com.pilgrimagesoftware.fernrohr/` on
> macOS and `~/.local/state/fernrohr/` on Linux. The previous run's log is kept beside it as
> `fernrohr.log.1`. Set `RUST_LOG` to `debug` or `trace` before starting Fernrohr for more detail.

### Report an error from where it happened

When a panel can't load what it shows, such as a resource list, a detail view, Events or Logs, it
shows the error with a **Report…** button below it. So does a failed connection in the
[cluster picker]({{ '/latest/clusters/' | relative_url }}#errors-and-retrying). Click it, or press
<kbd>Tab</kbd> to reach it and then <kbd>Enter</kbd> or <kbd>Space</kbd>. It opens the Report Issue
window as a bug report with the fields filled in:

- **Subject:** the error message's first line, cut to 80 characters.
- **Description:** the full message, followed by the error's technical detail in a code block.

If the window is already open with something typed in it, Report… only brings it to the front, and
your text stays. Edit either field before you choose **Report**. A report from a failed connection
leaves out the context's name, because the issue is public, but check the error text for anything
private before you send it.

*Summarizes the capability specs: `typography`, `visual-language`, `desktop-notifications`, `about-window`, `report-issue`, `app-logging`.*
