//! A command tunnel's command line, split into arguments: POSIX quoting, a
//! backslash before a line break joins the lines, and nothing is handed to a shell -
//! so pipes, redirections and `$VAR` mean nothing here. `{port}` is substituted per
//! argument *after* splitting, so a port value can never change how the line splits.

/// The placeholder replaced with the tunnel's local port.
pub(crate) const PORT_PLACEHOLDER: &str = "{port}";

/// Why a command line can't be split into arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgvError {
    /// Nothing but whitespace.
    Empty,
    /// An unclosed quote, or a trailing backslash with nothing after it.
    UnbalancedQuotes,
}

/// Splits `command_line` into arguments, joining backslash-continued lines first -
/// a command pasted from a terminal over several lines is one command.
pub(crate) fn split(command_line: &str) -> Result<Vec<String>, ArgvError> {
    let joined = command_line.replace("\\\r\n", "").replace("\\\n", "");
    let args = shlex::split(&joined).ok_or(ArgvError::UnbalancedQuotes)?;
    if args.is_empty() {
        return Err(ArgvError::Empty);
    }
    Ok(args)
}

/// Whether any argument carries the `{port}` placeholder.
pub(crate) fn has_port_placeholder(args: &[String]) -> bool {
    args.iter().any(|arg| arg.contains(PORT_PLACEHOLDER))
}

/// `args` with every `{port}` replaced by `port`.
// UNWIRED(#126): the command transport (section 2) is the first caller.
#[allow(dead_code)]
pub(crate) fn substitute_port(args: &[String], port: u16) -> Vec<String> {
    let port = port.to_string();
    args.iter()
        .map(|arg| arg.replace(PORT_PLACEHOLDER, &port))
        .collect()
}

#[cfg(test)]
mod tests;
