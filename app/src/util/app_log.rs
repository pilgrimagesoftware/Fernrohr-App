//! The application log: one `fernrohr.log` per run in the per-user log
//! directory, the previous run's kept as `fernrohr.log.1`.
//!
//! Logging never fails the app: if the directory or file can't be set up, the
//! logger still installs and writes to stderr (when that is a terminal) only.

use std::fs::{self, File, OpenOptions};
use std::io::{IsTerminal, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use log::{Level, LevelFilter, Log, Metadata, Record};
use parking_lot::Mutex;

use crate::consts::{LOG_FILE_NAME, LOG_TAIL_BYTES, LOG_TAIL_LINES};
use crate::util::paths::log_dir;

// UNWIRED: the Report Issue window is the first caller.
#[allow(dead_code)]
/// Where the application log is written.
pub fn log_path() -> PathBuf {
    log_dir().join(LOG_FILE_NAME)
}

/// Rotates the previous log, opens a fresh one and installs the global logger.
/// Calling it again is harmless: the first logger stays.
pub fn install() {
    let file = open_in(&log_dir());
    let level = parse_level(std::env::var("RUST_LOG").ok().as_deref());
    let logger = Box::new(FileLogger {
        file: Mutex::new(file),
        stderr: std::io::stderr().is_terminal(),
    });
    if log::set_logger(Box::leak(logger)).is_ok() {
        log::set_max_level(level);
    }
}

// UNWIRED: the Report Issue window is the first caller.
#[allow(dead_code)]
/// The last `LOG_TAIL_LINES` lines of the file, from at most `LOG_TAIL_BYTES`
/// bytes. When the file is larger than the byte budget the partial first line
/// is dropped.
pub fn read_tail(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let budget = LOG_TAIL_BYTES as u64;
    let trimmed = len > budget;
    if trimmed {
        file.seek(SeekFrom::Start(len - budget))?;
    }
    let mut bytes = Vec::with_capacity(LOG_TAIL_BYTES);
    file.take(budget).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = if trimmed {
        text.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        &text
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(LOG_TAIL_LINES);
    let mut out = lines[start..].join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

struct FileLogger {
    file: Mutex<Option<File>>,
    stderr: bool,
}

impl Log for FileLogger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format_record(
            SystemTime::now(),
            record.level(),
            record.target(),
            &record.args().to_string(),
        );
        if let Some(file) = self.file.lock().as_mut() {
            let _ = file.write_all(line.as_bytes()).and_then(|()| file.flush());
        }
        if self.stderr {
            let _ = std::io::stderr().write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Some(file) = self.file.lock().as_mut() {
            let _ = file.flush();
        }
    }
}

/// Moves an existing log to `fernrohr.log.1`, replacing an older one.
fn rotate(dir: &Path) {
    let current = dir.join(LOG_FILE_NAME);
    if current.exists() {
        let _ = fs::rename(&current, dir.join(format!("{LOG_FILE_NAME}.1")));
    }
}

/// Creates `dir`, rotates, and opens a fresh log there; `None` when it can't.
fn open_in(dir: &Path) -> Option<File> {
    fs::create_dir_all(dir).ok()?;
    rotate(dir);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(LOG_FILE_NAME))
        .ok()
}

/// `RUST_LOG` exactly one of the five level names (any case); anything else,
/// or unset, is `info`.
fn parse_level(value: Option<&str>) -> LevelFilter {
    match value.map(str::to_ascii_lowercase).as_deref() {
        Some("error") => LevelFilter::Error,
        Some("warn") => LevelFilter::Warn,
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        _ => LevelFilter::Info,
    }
}

/// `2026-10-13T19:41:55Z WARN fernrohr::k8s::client: message\n`.
fn format_record(time: SystemTime, level: Level, target: &str, message: &str) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    let (hour, minute, second) = (rem / 3_600, rem % 3_600 / 60, rem % 60);
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z {level} {target}: {message}\n"
    )
}

/// Howard Hinnant's `civil_from_days`: proleptic Gregorian date from a day
/// count since the Unix epoch, without a calendar dependency.
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

#[cfg(test)]
mod tests;
