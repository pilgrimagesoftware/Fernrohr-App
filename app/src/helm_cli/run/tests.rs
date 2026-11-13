//! 7.3: `helm` run with the expected arguments and environment, a failure
//! reported with stderr, a timeout killing the whole group, and a second run
//! on the same release refused.

use super::{HelmCommand, HelmFailure, ReleaseKey, RunSlots, run};
use crate::helm_cli::test_helm::{script, temp_dir};
use std::path::Path;
use std::time::{Duration, Instant};

fn command(helm: &Path, dir: &Path, timeout: Duration) -> HelmCommand {
    HelmCommand {
        binary: helm.to_path_buf(),
        args: ["rollback", "web", "3", "-n", "shop"]
            .map(String::from)
            .to_vec(),
        kubeconfig: dir.join("run.yaml"),
        context: "demo".into(),
        path_env: "/usr/bin:/bin".into(),
        timeout,
    }
}

#[tokio::test]
async fn helm_gets_the_action_the_kubeconfig_and_the_context() {
    let dir = temp_dir("run-args");
    let seen = dir.join("seen");
    let helm = script(
        &dir,
        "helm",
        &format!(
            "printf '%s\\n' \"$@\" > '{seen}'\necho \"KUBECONFIG=$KUBECONFIG\" >> '{seen}'\necho done",
            seen = seen.display()
        ),
    );
    let output = run(command(&helm, &dir, Duration::from_secs(30)))
        .await
        .expect("it succeeds");
    assert_eq!(output.stdout.trim(), "done");
    let kubeconfig = dir.join("run.yaml").display().to_string();
    let seen = std::fs::read_to_string(&seen).unwrap();
    let lines: Vec<&str> = seen.lines().collect();
    assert_eq!(
        lines,
        [
            "rollback",
            "web",
            "3",
            "-n",
            "shop",
            "--kubeconfig",
            kubeconfig.as_str(),
            "--kube-context",
            "demo",
            &format!("KUBECONFIG={kubeconfig}"),
        ]
    );
}

#[tokio::test]
async fn a_failure_reports_its_status_and_stderr() {
    let dir = temp_dir("run-fail");
    let helm = script(
        &dir,
        "helm",
        "echo 'some progress'\necho 'Error: release: not found' >&2\nexit 1",
    );
    let failure = run(command(&helm, &dir, Duration::from_secs(30)))
        .await
        .expect_err("it fails");
    assert_eq!(
        failure,
        HelmFailure::Exit {
            code: Some(1),
            message: "Error: release: not found".into(),
        }
    );
    assert_eq!(
        failure.to_string(),
        "helm exited with status 1: Error: release: not found"
    );
}

/// Past its timeout the run is stopped - and so is what `helm` started, a
/// hook's process here, since the whole group goes.
#[tokio::test]
async fn a_timeout_kills_the_process_group() {
    let dir = temp_dir("run-timeout");
    let child_pid = dir.join("child.pid");
    let helm = script(
        &dir,
        "helm",
        &format!("sleep 30 &\necho $! > '{}'\nwait", child_pid.display()),
    );
    // Long enough that even a loaded machine starts the script and writes
    // its child's pid before the timeout; the run still ends at it.
    let timeout = Duration::from_secs(8);
    let started = Instant::now();
    let failure = run(command(&helm, &dir, timeout))
        .await
        .expect_err("it times out");
    assert_eq!(failure, HelmFailure::TimedOut(timeout));
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "it didn't wait out the script's 30 s"
    );

    let pid = std::fs::read_to_string(&child_pid)
        .unwrap()
        .trim()
        .to_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(&pid) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!alive(&pid), "the hook's process went with the group");
}

/// A run dropped mid-way - the app quitting - takes `helm`'s whole group
/// with it, not just `helm`.
#[tokio::test]
async fn a_dropped_run_kills_the_process_group() {
    let dir = temp_dir("run-dropped");
    let child_pid = dir.join("child.pid");
    let helm = script(
        &dir,
        "helm",
        &format!("sleep 30 &\necho $! > '{}'\nwait", child_pid.display()),
    );
    let running = tokio::spawn(run(command(&helm, &dir, Duration::from_secs(60))));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !child_pid.exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Let the shell finish writing the pid.
    tokio::time::sleep(Duration::from_millis(100)).await;
    running.abort();
    let _ = running.await;

    let pid = std::fs::read_to_string(&child_pid)
        .unwrap()
        .trim()
        .to_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(&pid) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!alive(&pid), "the hook's process went with the group");
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn a_second_run_on_a_release_is_refused_until_the_first_ends() {
    let slots = RunSlots::default();
    let key = |release: &str| ReleaseKey {
        context: "demo".into(),
        namespace: "shop".into(),
        release: release.into(),
    };
    let first = slots.claim(key("web")).expect("free");
    assert_eq!(
        slots.claim(key("web")).err(),
        Some(HelmFailure::AlreadyRunning)
    );
    let _other = slots
        .claim(key("api"))
        .expect("another release runs alongside");
    drop(first);
    assert!(slots.claim(key("web")).is_ok(), "free again once it ends");
}
