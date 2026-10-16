//! The exec library (`exec-command-library`): saved in-pod commands in
//! `exec-library.toml` in the preference directory, a file the user may edit by
//! hand, like `namespace-sets.toml`.
//!
//! Entries are an ordered list, kept in the order they were made. An entry's
//! name is its identity, unique ignoring case and surrounding spaces. Its
//! command is split into arguments with POSIX quoting and run without a shell
//! (D2), so a command that doesn't split is refused here, before it is saved.
//! What an entry targets, and how that becomes pods, is [`Target`]'s; running it
//! is `ui::exec_library`'s.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Every saved entry, in the order they were made.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecLibrary {
    pub entries: Vec<Entry>,
}

/// One saved command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub command: String,
    /// The container to run in; `None` takes the pod's default container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    pub target: Target,
}

/// Which pods an entry runs in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    /// Whichever pod it is run from.
    Any,
    /// The pods in `namespace` - every namespace when `None` - that `selector`,
    /// in `kubectl -l` syntax, matches. An empty selector matches every pod.
    Selector {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        #[serde(default)]
        selector: String,
    },
    /// The pods a workload or Service selects, as `kubectl exec deploy/x`
    /// resolves them.
    Workload {
        namespace: String,
        workload_kind: WorkloadKind,
        name: String,
    },
}

/// The kinds a workload target can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkloadKind {
    Deployment,
    StatefulSet,
    DaemonSet,
    Service,
}

impl WorkloadKind {
    pub const ALL: [Self; 4] = [
        Self::Deployment,
        Self::StatefulSet,
        Self::DaemonSet,
        Self::Service,
    ];

    /// The Kind's name, as the API spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deployment => "Deployment",
            Self::StatefulSet => "StatefulSet",
            Self::DaemonSet => "DaemonSet",
            Self::Service => "Service",
        }
    }

    /// The API group: `apps`, or the core group for a Service.
    pub fn group(self) -> &'static str {
        match self {
            Self::Service => "",
            _ => "apps",
        }
    }

    /// The kind named `kind` in `group`, if a workload target can name it.
    pub fn of(group: &str, kind: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.group() == group && candidate.as_str() == kind)
    }
}

/// The dialog field an [`EntryError`] belongs beside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Command,
    Namespace,
    Selector,
    WorkloadName,
}

/// Why an entry can't be saved as asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryError {
    /// The name is empty, or only spaces.
    EmptyName,
    /// Another entry already has the name, ignoring case and spaces.
    NameTaken,
    /// The command is empty, or only spaces.
    EmptyCommand,
    /// The command has a quote that is never closed.
    UnbalancedQuotes,
    /// The selector doesn't parse; the parser's own explanation.
    Selector(String),
    /// A workload target has no namespace.
    NoNamespace,
    /// A workload target has no name.
    NoWorkloadName,
    /// No entry has the name.
    NotFound,
}

impl EntryError {
    /// The field to show the error beside; `None` for one that belongs to no
    /// field (`NotFound`).
    pub fn field(&self) -> Option<Field> {
        match self {
            Self::EmptyName | Self::NameTaken => Some(Field::Name),
            Self::EmptyCommand | Self::UnbalancedQuotes => Some(Field::Command),
            Self::Selector(_) => Some(Field::Selector),
            Self::NoNamespace => Some(Field::Namespace),
            Self::NoWorkloadName => Some(Field::WorkloadName),
            Self::NotFound => None,
        }
    }
}

impl fmt::Display for EntryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => f.write_str("An entry needs a name."),
            Self::NameTaken => f.write_str("That name is taken."),
            Self::EmptyCommand => f.write_str("An entry needs a command."),
            Self::UnbalancedQuotes => f.write_str("The command has a quote that is never closed."),
            Self::Selector(reason) => write!(f, "The selector doesn't parse: {reason}"),
            Self::NoNamespace => f.write_str("A workload target needs a namespace."),
            Self::NoWorkloadName => f.write_str("A workload target needs a name."),
            Self::NotFound => f.write_str("There is no entry with that name."),
        }
    }
}

impl std::error::Error for EntryError {}

