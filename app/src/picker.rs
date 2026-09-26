//! The cluster picker: shown in place of a window's panel workspace whenever that window
//! has no open or restored panels. Lists kubeconfig contexts, drives a connection through
//! `ClusterRegistry`, and emits [`PickerEvent::Connected`] on success so `shell::MainWindow`
//! can switch that window into its normal panel workspace.

use crate::cluster::connection::{ClusterConnection, ConnectionState};
use crate::cluster::kubeconfig;
use crate::cluster::session::ClusterRegistry;
use gpui_kit::component::button::Button;
use gpui_kit::*;

pub enum PickerEvent {
    Connected { context_name: String },
}

/// One attempt in flight: which context was picked and its connection entity, so the
/// picker can render `Connecting`/`WaitingForTunnel`/`Failed` and let the user retry.
struct Attempt {
    context_name: String,
    connection: Entity<ClusterConnection>,
}

pub struct ClusterPicker {
    contexts: Result<Vec<String>, String>,
    attempt: Option<Attempt>,
    focus_handle: FocusHandle,
}

impl ClusterPicker {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            contexts: kubeconfig::list_context_names(None).map_err(|error| error.to_string()),
            attempt: None,
            focus_handle: cx.focus_handle(),
        }
    }

    fn select(&mut self, context_name: String, cx: &mut Context<Self>) {
        let connection = ClusterRegistry::connection(cx, &context_name);
        cx.observe(&connection, {
            let context_name = context_name.clone();
            move |_this: &mut Self, connection, cx| {
                if let ConnectionState::Connected(_) = &connection.read(cx).state {
                    cx.emit(PickerEvent::Connected {
                        context_name: context_name.clone(),
                    });
                }
                cx.notify();
            }
        })
        .detach();
        self.attempt = Some(Attempt {
            context_name,
            connection,
        });
        cx.notify();
    }
}

impl EventEmitter<PickerEvent> for ClusterPicker {}

impl Focusable for ClusterPicker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ClusterPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let contexts = match &self.contexts {
            Ok(contexts) if !contexts.is_empty() => contexts.clone(),
            Ok(_) => {
                return div()
                    .size_full()
                    .child("No kubeconfig contexts are available.");
            }
            Err(error) => {
                return div()
                    .size_full()
                    .child(format!("Could not read kubeconfig: {error}"));
            }
        };

        let status = self.attempt.as_ref().map(|attempt| {
            let context_name = attempt.context_name.clone();
            match &attempt.connection.read(cx).state {
                ConnectionState::Connecting => {
                    div().child(format!("Connecting to {context_name}..."))
                }
                ConnectionState::WaitingForTunnel => {
                    div().child(format!("Waiting for tunnel to {context_name}..."))
                }
                ConnectionState::Failed(reason) => {
                    div().child(format!("Could not connect to {context_name}: {reason}"))
                }
                ConnectionState::Connected(_) => {
                    div().child(format!("Connected to {context_name}"))
                }
            }
        });

        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .child("Select a cluster")
            .children(status)
            .children(contexts.into_iter().map(|context_name| {
                Button::new(SharedString::from(context_name.clone()))
                    .label(context_name.clone())
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.select(context_name.clone(), cx);
                    }))
            }))
    }
}

// `use super::*` here would re-import `gpui_kit`'s `test` attribute macro (this file's
// `use gpui_kit::*` brings it in), shadowing the builtin `#[test]` and sending a plain
// sync test into `#[gpui_kit::test]`'s async-runtime expansion instead - hence the
// explicit imports below rather than a glob.
#[cfg(test)]
mod tests {
    use crate::cluster::kubeconfig;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    const FIXTURE: &str = r#"
apiVersion: v1
kind: Config
clusters:
  - name: kind-dev
    cluster:
      server: https://127.0.0.1:6443
contexts:
  - name: kind-dev
    context:
      cluster: kind-dev
      user: kind-dev
current-context: kind-dev
users:
  - name: kind-dev
    user: {}
"#;

    fn fixture_path() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fernrohr-picker-fixture-{n}.yaml"));
        fs::write(&path, FIXTURE).unwrap();
        path
    }

    #[test]
    fn lists_contexts_from_a_valid_kubeconfig() {
        let path = fixture_path();
        let contexts = kubeconfig::list_context_names(Some(&path)).unwrap();
        assert_eq!(contexts, vec!["kind-dev".to_string()]);
    }

    #[test]
    fn missing_kubeconfig_is_reported_as_an_error() {
        let missing = std::env::temp_dir().join("fernrohr-picker-fixture-does-not-exist.yaml");
        let result = kubeconfig::list_context_names(Some(&missing));
        assert!(result.is_err());
    }
}
