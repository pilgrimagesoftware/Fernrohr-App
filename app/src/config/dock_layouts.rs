use gpui_kit::component::dock::DockAreaState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Dock layouts keyed by Kubernetes context name.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DockLayouts {
    #[serde(default)]
    layouts: HashMap<String, DockAreaState>,
}

impl DockLayouts {
    #[cfg(test)]
    pub fn get(&self, context_name: &str) -> Option<&DockAreaState> {
        self.layouts.get(context_name)
    }

    pub fn insert(&mut self, context_name: String, layout: DockAreaState) {
        self.layouts.insert(context_name, layout);
    }
}

pub fn load(path: &Path) -> DockLayouts {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, layouts: &DockLayouts) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents =
        serde_json::to_string_pretty(layouts).expect("dock layout state must serialize to JSON");
    fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::{DockLayouts, load, save};
    use gpui_kit::component::dock::DockAreaState;

    #[test]
    fn layouts_round_trip_by_context() {
        let path = std::env::temp_dir().join("fernrohr-dock-layouts-test.json");
        let mut layouts = DockLayouts::default();
        layouts.insert("dev".into(), DockAreaState::default());
        layouts.insert("prod".into(), DockAreaState::default());

        save(&path, &layouts).unwrap();
        let loaded = load(&path);

        assert!(loaded.get("dev").is_some());
        assert!(loaded.get("prod").is_some());
        let _ = std::fs::remove_file(path);
    }
}
