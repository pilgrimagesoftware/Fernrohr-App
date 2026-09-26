use serde::{Deserialize, Serialize};

// UNWIRED: no settings UI or config-load call site reads this yet; only its
// own round-trip tests exercise it. First real caller is whatever surfaces a
// theme picker.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: Theme,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: Theme::System,
        }
    }
}

// UNWIRED: see `UiConfig` above - no caller reads a theme yet.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let config = UiConfig { theme: Theme::Dark };
        let text = toml::to_string(&config).unwrap();
        let parsed: UiConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn tolerates_unknown_fields() {
        let parsed: UiConfig = toml::from_str("theme = \"light\"\nfuture_field = 42\n").unwrap();
        assert_eq!(parsed.theme, Theme::Light);
    }
}
