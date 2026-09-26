use std::path::PathBuf;

#[cfg(target_os = "macos")]
const APP_DIR_NAME: &str = "com.pilgrimagesoftware.fernrohr";
#[cfg(not(target_os = "macos"))]
const APP_DIR_NAME: &str = "fernrohr";

fn resolve(base: Option<PathBuf>) -> PathBuf {
    base.unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
}

/// Window state, workspace layout: `dirs::data_dir()/<app_dir>/`.
pub fn state_dir() -> PathBuf {
    resolve(dirs::data_dir())
}

/// User-editable settings (`ui.toml`, `keymap.toml`): `dirs::preference_dir()/<app_dir>/`.
pub fn preference_dir() -> PathBuf {
    resolve(dirs::preference_dir())
}

/// Discovery cache, metric buffers: `dirs::cache_dir()/<app_dir>/`.
// UNWIRED: no cache-backed feature exists yet; discovery and metrics are the
// first intended callers.
#[allow(dead_code)]
pub fn cache_dir() -> PathBuf {
    resolve(dirs::cache_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_dir_ends_with_app_dir_name() {
        assert!(state_dir().ends_with(APP_DIR_NAME));
    }

    #[test]
    fn preference_dir_ends_with_app_dir_name() {
        assert!(preference_dir().ends_with(APP_DIR_NAME));
    }

    #[test]
    fn cache_dir_ends_with_app_dir_name() {
        assert!(cache_dir().ends_with(APP_DIR_NAME));
    }

    #[test]
    fn resolve_falls_back_to_dot_when_platform_dir_unavailable() {
        assert_eq!(resolve(None), PathBuf::from(".").join(APP_DIR_NAME));
    }
}
