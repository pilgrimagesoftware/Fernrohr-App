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
    /// Every context this window uses, in the order it added them
    /// (`window-context-bar` design.md decision 3). Empty in a file written by a
    /// version that stored one context per window instead - `util::shell::
    /// restored_contexts` derives the list from `panels`' own `cluster_context`s
    /// for that case, so this field is never the only way to find out.
    pub contexts: Vec<String>,
    pub panels: Vec<PanelDescriptor>,
}

impl Default for WindowLayout {
    fn default() -> Self {
        Self {
            width: 1024.0,
            height: 768.0,
            x: None,
            y: None,
            contexts: Vec::new(),
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
                contexts: vec!["kind-dev".into()],
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

    /// Tasks.md 2.1: a window using two contexts round-trips both, in order.
    #[test]
    fn round_trips_a_two_context_window() {
        let config = WorkspaceConfig {
            windows: vec![WindowLayout {
                contexts: vec!["kind-dev".into(), "staging".into()],
                panels: vec![
                    PanelDescriptor::Pods {
                        cluster_context: "kind-dev".into(),
                        namespace: NamespaceScope::All,
                        filter: String::new(),
                        sort: SortState {
                            column: "name".into(),
                            ascending: true,
                        },
                    },
                    PanelDescriptor::Pods {
                        cluster_context: "staging".into(),
                        namespace: NamespaceScope::All,
                        filter: String::new(),
                        sort: SortState {
                            column: "name".into(),
                            ascending: true,
                        },
                    },
                ],
                ..WindowLayout::default()
            }],
        };
        let text = toml::to_string(&config).unwrap();
        let parsed: WorkspaceConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
        assert_eq!(
            parsed.windows[0].contexts,
            vec!["kind-dev".to_string(), "staging".to_string()]
        );
    }

    /// Tasks.md 2.1: a context with no panels of its own still round-trips - a
    /// window can hold a context (`util::shell`'s `ClusterRegistry::hold`) with
    /// nothing open for it yet, since `contexts` and `panels` name independent
    /// facts about the window.
    #[test]
    fn round_trips_a_window_whose_second_context_has_no_panels() {
        let config = WorkspaceConfig {
            windows: vec![WindowLayout {
                contexts: vec!["kind-dev".into(), "staging".into()],
                panels: vec![PanelDescriptor::Pods {
                    cluster_context: "kind-dev".into(),
                    namespace: NamespaceScope::All,
                    filter: String::new(),
                    sort: SortState {
                        column: "name".into(),
                        ascending: true,
                    },
                }],
                ..WindowLayout::default()
            }],
        };
        let text = toml::to_string(&config).unwrap();
        let parsed: WorkspaceConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
        assert_eq!(parsed.windows[0].contexts.len(), 2);
        assert_eq!(parsed.windows[0].panels.len(), 1);
    }

    /// Tasks.md 2.1: a file written before `contexts` existed has no such key at
    /// all - it must still parse, with `contexts` reading as empty rather than
    /// failing the whole file. Deriving a usable list from `panels` for this case
    /// is `util::shell::restored_contexts`'s job, not this type's.
    #[test]
    fn a_legacy_file_with_no_contexts_field_parses_with_an_empty_list() {
        let toml_text = r#"
            [[windows]]
            width = 1024.0
            height = 768.0

            [[windows.panels]]
            kind = "pods"
            cluster_context = "kind-dev"
            namespace = "all"
            filter = ""

            [windows.panels.sort]
            column = "name"
            ascending = true
        "#;
        let parsed: WorkspaceConfig = toml::from_str(toml_text).unwrap();
        assert!(parsed.windows[0].contexts.is_empty());
        assert_eq!(parsed.windows[0].panels.len(), 1);
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
