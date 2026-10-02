//! Section 3.4 coverage: dropping the transport kills the whole process
//! group, including any `-J` jump-host hop - not just the direct `ssh`
//! child - so no hop process survives as an orphan.

use std::net::TcpListener;
use std::process::{Command as StdCommand, Stdio};
use std::time::Duration;

use crate::forward::supervisor::ForwardTransport;
use crate::tunnel::ssh::SshTransport;

use super::fixture::{LocalSshd, spawn_echo_server, which};

/// True if any process is still alive in the process group rooted at `pid` (the
/// direct `ssh` child's pid, which is also its own pgid since `connect` spawns it
/// with `process_group(0)`). A `-J` jump-host hop inherits that group rather than
/// calling `setpgid` itself, so this catches it too.
fn process_group_has_members(pid: u32) -> bool {
    StdCommand::new("pgrep")
        .arg("-g")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Section 3.4: dropping the transport must kill the *whole* process group, not just
/// the direct `ssh` child - otherwise a `-J` hop (a separate `ssh` subprocess that
/// inherits the group rather than detaching) survives as an orphan. Establishes a
/// forward through a jump-host chain exactly like the test above, then drops the
/// transport and asserts `pgrep -g <pid>` finds nobody left in that group.
#[tokio::test]
async fn dropping_the_transport_kills_the_whole_process_group_including_the_jump_hop() {
    let Some(jump) = LocalSshd::spawn() else {
        eprintln!("skipping: no local sshd/ssh-keygen available to spawn a test jump host");
        return;
    };
    let Some(target) = LocalSshd::spawn_with_client_key(Some(&jump.client_key)) else {
        eprintln!("skipping: failed to spawn the chained target sshd");
        return;
    };
    let Some(_pgrep) = which("pgrep") else {
        eprintln!("skipping: no pgrep available to inspect the process group");
        return;
    };

    let jump_user = std::env::var("USER").unwrap();
    let ssh_config_file = jump.dir.join("jump_ssh_config");
    std::fs::write(
        &ssh_config_file,
        format!(
            "Host jumphop\n\
             \tHostName 127.0.0.1\n\
             \tPort {}\n\
             \tUser {jump_user}\n\
             \tIdentityFile {}\n\
             \tUserKnownHostsFile {}\n\
             \tStrictHostKeyChecking yes\n\
             \tBatchMode yes\n",
            jump.port,
            jump.client_key.display(),
            jump.known_hosts.display(),
        ),
    )
    .unwrap();

    let echo_port = spawn_echo_server().await;
    let local_port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let config = target.config_via(
        &["jumphop".to_string()],
        echo_port,
        local_port,
        &target.known_hosts,
        Some(&ssh_config_file),
    );
    let mut transport = SshTransport::new(config);
    transport
        .connect()
        .await
        .expect("ssh forward should come up through the jump host chain");
    let pid = transport
        .child_pid()
        .expect("connect should have recorded the spawned child's pid");

    // Give the -J hop's own ssh subprocess a moment to actually be running under the
    // same group before we check for it - connect() only proves the local port is
    // accepting connections, not that the hop process has finished forking.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        process_group_has_members(pid),
        "expected ssh (and its jump-host hop) to still be running before drop"
    );

    drop(transport);
    // kill_process_group shells out to `kill`; give the signal a moment to land.
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert!(
        !process_group_has_members(pid),
        "no member of ssh's process group (including the -J hop) should survive Drop"
    );
}
