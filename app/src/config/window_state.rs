use serde::{Deserialize, Serialize};

/// The last-known geometry of the main window, restored on launch.
// UNWIRED: no load/save call site wires this into window-open yet; only its
// own round-trip tests exercise it.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowStateConfig {
    pub width: f32,
    pub height: f32,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub maximized: bool,
}

impl Default for WindowStateConfig {
    fn default() -> Self {
        Self {
            width: 1024.0,
            height: 768.0,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let config = WindowStateConfig {
            width: 1200.0,
            height: 800.0,
            x: Some(10.0),
            y: Some(20.0),
            maximized: true,
        };
        let text = toml::to_string(&config).unwrap();
        let parsed: WindowStateConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn tolerates_unknown_fields() {
        let parsed: WindowStateConfig =
            toml::from_str("width = 800.0\nheight = 600.0\nfuture_field = \"x\"\n").unwrap();
        assert_eq!(parsed.width, 800.0);
        assert_eq!(parsed.height, 600.0);
    }
}
