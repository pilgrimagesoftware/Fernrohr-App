//! 7.1: `helm` found on the PATH or at the Settings path, accepted only as
//! Helm 3, and every refusal saying why.

use super::locate;
use crate::helm_cli::test_helm::{helm_reporting, script, temp_dir};

#[tokio::test]
async fn helm_3_on_the_path_is_found() {
    let dir = temp_dir("on-path");
    let (helm, path_env) = helm_reporting(&dir, "v3.14.2+gc309b6f");
    let found = locate(None, &path_env).await.expect("helm 3 is found");
    assert_eq!(found.path, helm);
    assert_eq!(found.version, "v3.14.2+gc309b6f");
}

#[tokio::test]
async fn the_settings_path_wins_and_needs_no_path() {
    let dir = temp_dir("settings-path");
    let helm = script(&dir, "my-helm", "echo v3.15.0+g1234");
    let found = locate(Some(&helm), "/usr/bin:/bin")
        .await
        .expect("the Settings path's helm 3 is found");
    assert_eq!(found.path, helm);
}

#[tokio::test]
async fn helm_2_is_refused_with_the_reason() {
    let dir = temp_dir("v2");
    // Helm 2 prints its client version, then fails without Tiller.
    script(&dir, "helm", "echo 'Client: v2.17.0+ga690bad'\nexit 1");
    let path_env = format!("{}:/usr/bin:/bin", dir.display());
    let refused = locate(None, &path_env)
        .await
        .expect_err("helm 2 is refused");
    assert!(refused.contains("Helm 3 is required"), "{refused}");
    assert!(
        refused.contains("v2.17.0"),
        "names what it found: {refused}"
    );
}

#[tokio::test]
async fn missing_helm_is_refused_with_the_reason() {
    let dir = temp_dir("missing");
    let refused = locate(None, &dir.display().to_string())
        .await
        .expect_err("no helm on the PATH");
    assert!(
        refused.contains("isn't on your login shell's PATH"),
        "{refused}"
    );

    let nowhere = dir.join("no-such-helm");
    let refused = locate(Some(&nowhere), "/usr/bin:/bin")
        .await
        .expect_err("no helm at the Settings path");
    assert!(refused.contains("There's no helm program at"), "{refused}");
}
