//! The exec library's UI (`exec-command-library`): the app-wide store of saved
//! commands ([`store`]), running an entry - resolving its pods, asking which
//! when several match ([`chooser`]) - in the shell panel ([`run`]). The entries
//! themselves are `config::exec_library`'s.

pub(crate) mod chooser;
pub(crate) mod run;
pub(crate) mod store;
