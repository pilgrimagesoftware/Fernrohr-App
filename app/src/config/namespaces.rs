//! Named namespace sets (`namespace-sets`): `namespace-sets.toml` in the
//! preference directory, a file the user may edit by hand, like `keymap.toml`.
//!
//! The sets are an ordered list - a set's position is its quick-selection
//! digit, so a map, which would reorder on a rename, won't do. A set's name is
//! its identity and is unique; a set always has at least one namespace, since
//! an empty include set means the same as "all namespaces". Namespaces are kept
//! sorted and unique, the same rule as a panel's scope, so matching a panel's
//! scope to a set is a plain comparison.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Every saved set, in the order they were made.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NamespaceSetsConfig {
    pub sets: Vec<NamespaceSetConfig>,
}

/// One named set of namespaces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceSetConfig {
    pub name: String,
    pub namespaces: Vec<String>,
}

/// Why a set can't be saved as asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetError {
    /// The name is empty, or only spaces.
    EmptyName,
    /// Another set already has the name.
    NameTaken,
    /// The set would have no namespaces.
    NoNamespaces,
    /// No set has the name.
    NotFound,
}

impl fmt::Display for SetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SetError::EmptyName => "A set needs a name.",
            SetError::NameTaken => "That name is taken.",
            SetError::NoNamespaces => "A set needs at least one namespace.",
            SetError::NotFound => "There is no set with that name.",
        })
    }
}

impl std::error::Error for SetError {}

/// `namespaces` sorted and without repeats - a panel scope's own order.
fn normalized(mut namespaces: Vec<String>) -> Vec<String> {
    namespaces.sort_unstable();
    namespaces.dedup();
    namespaces
}

impl NamespaceSetsConfig {
    /// The set named `name`.
    pub fn find(&self, name: &str) -> Option<&NamespaceSetConfig> {
        self.sets.iter().find(|set| set.name == name)
    }

    /// Adds a set named `name` holding `namespaces`, after the others.
    pub fn create(&mut self, name: &str, namespaces: Vec<String>) -> Result<(), SetError> {
        let name = self.check_name(name, None)?;
        let namespaces = normalized(namespaces);
        if namespaces.is_empty() {
            return Err(SetError::NoNamespaces);
        }
        self.sets.push(NamespaceSetConfig { name, namespaces });
        Ok(())
    }

    /// Renames the set `from` to `to`, keeping its place.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), SetError> {
        let to = self.check_name(to, Some(from))?;
        let set = self.find_mut(from)?;
        set.name = to;
        Ok(())
    }

    /// Adds `namespace` to the set `name`, or removes it. A set's last
    /// namespace can't be removed.
    pub fn apply_to(&mut self, name: &str, namespace: &str, add: bool) -> Result<(), SetError> {
        let set = self.find_mut(name)?;
        let mut namespaces = set.namespaces.clone();
        if add {
            namespaces.push(namespace.to_string());
        } else {
            namespaces.retain(|held| held != namespace);
        }
        let namespaces = normalized(namespaces);
        if namespaces.is_empty() {
            return Err(SetError::NoNamespaces);
        }
        set.namespaces = namespaces;
        Ok(())
    }

    /// Deletes the set `name`; whether there was one.
    pub fn remove_set(&mut self, name: &str) -> bool {
        let before = self.sets.len();
        self.sets.retain(|set| set.name != name);
        self.sets.len() != before
    }

    /// The set holding exactly `namespaces`, in any order - what names a
    /// panel's scope in its title bar. "All namespaces" (empty) matches none.
    pub fn matching(&self, namespaces: &[String]) -> Option<&NamespaceSetConfig> {
        let namespaces = normalized(namespaces.to_vec());
        if namespaces.is_empty() {
            return None;
        }
        self.sets.iter().find(|set| set.namespaces == namespaces)
    }

    fn find_mut(&mut self, name: &str) -> Result<&mut NamespaceSetConfig, SetError> {
        self.sets
            .iter_mut()
            .find(|set| set.name == name)
            .ok_or(SetError::NotFound)
    }

    /// `name` trimmed, if it's non-empty and no set but `except` has it.
    fn check_name(&self, name: &str, except: Option<&str>) -> Result<String, SetError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(SetError::EmptyName);
        }
        let taken = self
            .sets
            .iter()
            .any(|set| set.name == name && Some(set.name.as_str()) != except);
        if taken {
            return Err(SetError::NameTaken);
        }
        Ok(name.to_string())
    }
}

#[cfg(test)]
mod tests;
