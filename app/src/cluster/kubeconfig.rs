use kube::config::{Kubeconfig, KubeconfigError};
use std::path::Path;

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
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

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
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fernrohr-kubeconfig-fixture-{n}.yaml"));
        fs::write(&path, FIXTURE).unwrap();
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
}
