use serde::{Deserialize, Serialize};

/// Every window and its panel layout, persisted across restarts.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceConfig {
    pub windows: Vec<WindowLayout>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowLayout {
    pub width: f32,
    pub height: f32,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub panels: Vec<PanelDescriptor>,
}

impl Default for WindowLayout {
    fn default() -> Self {
        Self {
            width: 1024.0,
            height: 768.0,
            x: None,
            y: None,
            panels: Vec::new(),
        }
    }
}

/// A panel to restore into a window. `Unknown` catches any `kind` this build
/// doesn't recognize (e.g. written by a newer version) so restore can skip it
/// with a log line instead of failing the whole layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PanelDescriptor {
    Pods {
        cluster_context: String,
        namespace: NamespaceScope,
        filter: String,
        sort: SortState,
    },
    Logs {
        cluster_context: String,
        namespace: String,
        pod: String,
        container: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NamespaceScope {
    All,
    Single(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortState {
    pub column: String,
    pub ascending: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let config = WorkspaceConfig {
            windows: vec![WindowLayout {
                width: 1200.0,
                height: 900.0,
                x: Some(0.0),
                y: Some(0.0),
                panels: vec![PanelDescriptor::Pods {
                    cluster_context: "kind-dev".into(),
                    namespace: NamespaceScope::Single("kube-system".into()),
                    filter: "nginx".into(),
                    sort: SortState {
                        column: "age".into(),
                        ascending: false,
                    },
                }],
            }],
        };
        let text = toml::to_string(&config).unwrap();
        let parsed: WorkspaceConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn tolerates_unknown_fields() {
        let parsed: WorkspaceConfig = toml::from_str("future_field = true\n").unwrap();
        assert_eq!(parsed, WorkspaceConfig::default());
    }

    #[test]
    fn unrecognized_panel_kind_parses_as_unknown() {
        let toml_text = r#"
            [[windows]]
            width = 1024.0
            height = 768.0

            [[windows.panels]]
            kind = "exec-shell"
        "#;
        let parsed: WorkspaceConfig = toml::from_str(toml_text).unwrap();
        assert_eq!(parsed.windows[0].panels, vec![PanelDescriptor::Unknown]);
    }
}
