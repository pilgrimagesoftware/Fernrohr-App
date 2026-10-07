//! #178: a kubeconfig's bare exec plugin is found on the login shell's `PATH`
//! though the process's own `PATH` lacks it - as a Dock-launched app's does.

use super::use_login_path;
use crate::k8s::cluster::mock_api::MockApi;
use kube::config::{KubeConfigOptions, Kubeconfig};
use std::path::{Path, PathBuf};

const PLUGIN: &str = "fernrohr-test-auth-plugin";

/// A directory - on no `PATH` but the one a test hands over - holding an
/// executable plugin that answers with a token and leaves a mark when run.
fn plugin_dir() -> PathBuf {
    let dir = crate::util::test_paths::temp_path("exec-plugin");
    std::fs::create_dir_all(&dir).unwrap();
    let plugin = dir.join(PLUGIN);
    std::fs::write(
        &plugin,
        format!(
            "#!/bin/sh\n: > '{}/ran'\n\
             echo '{{\"apiVersion\":\"client.authentication.k8s.io/v1beta1\",\
             \"kind\":\"ExecCredential\",\"status\":{{\"token\":\"t\"}}}}'\n",
            dir.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&plugin, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

/// A kubeconfig for `server` whose user authenticates with the bare `PLUGIN`.
fn kubeconfig(server: &str, env: &str) -> Kubeconfig {
    Kubeconfig::from_yaml(&format!(
        "apiVersion: v1\nkind: Config\ncurrent-context: dev\n\
         clusters:\n- name: dev\n  cluster:\n    server: {server}\n\
         contexts:\n- name: dev\n  context:\n    cluster: dev\n    user: dev\n\
         users:\n- name: dev\n  user:\n    exec:\n      \
         apiVersion: client.authentication.k8s.io/v1beta1\n      \
         command: {PLUGIN}\n      interactiveMode: Never\n{env}"
    ))
    .expect("a kubeconfig")
}

async fn config(kubeconfig: Kubeconfig) -> kube::Config {
    kube::Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default())
        .await
        .expect("a config")
}

fn exec_path_var(config: &kube::Config) -> Option<String> {
    config
        .auth_info
        .exec
        .as_ref()?
        .env
        .iter()
        .flatten()
        .find(|var| var.get("name").map(String::as_str) == Some("PATH"))?
        .get("value")
        .cloned()
}

#[tokio::test]
async fn a_bare_plugin_on_the_login_path_is_found_and_runs_with_it() {
    let dir = plugin_dir();
    let login_path = format!("/nonexistent:{}:/usr/bin:/bin", dir.display());
    let mut config = config(kubeconfig("http://127.0.0.1:1", "")).await;

    use_login_path(&mut config, &login_path);

    let exec = config.auth_info.exec.as_ref().unwrap();
    assert_eq!(
        exec.command.as_deref().map(Path::new),
        Some(dir.join(PLUGIN).as_path())
    );
    assert_eq!(exec_path_var(&config).as_deref(), Some(login_path.as_str()));
}

#[tokio::test]
async fn a_path_the_kubeconfig_sets_for_the_plugin_wins() {
    let dir = plugin_dir();
    let env = "      env:\n      - name: PATH\n        value: /opt/own/bin\n";
    let mut config = config(kubeconfig("http://127.0.0.1:1", env)).await;

    use_login_path(&mut config, &format!("{}", dir.display()));

    assert_eq!(exec_path_var(&config).as_deref(), Some("/opt/own/bin"));
}

#[tokio::test]
async fn a_plugin_not_on_the_login_path_is_left_for_kube() {
    let mut config = config(kubeconfig("http://127.0.0.1:1", "")).await;
    use_login_path(&mut config, "/nonexistent");
    assert_eq!(
        config.auth_info.exec.as_ref().unwrap().command.as_deref(),
        Some(PLUGIN)
    );
}

/// The regression itself: the process's `PATH` lacks the plugin's directory,
/// as a Dock-launched app's does. Without the login `PATH` the client can't
/// run it; with it, a request runs the plugin and gets through.
#[tokio::test]
async fn a_client_runs_a_bare_plugin_found_only_on_the_login_path() {
    let dir = plugin_dir();
    let api = MockApi::start(&[(
        "/version",
        200,
        r#"{"major":"1","minor":"30","gitVersion":"v1.30.0","gitCommit":"","gitTreeState":"","buildDate":"","goVersion":"","compiler":"","platform":""}"#,
    )]);
    let server = format!("http://{}", api.addr);

    // kube runs the plugin as it builds the client - or, in some versions, on
    // the first request: the failure may surface at either.
    let unfound = config(kubeconfig(&server, "")).await;
    let error = match kube::Client::try_from(unfound) {
        Err(error) => format!("{error:?}"),
        Ok(client) => format!(
            "{:?}",
            client
                .apiserver_version()
                .await
                .expect_err("the plugin isn't on the process PATH")
        ),
    };
    assert!(
        error.contains("No such file"),
        "the field report's failure: {error}"
    );
    assert!(!dir.join("ran").exists());

    let mut found = config(kubeconfig(&server, "")).await;
    use_login_path(&mut found, &format!("{}:/usr/bin:/bin", dir.display()));
    let version = kube::Client::try_from(found)
        .unwrap()
        .apiserver_version()
        .await
        .expect("the plugin ran and the request got through");
    assert_eq!(version.git_version, "v1.30.0");
    assert!(dir.join("ran").exists(), "the plugin ran");
}
