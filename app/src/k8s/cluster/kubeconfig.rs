use kube::config::{Kubeconfig, KubeconfigError};
use std::path::Path;

/// Failure modes for [`server_for_context`]: either the kubeconfig itself couldn't be
/// read, the context/cluster it points at doesn't resolve to a `server:` URL, or that
/// URL has no host to forward to.
#[derive(Debug)]
pub enum ServerForContextError {
    Kubeconfig(KubeconfigError),
    /// `context` has no entry in the kubeconfig's `contexts` list.
    UnknownContext(String),
    /// `context`'s cluster has no matching `clusters` entry, or that entry has no
    /// `server:` set.
    NoServer(String),
    /// The cluster's `server:` URL has no host - the message names the URL itself, per
    /// tasks.md 2.1.
    NoHost(String),
}

impl std::fmt::Display for ServerForContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kubeconfig(err) => write!(f, "failed to read kubeconfig: {err}"),
            Self::UnknownContext(context) => {
                write!(f, "kubeconfig has no context named {context:?}")
            }
            Self::NoServer(cluster) => {
                write!(f, "kubeconfig cluster {cluster:?} has no server URL")
            }
            Self::NoHost(url) => write!(f, "kubeconfig server URL has no host: {url}"),
        }
    }
}

impl std::error::Error for ServerForContextError {}

/// The host and port a bound context's forward should reach, taken from that
/// context's kubeconfig `server:` URL (section 1.2 of `context-tunnel-binding`).
/// `path` is resolved exactly like [`list_context_names`]: the given path, or
/// `$KUBECONFIG`/`~/.kube/config` when `None`. The port defaults from the URL's scheme
/// (`https` -> 443, `http` -> 80) when the URL omits one explicitly.
pub fn server_for_context(
    path: Option<&Path>,
    context: &str,
) -> Result<(String, u16), ServerForContextError> {
    let kubeconfig = match path {
        Some(path) => Kubeconfig::read_from(path),
        None => Kubeconfig::read(),
    }
    .map_err(ServerForContextError::Kubeconfig)?;

    let cluster_name = kubeconfig
        .contexts
        .iter()
        .find(|named| named.name == context)
        .and_then(|named| named.context.as_ref())
        .map(|ctx| ctx.cluster.clone())
        .ok_or_else(|| ServerForContextError::UnknownContext(context.to_string()))?;

    let server = kubeconfig
        .clusters
        .iter()
        .find(|named| named.name == cluster_name)
        .and_then(|named| named.cluster.as_ref())
        .and_then(|cluster| cluster.server.clone())
        .ok_or_else(|| ServerForContextError::NoServer(cluster_name.clone()))?;

    split_server_url(&server)
}

/// Splits a kubeconfig `server:` URL (`scheme://host[:port][/path]`) into its host and
/// port, defaulting the port from the scheme when the URL omits one. IPv6 literal hosts
/// (`[::1]:6443`) aren't handled - no kubeconfig fixture seen so far needs them.
fn split_server_url(url: &str) -> Result<(String, u16), ServerForContextError> {
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);

    let (host, explicit_port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().ok()),
        None => (authority, None),
    };

    if host.is_empty() {
        return Err(ServerForContextError::NoHost(url.to_string()));
    }

    let port = match explicit_port {
        Some(port) => port,
        None => match scheme {
            "http" => 80,
            _ => 443,
        },
    };
    Ok((host.to_string(), port))
}

/// Lists context names from the kubeconfig at `path`, or, if `path` is
/// `None`, from `$KUBECONFIG` then `~/.kube/config` (kube's own default
/// resolution). Never writes: `Kubeconfig::read` only reads the file.
pub fn list_context_names(path: Option<&Path>) -> Result<Vec<String>, KubeconfigError> {
    let kubeconfig = match path {
        Some(path) => Kubeconfig::read_from(path)?,
        None => Kubeconfig::read()?,
    };
    Ok(kubeconfig.contexts.into_iter().map(|c| c.name).collect())
}

