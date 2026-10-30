# ADR-0001: A macOS status item through objc2

**Status:** Accepted

**Date:** 2026-10-30

## Context

Quiet mode (#236) keeps Fernrohr running on macOS after its last window closes, with an icon in the
menu bar whose menu opens a context, Settings, the MCP settings and the Tunnel Manager, or quits.

gpui (`gpui-pre` 0.3.7, through gpui-kit 0.7.0) has no status item API, and nothing in the build
offers one: no `tray-icon`, `muda` or similar crate is in `Cargo.lock`. gpui's macOS backend talks to
AppKit through `objc2`, `objc2-foundation` and `objc2-app-kit` 0.3.2, which the app already uses
for the accent colour. Linux and Windows keep quitting with their last window; their tray is #336.

## Decision

Draw the status item with AppKit's `NSStatusItem` directly, in one module behind
`cfg(target_os = "macos")`: `app/src/ui/status_menu/macos.rs`.

- Depend on `objc2` 0.6.4 and `objc2-foundation` 0.3.2 directly, and widen `objc2-app-kit` 0.3.2's
  features to `NSStatusBar`, `NSStatusItem`, `NSStatusBarButton`, `NSMenu`, `NSMenuItem`,
  `NSImage`, `NSButton`, `NSControl`, `NSView` and `NSResponder`. All three crates are already in
  the build at these versions; the change adds no crate.
- What the menu lists is a pure, platform-neutral function (`ui::status_menu::items`), tested
  without AppKit. Each entry dispatches an existing command's action, so the menu is another route
  to those commands rather than its own implementation.
- One Objective-C class, `FernrohrStatusMenuTarget` (`define_class!`), receives the menu's
  `choose:` action and its delegate's `menuWillOpen:`. It forwards both through a channel to a
  foreground task, so no app code runs inside AppKit's callback.
- The `unsafe` surface is three calls, each with a `SAFETY` comment naming its invariant: the
  class's `init`, creating a menu item with the `choose:` action, and setting the item's target.
  AppKit holds targets and delegates weakly, so the target lives in an app global for the app's
  lifetime, outliving every item that refers to it.
- The icon is the `binoculars` SF Symbol as a template image, so it follows the menu bar's
  appearance. The item is shown from launch, not only once the windows are gone. The Dock icon and
  the activation policy are left as they are.

## Consequences

- Quiet mode has a way back with no window open, and the MCP server can keep serving behind it.
- This is the app's second piece of direct AppKit code, after the accent colour. A gpui upgrade
  that changes its objc2 versions moves these pins with it.
- The AppKit side can't run in `cargo test`: status items need the main thread, and the test
  harness never runs there. The menu model, the actions each entry dispatches and the quiet-mode
  lifecycle are tested headless. The status item itself is checked by hand on macOS.
- Linux and Windows get nothing yet; #336 tracks their tray.

## Alternatives Considered

- **`tray-icon` (with `muda`):** cross-platform and maintained, but it adds two crates and their
  dependency trees for a macOS-only feature today, and brings its own menu model and event loop
  integration to bridge into gpui. Worth revisiting for #336.
- **Waiting for gpui to add a status item:** nothing upstream is planned that we know of, and quiet
  mode needs it now.
- **Hiding the Dock icon (`LSUIElement`, or the Accessory activation policy):** rejected by the
  user's decision for #236. The Dock icon stays, and clicking it reopens a window.
