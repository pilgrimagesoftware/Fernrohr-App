//! Which container a shell panel is in, and - for a library entry
//! (`exec-command-library` D1) - the command it runs there in place of the
//! interactive shell. Saved with the panel, so a restored panel names what it
//! ran; a restored panel never runs it again.

use serde_json::{Value, json};

/// A saved library entry's command, as it was when it was run.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LibraryCommand {
    /// The entry's name, for the panel's title.
    pub entry_name: String,
    /// The command split into arguments: run as is, with no shell.
    pub argv: Vec<String>,
}

/// Which container a session is in, and what it runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExecTarget {
    pub namespace: String,
    pub pod: String,
    pub container: String,
    /// A library entry's command, or `None` for the interactive shell.
    pub command: Option<LibraryCommand>,
}

/// The shell a plain session starts: bash where the image has it, else sh.
pub(super) const SHELL: [&str; 3] = [
    "/bin/sh",
    "-c",
    "command -v bash >/dev/null 2>&1 && exec bash || exec sh",
];

impl ExecTarget {
    /// The argv the session attaches: the library command's, else [`SHELL`].
    pub(crate) fn argv(&self) -> Vec<String> {
        match &self.command {
            Some(command) => command.argv.clone(),
            None => SHELL.iter().map(|arg| arg.to_string()).collect(),
        }
    }

    /// The panel title's text: "Shell" or the entry's name, then the pod and
    /// container.
    pub(crate) fn title(&self) -> String {
        let what = self
            .command
            .as_ref()
            .map_or("Shell", |command| command.entry_name.as_str());
        format!("{what}: {} · {}", self.pod, self.container)
    }

    /// The target's fields of a saved panel's state.
    pub(crate) fn saved(&self) -> Value {
        let mut saved = json!({
            "namespace": self.namespace,
            "pod": self.pod,
            "container": self.container,
        });
        if let Some(command) = &self.command {
            saved["command"] = json!({ "entry_name": command.entry_name, "argv": command.argv });
        }
        saved
    }

    /// The target a saved panel's `state` names, or `None` when it doesn't.
    pub(crate) fn from_saved(state: &Value) -> Option<Self> {
        let command = match state.get("command") {
            None | Some(Value::Null) => None,
            Some(command) => Some(LibraryCommand {
                entry_name: command["entry_name"].as_str()?.to_string(),
                argv: serde_json::from_value(command["argv"].clone()).ok()?,
            }),
        };
        Some(Self {
            namespace: state["namespace"].as_str()?.to_string(),
            pod: state["pod"].as_str()?.to_string(),
            container: state["container"].as_str()?.to_string(),
            command,
        })
    }
}
