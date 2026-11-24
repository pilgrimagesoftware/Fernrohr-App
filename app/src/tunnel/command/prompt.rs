//! Telling an interactive prompt apart from ordinary output (#423). A command
//! tunnel's command has no one to answer it: stdin is a pipe Fernrohr holds open
//! and never writes to. A vendor CLI that stops to ask for a password, a key's
//! passphrase, a verification code or a yes/no answer therefore waits forever, and
//! the tunnel never comes up. What it last printed says so, if it looks like a
//! question about credentials or confirmation.
//!
//! Deliberately narrow: the text has to end the way a prompt does *and* name what
//! it asks for, so a log line that merely mentions a password isn't taken for one.
//! A prompt written straight to the terminal (`/dev/tty`) never reaches the
//! command's output; the startup timeout catches that case instead.

/// What a credentials or confirmation prompt asks for, lowercase.
const PROMPT_WORDS: &[&str] = &[
    "password",
    "passphrase",
    "passcode",
    "verification code",
    "security key",
    "one-time",
    "authenticator",
    "(y/n)",
    "[y/n]",
    "yes/no",
    "do you want to continue",
];

/// Whether `text`, the last thing a command printed, is it asking for input: it
/// ends as a prompt does (`:`, `?`, `)` or `]`, trailing spaces aside) and names a
/// credential or a confirmation.
pub(super) fn looks_like_prompt(text: &str) -> bool {
    let text = text.trim();
    if !text.ends_with([':', '?', ')', ']']) {
        return false;
    }
    let lower = text.to_lowercase();
    PROMPT_WORDS.iter().any(|word| lower.contains(word))
}

#[cfg(test)]
mod tests {
    use super::looks_like_prompt;

    #[test]
    fn credential_and_confirmation_prompts_are_prompts() {
        for prompt in [
            "Enter passphrase for key '/home/user/.ssh/id_ed25519': ",
            "user@bastion.example.com's password: ",
            "Please enter your password:",
            "Enter verification code: ",
            "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
            "Do you want to continue (Y/n)?  ",
            "Overwrite existing key [y/N]",
            "Confirm user presence for key ED25519-SK (security key):",
        ] {
            assert!(looks_like_prompt(prompt), "{prompt:?}");
        }
    }

    #[test]
    fn ordinary_output_is_not_a_prompt() {
        for line in [
            "",
            "Testing if tunnel connection works.",
            "Listening on port [12345].",
            "Updating project ssh metadata...done.",
            "Using password authentication.",
            "Permission denied (publickey,password).",
            "Waiting for connection:",
            "Enter passphrase for key, then",
        ] {
            assert!(!looks_like_prompt(line), "{line:?}");
        }
    }
}
