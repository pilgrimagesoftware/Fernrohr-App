# ADR-0004: Build against a gpui-kit fork for the menu hover fix

**Status:** Accepted

**Date:** 2026-11-25

## Context

gpui-kit's `PopupMenu` lets the pointer choose what Enter activates: hovering a row writes the
menu's selected index, which `Confirm` reads ([Fernrohr#118](https://github.com/pilgrimagesoftware/Fernrohr/issues/118)).
A pointer resting over an open menu, which is where a click leaves it, silently changes the
row a keyboard user confirms. That breaks the keyboard-first rule that hover never moves the
selection. Table context menus, tab menus and `Button::dropdown_menu` all build `PopupMenu`
inside the library, so the App can't wrap its way around it.

## Decision

The App builds against `pilgrimagesoftware/gpui-kit`, a fork that tracks upstream releases.
Each upstream release gets a `fernrohr/vX.Y.Z` branch, carrying the fix and anything else we
need before upstream ships it, tagged `vX.Y.Z-fernrohr.N`. The root `Cargo.toml` patches all
five gpui-kit crates (`gpui-kit`, `gpui-component`, `gpui-base`, `gpui-component-macros`,
`gpui-kit-assets`) to that tag; patching fewer conflicts on gpui-kit-assets' `links`. The App
stays on the newest upstream release: the fork moves to each one by re-applying its patches
(the fork's `FORK.md` describes how). The first fork tag is `v0.11.0-fernrohr.1`.

The patch: hovering a plain row only highlights it, through its own hover style. A submenu
row still takes the selection on hover, since that is what opens it, and hovering a plain row
while a submenu is open closes the submenu without choosing that row.

**Exit condition:** drop the `[patch.crates-io]` section, and this record, once an upstream
release stops moving the selection on hover over a plain row. The regression test
`a_hovered_row_does_not_change_what_enter_activates` (`util/shell/contexts/awaiting_tests.rs`)
tells whether a release does: it fails against unpatched gpui-kit.

## Consequences

- Keyboard users get the row they chose, wherever the pointer rests.
- Every gpui-kit upgrade now has a fork step: branch from the new tag, port the patch, tag,
  repoint the five entries. The hover handler moves between releases, so the port is by hand.
- The App's crates resolve gpui-kit from git, not crates.io: `cargo audit` and Dependabot see
  the fork, and a build needs GitHub access to the fork.
- `gpui-terminal`, our other fork, must use the same gpui-kit release; the patch covers its
  dependency too, since `[patch.crates-io]` applies to the whole build.

## Alternatives Considered

- **Wait for upstream.** No release date, and the App would keep the bug until then. Worth
  proposing the patch upstream anyway; the exit condition covers it landing.
- **Wrap `PopupMenu` in the App.** The menus that show the bug are built inside the library
  (table context menus, dropdown buttons), so a wrapper can't reach them.
- **Vendor gpui-kit into this repo.** Much more code to own, and harder to follow upstream than
  a branch per release.