/// The kubeconfig's `current-context`, by the same resolution as
/// [`list_context_names`]. `Config::infer` doesn't retain the context name it
/// resolved to, so the section 6.2 connect path reads it separately here to
/// look up a `tunnels.toml` binding.
pub fn current_context_name(path: Option<&Path>) -> Result<Option<String>, KubeconfigError> {
    let kubeconfig = match path {
        Some(path) => Kubeconfig::read_from(path)?,
        None => Kubeconfig::read()?,
    };
    Ok(kubeconfig.current_context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const FIXTURE: &str = r#"
apiVersion: v1
kind: Config
clusters:
  - name: kind-dev
    cluster:
      server: https://127.0.0.1:6443
  - name: staging
    cluster:
      server: https://staging.example.com:6443
contexts:
  - name: kind-dev
    context:
      cluster: kind-dev
      user: kind-dev
  - name: staging
    context:
      cluster: staging
      user: staging
current-context: kind-dev
users:
  - name: kind-dev
    user: {}
  - name: staging
    user: {}
"#;

    fn fixture_path() -> std::path::PathBuf {
        let path = crate::util::test_paths::temp_path("kubeconfig-fixture").with_extension("yaml");
        fs::write(&path, FIXTURE).unwrap();
        path
    }

    /// Covers tasks.md 2.1's three cases: a server URL with no explicit port (defaults
    /// from the scheme), one with an explicit port, and one with no host at all.
    const SERVER_FIXTURE: &str = r#"
apiVersion: v1
kind: Config
clusters:
  - name: no-port
    cluster:
      server: https://10.0.0.1
  - name: with-port
    cluster:
      server: https://api.internal:6443
  - name: hostless
    cluster:
      server: https://
contexts:
  - name: no-port
    context:
      cluster: no-port
      user: no-port
  - name: with-port
    context:
      cluster: with-port
      user: with-port
  - name: hostless
    context:
      cluster: hostless
      user: hostless
users:
  - name: no-port
    user: {}
  - name: with-port
    user: {}
  - name: hostless
    user: {}
"#;

    fn server_fixture_path() -> std::path::PathBuf {
        let path =
            crate::util::test_paths::temp_path("kubeconfig-server-fixture").with_extension("yaml");
        fs::write(&path, SERVER_FIXTURE).unwrap();
        path
    }

    #[test]
    fn lists_all_context_names_from_fixture() {
        let path = fixture_path();

        let contexts = list_context_names(Some(&path)).unwrap();

        assert_eq!(
            contexts,
            vec!["kind-dev".to_string(), "staging".to_string()]
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_does_not_modify_the_file() {
        let path = fixture_path();
        let bytes_before = fs::read(&path).unwrap();
        let mtime_before = fs::metadata(&path).unwrap().modified().unwrap();

        list_context_names(Some(&path)).unwrap();

        let bytes_after = fs::read(&path).unwrap();
        let mtime_after = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(bytes_before, bytes_after);
        assert_eq!(mtime_before, mtime_after);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn server_for_context_defaults_the_port_from_the_https_scheme() {
        let path = server_fixture_path();

        let (host, port) = server_for_context(Some(&path), "no-port").unwrap();

        assert_eq!(host, "10.0.0.1");
        assert_eq!(port, 443);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn server_for_context_uses_an_explicit_port() {
        let path = server_fixture_path();

        let (host, port) = server_for_context(Some(&path), "with-port").unwrap();

        assert_eq!(host, "api.internal");
        assert_eq!(port, 6443);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn server_for_context_errors_naming_the_url_when_there_is_no_host() {
        let path = server_fixture_path();

        let err = server_for_context(Some(&path), "hostless").unwrap_err();

        assert!(matches!(err, ServerForContextError::NoHost(ref url) if url == "https://"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn server_for_context_errors_on_an_unknown_context() {
        let path = server_fixture_path();

        let err = server_for_context(Some(&path), "does-not-exist").unwrap_err();

        assert!(
            matches!(err, ServerForContextError::UnknownContext(ref c) if c == "does-not-exist")
        );
        let _ = fs::remove_file(&path);
    }
}
