
## 0.1.0 - 2026-10-02

### <!-- 0 -->🚀 Features
- Cargo workspace, gpui-kit dock shell, kube-rs/tokio deps, CI matrix
- Platform paths and typed TOML config loading
- Tokio runtime spine with coalescing drain and resource index
- Cluster connection - kubeconfig loading, connect probe, API discovery
- App shell - docked multi-panel windows, new window command, workspace persistence
- Resource browser - refcounted watch registry, Pods table with namespace/filter/sort, reconnect reconciliation
- Pod logs - line-by-line stream, follow mode, container selection, distinct terminal states
- Command system - CommandRegistry, keymap.toml loader, fuzzy palette items
- Wire command palette to cmd-shift-p, opening as a Dialog on the focused window
- Wire Pods panel to a live cluster connection and kube_runtime watch
- Wire a real Logs panel into the dock, replacing PlaceholderPanel
- Add keyring dependency with keychain smoke test
- Verify TLS server-name pinning through a local port
- Document ssh on PATH and add a fail-fast check
- Add ManagedForward trait and local-port allocator
- Add the per-forward supervisor task
- Add identity-keyed ForwardRegistry
- Implement SshTunnel transport for section 3.1
- Readiness probe and auth/bind failure classification for SshTunnel
- Section 3.3 jump-host chain test coverage
- Process-group kill on drop and stale-pidfile sweep (section 3.4)
- Implement PodPortForwardTransport and accept-loop bridge (section 4.1)
- Define tunnels.toml schema (section 5.1)
- Add keychain wrapper with in-memory fallback (section 5.2)
- Implement tunnel CRUD store (section 5.3)
- Add context binding to TunnelStore (section 6.1)
- Wire connect path to tunnel forwards (section 6.2)
- Add Paused state to WatchRegistry (section 7.1)
- Implement ConnectionHealth pause/resume on forward flap (section 7.2)
- Wire exec-plugin 401 handling into pause/resume path (section 7.3)
- Surface pause reason and elapsed time on panels (section 7.4)
- Add cluster picker and connect-by-context
- Resource-kind navigation and picker-on-empty-dock
- Wire theme/appearance and redesign picker and sidebar
- Panel navigation and resource kind panels (sections 8-9)
- Scope struct and title-bar rules for resource panels
- Title bar naming kind, cluster, and namespace (section 10)
- Show the Fernrohr logo above the cluster chooser
- Add resource cluster dropdown
- Add cluster namespace selection
- Show the Fernrohr logo at full size in the cluster picker
- Restore context dock layouts
- Show pod detail in its own dockable panel
- `y` opens the pod detail panel on the YAML, not the field list
- Show pod/container in the title, virtualize line rendering
- Sortable, resizable, reorderable Pods table
- Per-context health and severity
- Window status bar
- Bastion-only tunnel config with auth marker
- Derive forward target from the context's API server
- Pick a context's tunnel from the picker
- Tunnels window
- Hold/release a context per window, not per app
- Persist and restore every context a window used
- Native application menu bar, Manrope/Monaco fonts
- Wire per-context release for Disconnect, add window-context-bar's pure decision logic
- Make the Resource panel's cluster dropdown follow the active context
- Add the workspace window's context bar
- Wire the context bar, add/disconnect a context, and persist live contexts
- Name a panel's context in a tooltip and a Context line, not its title
- Resizable Resource panel with the context selector beside its label
- Remember each window's Resource panel width
- Name the context in Logs and Pod detail titles in multi-context windows
- Pod and pod/container headings, with context, at the top of Pod detail and Logs
- Select a context before connecting it
- Drive the cluster picker fully from the keyboard
- Reorganize pod detail panel into tabs
- Register pod detail shortcuts as context-gated commands
- Add the Resource panel's category model
- Group the Resource panel into collapsible category sections
- Add a bottom-pinned filter to the Resource panel
- Make the Resource panel fully keyboard-operable
- Focus Resources command, the keyboard's way into the Resource panel
- Resource panel actions are palette commands
- Resource rows show the kind; the API group moves to a tooltip
- Register the Pods panel shortcuts as palette commands
- Underline the focused panel's tab
- Mark the focused tab with the user's macOS accent colour
- Move keyboard focus between panels, and focus opened panels
- Project the objects a pod references as typed ObjectRefs
- Render followable references as links
- Open followed references in the source panel's context
- Follow references from the keyboard with a "Go to…" picker
- Open any discovered kind in a generic object viewer
- Kind-specific sections for the kinds pods reference
- Switch tabs from the keyboard; Cmd-W closes the focused tab
- In-app keybindings editor
- Configuration tab with per-value Secret reveal
- Reveal a Secret's values one at a time
- Visual definition - one accent, layered surfaces, status colour
- Add release and package workflows
- Replace the About window placeholder with a real one
- Bucket Custom Resources' kinds by API group
- Per-window collapsed-subgroup state, starting empty
- Render Custom Resources' API-group subgroups
- Toggle a subgroup by clicking its header
- A filter opens collapsed subgroups with a match
- Step the Resource panel's keyboard cursor through subgroup headers
- Add a toggle-group command for Custom Resources subgroups
- Start Custom Resources subgroups collapsed
- Add collapse-all and expand-all commands for resource groups
- Collapse large ConfigMap values in the Configuration tab
- Compute each container's expanded detail
- Expand a container card to show its detail
- Use a disclosure chevron and tint secret reveals danger
- Add title_for, a window's title from its mode
- Give each main window a visible initial title
- Re-title a window as its cluster contexts change
- Ad-hoc sign the macOS app and dmg
- Bundle Adamina and add frame, data and code type roles
- Draw frame text in Adamina and data text in Manrope
- Add ui::space spacing tokens scaled by text size
- Add a text-size preference that scales every window
- Add Increase, Decrease and Reset Text Size commands
- Add a Text Size stepper to the Settings window
- Add Report Issue menu item to Help menu (#67)
- Vendor the Kubernetes icon set and its fallbacks
- Look up a resource kind's icon by group and kind
- Draw a kind icon 1:1 at its text's size
- Lead every panel's tab with its kind's icon
- Lead the Resource panel's kind rows with their icons
- Lead every resource reference with its kind's icon
- Show the container icon on each container card
- Credit the Kubernetes icon set and ship its licence
- Lead detail panel headers with a header-size kind icon


### <!-- 1 -->🐛 Bug Fixes
- Allow parking in runtime tests for cross-thread tokio wakeups
- Return the log stream drain Task instead of detaching, fixing a CI race
- Wire ClusterSession into Pods panel creation for a real shared watch
- Capture window geometry on close so quit-and-relaunch actually restores layout
- Correct test.yml path filters after develop/master reconcile
- Silence dead-code noise the cherry-pick missed
- Correct test.yml path filters after develop/master reconcile
- Start dbus/gnome-keyring so Linux tests don't hang on Secret Service
- Stop two suites depending on ambient machine state
- Floor the window size so the picker cannot be clipped
- Ship the logo losslessly - the q90 encode degraded at display size
- Drop mono font in Pods table, inset focus border, fix list panel titles
- Move namespace picker into panel bodies, expand pod detail
- Bind the panel's own shortcuts, fix focus-loss border
- Double-click opens pod detail, container picker, follow/jump controls, YAML toggle relocation
- Keep the selected pod highlighted across sorts and row updates
- Don't flag bindings stale when the kubeconfig can't be read
- Refresh open pickers when tunnels change
- Remove the inset border around panel content
- Report the full error source chain on connect failures
- Restore stale temp-file cleanup dropped from the WIP commit
- Put tunnel commands in the native menu bar's Context menu
- Defer context sync to stop re-entrant updates; cover the context bar
- Draw Root's dialog, sheet and notification layers in main windows
- Capsule context chips and give the disconnect dialog its buttons
- Right-align the Resource panel's context selector
- Open the Tunnels window at a modest, centered size
- Keep the Resource panel's context selector inside the panel
- Carry the context with a selected pod
- Readable, copyable stream errors
- Render error text verbatim through markdown
- Draw the Resource panel header outside Sidebar's slot
- Stop ssh forwards when the app quits and sweep leftovers at startup
- Only a click selects a picker context; drop Connect's missing icon
- Standard macOS app and window menu items, with their shortcuts
- Harden pod detail Events and Managed Fields tabs
- Bind every registered command; one key hint per picker action
- Don't restore windows the user closed
- Stop process-group kill from signalling every process on Linux
- Keep the pod heading's context label from overlapping the key hints
- Gate the test-only Resource panel focus query
- Command palette focuses its search and offers the focused panel's commands
- The command palette keeps its own, visible keyboard selection
- Register the platform menu items and drop duplicate bindings
- Adopt #24's focused-tab title and drop focus_frame
- Open a panel in the tab group it was opened from
- Make followable links look like links before hover
- Keep test accessors private; test the rebuilt menu
- Add icns icon and before-packaging-command for cargo-packager
- Put the Expand control beside its ConfigMap key
- Collapse ConfigMap values only past 100 characters
- Save a window's content size, not its outer frame
- Save only main windows' layouts at quit
- Save the workspace when a window stops moving or resizing
- Align container card text and drop the Containers label
- Unset empty Apple secrets before packaging
- Accept the dmg license without a failing pipe
- Bundle Manrope as static faces, not a variable font
- Ship the OFL licence with the bundled fonts
- Restore About window close button and app-name color
- Draw the logo at its device resolution
- Draw the icon at its full size and resolution
- Focus what the picker draws, so the palette opens without a kubeconfig


### <!-- 10 -->💼 Other
- Single-threaded verbose test run with timeout to isolate Linux hang
- Keep debug test wrapper Linux-only, macOS lacks GNU stdbuf/timeout
- Cut Linux debug-info/parallelism and log memory to test OOM theory
- Split Build step per-OS, empty RUSTFLAGS/CARGO_BUILD_JOBS broke macOS
- Drop unproven dbus/debuginfo experiments, keep bounded Linux test run
- Resolve develop conflict
- Per-run keychain ids in store tests, unconditional credential delete
- Bump tokio-tungstenite from 0.29.0 to 0.30.0
- Bump gpui-kit from 0.6.6 to 0.7.0


### <!-- 2 -->🚜 Refactor
- Organize the code
- Drop the per-panel pause banner
- Model a window's contexts as a list
- Split pod_detail.rs into a module under the 500-line limit
- Move panel title tests to a sibling tests.rs
- Move pidfile tests to a sibling tests.rs
- Split util/shell.rs and its tests under the 500-line limit
- Share the keystroke-test window helpers via test_support
- Split pods.rs and its tests under the 500-line limit
- Split connection.rs under the 500-line limit
- Split picker.rs and its tests under the 500-line limit
- Split the tunnels list and editor under the 500-line limit
- Split util/logs.rs under the 500-line limit
- Split session.rs under the 500-line limit
- Split tunnel/store.rs under the 500-line limit
- Split the sshd integration tests under the 500-line limit
- Space detail panels with the spacing tokens
- Space tables with the spacing tokens
- Space the context and status bars with the spacing tokens
- Space dialogs and windows with the spacing tokens
- Open and close dialogs through WindowExt
- Name util::shell::open_window explicitly
- Open every window through gpui_kit::open_window


### <!-- 3 -->📚 Documentation
- Add logo to top of README
- Add logo to top of README
- Add logo to top of README
- Note that keyboard navigation is a first-class requirement
- Every user-facing action is a command palette entry


### <!-- 5 -->🎨 Styling
- Apply cargo fmt
- Format pod detail panel


### <!-- 6 -->🧪 Testing
- Cover target-loss recovery via the existing supervisor (section 4.2)
- Assert unbound context never touches ForwardRegistry
- Close out deferred tests for 1.2, 3.2, 4.4
- Cover context dock layouts
- Cover second-window connect through the real picker path
- Pin the shortcut from a focused child scope, both ways
- Allow parking where tests start a real cluster connect
- Pin registry sessions to a fixed connection state in tests
- Make real-keychain tests opt-in
- Compare bindings through the same parse, not a macOS literal
- Wait for the swept orphan to be reaped
- Check the platform shortcuts on every platform
- Wait for the orphaned stand-in to settle before sweeping it
- Reduce motion so picker clicks don't race the dialog's entrance
- Reduce motion in the object panel's picker tests
- Pin keymap.toml overrides for the panel focus commands
- Compare keys as each platform spells them
- A filter shows only subgroups with a match
- Replace real cluster names with placeholders
- Assert window titles across context changes and restore


### <!-- 7 -->⚙️ Miscellaneous Tasks
- Rename the license file
- Scaffold Rust project with CI, docs, and robot guidance
- Store images
- Add symlink to Rust structure rules
- Silence pre-existing dead-code clippy noise per UNWIRED convention
- Silence pre-existing dead-code clippy noise per UNWIRED convention
- Store images
- Merge master (tunnel-subsystem) into develop
- Checkpoint
- Checkpoint
- Checkpoint
- Ignore worktrees/
- Checkpoint
- Update README with signing instructions


# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Repository scaffolding with CI, testing, and contribution guidelines
- Rust project structure with GPUI and GPUI-Kit dependencies
- Initial GitHub Actions workflows for build and lint checks
