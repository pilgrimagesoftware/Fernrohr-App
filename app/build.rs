//! Stamps the binary with the commit it was built from and the date it was
//! built, so the About window can name the running build and not only its
//! version number (`openspec/changes/about-window`).
//!
//! Neither value is allowed to fail the build: a binary built from a source
//! tarball has no repository to ask, and `git` may not be installed on the
//! machine doing the building. That case stamps [`UNKNOWN`] rather than an
//! empty string, so the About window can say the commit is unknown instead
//! of showing a blank where an identifier belongs.

use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Stamped in place of the commit when it cannot be determined. The About
/// window matches on this exact value, so the two must stay in step.
const UNKNOWN: &str = "unknown";

fn main() {
    let commit = commit().unwrap_or_else(|| UNKNOWN.to_string());
    println!("cargo:rustc-env=FERNROHR_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=FERNROHR_BUILD_DATE={}", build_date());
    watch_head();
}

/// The short commit hash, suffixed `-dirty` when the working tree has
/// uncommitted changes. `None` when `git` is missing or the directory is not
/// a repository.
fn commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hash = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if hash.is_empty() {
        return None;
    }
    Some(if is_dirty() {
        format!("{hash}-dirty")
    } else {
        hash
    })
}

fn is_dirty() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
}

/// Today's UTC date as `YYYY-MM-DD`, computed from `SystemTime` with no extra
/// dependency: `build.rs` runs outside the crate's own dependency graph, and
/// three integers don't justify a `build-dependencies` entry.
fn build_date() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_secs();
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's `civil_from_days`: proleptic Gregorian date from a day
/// count since the Unix epoch. Avoids pulling in a calendar library just to
/// turn a day count into year/month/day.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

/// Re-runs this script when the repository's current commit changes, so a
/// new commit re-stamps the binary without a full `cargo clean`. Silently
/// does nothing outside a git repository.
fn watch_head() {
    let git_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.git");
    if !git_dir.exists() {
        return;
    }
    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD"))
        && let Some(ref_path) = head.trim().strip_prefix("ref: ")
    {
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join(ref_path).display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::civil_from_days;

    #[test]
    fn epoch_day_zero_is_1970_01_01() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn a_known_recent_date_round_trips() {
        // 2024-03-01 is day 19782 since the Unix epoch.
        assert_eq!(civil_from_days(19_782), (2024, 3, 1));
    }
}
