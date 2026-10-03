//! The Resource panel's edge preference (`cluster-picker-and-navigation` 11.1): the
//! window edge a new window's panel opens on, stored as `ui.toml`'s `resource_side`.
//! Moving the panel in one window (11.2) doesn't touch it; Make Resource Panel's
//! Side the Default saves that window's current side as the preference. The window
//! owns the command's handler, since only it knows which side its panel is on.

use super::ResourceSide;
use crate::command::{Command, CommandRegistry, MenuSlot, ViewGroup};
use crate::config::{self, ui::UiConfig};
use gpui_kit::*;
use std::path::{Path, PathBuf};

actions!(resource_panel, [SaveResourceSide]);

pub(crate) const SAVE_SIDE_COMMAND_ID: &str = "resource.save_side";

/// The preferred side, and the preference file it's saved to. No file means
/// nothing is saved, which is what tests that never call [`init_side_preference`]
/// get.
struct Preference {
    side: ResourceSide,
    path: Option<PathBuf>,
}

impl Global for Preference {}

/// Remembers the stored `side` and the preference file (`ui.toml`) it came from,
/// so a new window opens on it and a saved change is written back there.
pub fn init_side_preference(side: ResourceSide, path: PathBuf, cx: &mut App) {
    cx.set_global(Preference {
        side,
        path: Some(path),
    });
}

/// The edge a new window's Resource panel opens on: the preference, else the left.
pub fn preferred_side(cx: &App) -> ResourceSide {
    cx.try_global::<Preference>()
        .map_or(ResourceSide::default(), |preference| preference.side)
}

/// Makes `side` the preference for every window opened from now on, and saves it.
/// Windows already open stay where they are.
pub(crate) fn set_preferred_side(side: ResourceSide, cx: &mut App) {
    let path = cx
        .try_global::<Preference>()
        .and_then(|preference| preference.path.clone());
    cx.set_global(Preference {
        side,
        path: path.clone(),
    });
    if let Some(path) = path {
        save(side, &path);
    }
}

/// Rewrites only `resource_side` in the preference file, keeping the rest of it.
fn save(side: ResourceSide, path: &Path) {
    let mut ui: UiConfig = config::load(path);
    ui.resource_side = side;
    if let Err(error) = config::save(path, &ui) {
        log::warn!(
            "failed to save the Resource panel side to {}: {error}",
            path.display()
        );
    }
}

/// A global View-menu command with no default key: it's set once in a while, so
/// the palette and the menu are routes enough.
pub(super) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: SAVE_SIDE_COMMAND_ID,
        title: "Make Resource Panel's Side the Default",
        default_binding: "",
        context: None,
        action: Box::new(SaveResourceSide),
        menu: Some(MenuSlot::View(ViewGroup::ResourcePanel)),
    });
}
