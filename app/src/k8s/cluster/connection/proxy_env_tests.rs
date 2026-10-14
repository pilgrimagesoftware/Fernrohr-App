//! #198: `NO_PROXY` applied to a proxy from the environment, as `kubectl`
//! applies it; never to the kubeconfig's own `proxy-url`.

use super::{apply_no_proxy, bypasses, kubeconfig_proxy, redacted};
use kube::Config;
use kube::config::Kubeconfig;

#[test]
fn no_proxy_entries_match_hosts_and_their_domains() {
    let list = "localhost, .corp.example, example.org:8443, 10.0.0.1, [::1]";
    assert!(bypasses(list, "localhost"));
    assert!(bypasses(list, "api.corp.example"));
    assert!(bypasses(list, "corp.example"));
    assert!(
        bypasses(list, "api.example.org"),
        "a port on an entry is ignored"
    );
    assert!(bypasses(list, "10.0.0.1"));
    assert!(bypasses(list, "[::1]"));
    assert!(bypasses(list, "API.CORP.EXAMPLE"), "case is ignored");
    assert!(!bypasses(list, "corp.example.net"));
    assert!(!bypasses(list, "notcorp.example"));
    assert!(!bypasses(list, "10.0.0.2"));
    assert!(bypasses("*", "anything.example"));
    assert!(!bypasses("", "anything.example"));
}

fn config_with_proxy(server: &str) -> Config {
    let mut config = Config::new(server.parse().unwrap());
    config.proxy_url = Some("http://proxy.example:3128".parse().unwrap());
    config
}

#[test]
fn an_environment_proxy_is_dropped_for_a_no_proxy_host() {
    let mut config = config_with_proxy("https://api.corp.example:6443");
    apply_no_proxy(&mut config, false, Some(".corp.example"));
    assert_eq!(config.proxy_url, None);

    let mut config = config_with_proxy("https://api.elsewhere.example:6443");
    apply_no_proxy(&mut config, false, Some(".corp.example"));
    assert!(config.proxy_url.is_some(), "not named: kept");

    let mut config = config_with_proxy("https://api.corp.example:6443");
    apply_no_proxy(&mut config, false, None);
    assert!(config.proxy_url.is_some(), "no NO_PROXY: kept");
}

#[test]
fn the_kubeconfigs_own_proxy_is_kept_whatever_no_proxy_says() {
    let mut config = config_with_proxy("https://api.corp.example:6443");
    apply_no_proxy(&mut config, true, Some("*"));
    assert!(config.proxy_url.is_some());
}

#[test]
fn the_kubeconfigs_proxy_is_read_off_the_contexts_cluster() {
    let yaml = "apiVersion: v1\nkind: Config\ncurrent-context: dev\nclusters:\n  - name: dev\n    cluster:\n      server: https://api.dev.example\n      proxy-url: http://proxy.example:3128\n  - name: prod\n    cluster:\n      server: https://api.prod.example\ncontexts:\n  - name: dev\n    context:\n      cluster: dev\n      user: me\n  - name: prod\n    context:\n      cluster: prod\n      user: me\nusers:\n  - name: me\n    user: {}\n";
    let kubeconfig = Kubeconfig::from_yaml(yaml).unwrap();
    assert_eq!(
        kubeconfig_proxy(&kubeconfig, None).as_deref(),
        Some("http://proxy.example:3128"),
        "the current context's"
    );
    assert_eq!(
        kubeconfig_proxy(&kubeconfig, Some("dev")).as_deref(),
        Some("http://proxy.example:3128")
    );
    assert_eq!(kubeconfig_proxy(&kubeconfig, Some("prod")), None);
    assert_eq!(kubeconfig_proxy(&kubeconfig, Some("missing")), None);
}

/// App#279 review: a proxy is shown and logged as `scheme://host:port` only -
/// never its credentials, path or query.
#[test]
fn a_proxy_is_redacted_to_scheme_host_and_port() {
    let with_userinfo: http::Uri = "http://user:s3cr3t@proxy.example:3128/path?q=1"
        .parse()
        .unwrap();
    let shown = redacted(&with_userinfo);
    assert_eq!(shown, "http://proxy.example:3128");
    assert!(!shown.contains("s3cr3t") && !shown.contains("user"));
    let bare: http::Uri = "http://proxy.example".parse().unwrap();
    assert_eq!(redacted(&bare), "http://proxy.example");
}
