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
    let contents = serde_json::to_string_pretty(layouts).map_err(std::io::Error::other)?;
    super::write_atomic(path, &contents)
}

#[cfg(test)]
mod tests {
    use super::{DockLayouts, load, save};
    use gpui_kit::component::dock::DockAreaState;

    #[test]
    fn layouts_round_trip_by_context() {
        let path = crate::util::test_paths::temp_path("dock-layouts").with_extension("json");
        let mut layouts = DockLayouts::default();
        layouts.insert(
            "dev".into(),
            DockAreaState {
                version: Some(1),
                ..Default::default()
            },
        );
        layouts.insert(
            "prod".into(),
            DockAreaState {
                version: Some(2),
                ..Default::default()
            },
        );

        save(&path, &layouts).unwrap();
        let loaded = load(&path);

        assert_eq!(loaded.get("dev").and_then(|state| state.version), Some(1));
        assert_eq!(loaded.get("prod").and_then(|state| state.version), Some(2));
        let _ = std::fs::remove_file(path);
    }

    /// Tier 1 #3: a corrupt or foreign `dock-layouts.json` loads as no saved
    /// layouts, so the window opens on its default dock instead of failing.
    #[test]
    fn a_corrupt_file_loads_as_no_layouts() {
        let path =
            crate::util::test_paths::temp_path("dock-layouts-corrupt").with_extension("json");
        for contents in ["{ not json", r#"{"layouts": {"dev": 7}}"#, "[]"] {
            std::fs::write(&path, contents).unwrap();
            assert!(load(&path).get("dev").is_none(), "{contents}");
        }
        let _ = std::fs::remove_file(path);
    }
}
