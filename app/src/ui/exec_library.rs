//! The exec library's UI (`exec-command-library`): the app-wide store of saved
//! commands ([`store`]); the Exec Library window ([`window`]) and the entry
//! dialog ([`dialog`]) behind its commands ([`commands`]); and running an entry -
//! resolving its pods, asking which when several match ([`chooser`]) - in the
//! shell panel ([`run`]). The entries themselves are `config::exec_library`'s.

pub(crate) mod chooser;
pub(crate) mod commands;
pub(crate) mod dialog;
pub(crate) mod resource;
pub(crate) mod run;
pub(crate) mod store;
pub(crate) mod window;

#[cfg(test)]
pub(crate) mod test_support;