/// `command` as the argv it runs: POSIX-quoted words, no shell.
pub fn split_command(command: &str) -> Result<Vec<String>, EntryError> {
    if command.trim().is_empty() {
        return Err(EntryError::EmptyCommand);
    }
    let argv = shlex::split(command).ok_or(EntryError::UnbalancedQuotes)?;
    if argv.is_empty() {
        return Err(EntryError::EmptyCommand);
    }
    Ok(argv)
}

/// How two names compare for uniqueness: trimmed, ignoring case.
fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

impl Entry {
    /// The command's argv - only an entry that [`ExecLibrary::create`]
    /// accepted is sure to split.
    pub fn argv(&self) -> Result<Vec<String>, EntryError> {
        split_command(&self.command)
    }

    /// `self` checked and tidied: name, command, namespace, selector and
    /// container trimmed, an empty container and namespace dropped.
    fn checked(mut self) -> Result<Self, EntryError> {
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            return Err(EntryError::EmptyName);
        }
        self.command = self.command.trim().to_string();
        split_command(&self.command)?;
        self.container = self
            .container
            .map(|container| container.trim().to_string())
            .filter(|container| !container.is_empty());
        self.target = match self.target {
            Target::Any => Target::Any,
            Target::Selector {
                namespace,
                selector,
            } => {
                let selector = selector.trim().to_string();
                crate::k8s::label_selector::parse(&selector)
                    .map_err(|error| EntryError::Selector(error.to_string()))?;
                Target::Selector {
                    namespace: namespace
                        .map(|namespace| namespace.trim().to_string())
                        .filter(|namespace| !namespace.is_empty()),
                    selector,
                }
            }
            Target::Workload {
                namespace,
                workload_kind,
                name,
            } => {
                let (namespace, name) = (namespace.trim().to_string(), name.trim().to_string());
                if namespace.is_empty() {
                    return Err(EntryError::NoNamespace);
                }
                if name.is_empty() {
                    return Err(EntryError::NoWorkloadName);
                }
                Target::Workload {
                    namespace,
                    workload_kind,
                    name,
                }
            }
        };
        Ok(self)
    }
}

impl ExecLibrary {
    /// The entry named `name`, ignoring case and surrounding spaces.
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|entry| same_name(&entry.name, name))
    }

    /// Checks `entry` as [`Self::create`] would, without saving it - what the
    /// dialog shows inline. `editing` is the name of the entry being edited,
    /// whose own name doesn't count as taken.
    pub fn check(&self, entry: Entry, editing: Option<&str>) -> Result<Entry, EntryError> {
        let entry = entry.checked()?;
        let taken = self.entries.iter().any(|held| {
            same_name(&held.name, &entry.name)
                && !editing.is_some_and(|editing| same_name(&held.name, editing))
        });
        if taken {
            return Err(EntryError::NameTaken);
        }
        Ok(entry)
    }

    /// Adds `entry` after the others.
    pub fn create(&mut self, entry: Entry) -> Result<(), EntryError> {
        let entry = self.check(entry, None)?;
        self.entries.push(entry);
        Ok(())
    }

    /// Replaces the entry named `name` with `entry`, keeping its place.
    pub fn update(&mut self, name: &str, entry: Entry) -> Result<(), EntryError> {
        let entry = self.check(entry, Some(name))?;
        let held = self
            .entries
            .iter_mut()
            .find(|held| same_name(&held.name, name))
            .ok_or(EntryError::NotFound)?;
        *held = entry;
        Ok(())
    }

    /// Copies the entry named `name` to just after it, as "<name> copy" (or
    /// "<name> copy 2", and so on, past any taken), and returns the copy's name.
    pub fn duplicate(&mut self, name: &str) -> Result<String, EntryError> {
        let index = self
            .entries
            .iter()
            .position(|held| same_name(&held.name, name))
            .ok_or(EntryError::NotFound)?;
        let base = format!("{} copy", self.entries[index].name);
        let copy_name = (1..)
            .map(|n| {
                if n == 1 {
                    base.clone()
                } else {
                    format!("{base} {n}")
                }
            })
            .find(|candidate| self.find(candidate).is_none())
            .expect("an untaken name is always found");
        let mut copy = self.entries[index].clone();
        copy.name = copy_name.clone();
        self.entries.insert(index + 1, copy);
        Ok(copy_name)
    }

    /// Deletes the entry named `name`; whether there was one.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|held| !same_name(&held.name, name));
        self.entries.len() != before
    }
}

#[cfg(test)]
mod tests;
