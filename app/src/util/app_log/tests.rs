use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A fresh, empty directory unique to this process and call.
fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fernrohr-applog-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn rotate_moves_the_log_and_replaces_an_older_one() {
    let dir = temp_dir();
    fs::write(dir.join(LOG_FILE_NAME), "new").unwrap();
    fs::write(dir.join("fernrohr.log.1"), "old").unwrap();
    rotate(&dir);
    assert!(!dir.join(LOG_FILE_NAME).exists());
    assert_eq!(
        fs::read_to_string(dir.join("fernrohr.log.1")).unwrap(),
        "new"
    );
}

#[test]
fn rotate_without_a_log_does_nothing() {
    let dir = temp_dir();
    rotate(&dir);
    assert!(!dir.join("fernrohr.log.1").exists());
}

#[test]
fn open_in_creates_the_directory_and_a_fresh_file() {
    let dir = temp_dir().join("nested");
    assert!(open_in(&dir).is_some());
    assert!(dir.join(LOG_FILE_NAME).exists());
}

#[test]
fn level_defaults_to_info() {
    assert_eq!(parse_level(None), LevelFilter::Info);
}

#[test]
fn each_valid_level_parses_in_any_case() {
    assert_eq!(parse_level(Some("error")), LevelFilter::Error);
    assert_eq!(parse_level(Some("WARN")), LevelFilter::Warn);
    assert_eq!(parse_level(Some("Info")), LevelFilter::Info);
    assert_eq!(parse_level(Some("debug")), LevelFilter::Debug);
    assert_eq!(parse_level(Some("TRACE")), LevelFilter::Trace);
}

#[test]
fn an_invalid_level_falls_back_to_info() {
    for value in ["", "off", "fernrohr=debug", " debug", "verbose"] {
        assert_eq!(parse_level(Some(value)), LevelFilter::Info, "{value:?}");
    }
}

#[test]
fn record_has_timestamp_level_target_and_message_on_one_line() {
    // 2026-10-13T19:41:55Z
    let time = UNIX_EPOCH + Duration::from_secs(1_791_920_515);
    assert_eq!(
        format_record(time, Level::Warn, "fernrohr::k8s::client", "message"),
        "2026-10-13T19:41:55Z WARN fernrohr::k8s::client: message\n"
    );
}

#[test]
fn epoch_day_zero_is_1970_01_01() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
}

#[test]
fn a_short_file_is_returned_whole() {
    let path = temp_dir().join(LOG_FILE_NAME);
    fs::write(&path, "one\ntwo\n").unwrap();
    assert_eq!(read_tail(&path).unwrap(), "one\ntwo\n");
}

#[test]
fn a_long_file_is_trimmed_to_the_last_lines() {
    let path = temp_dir().join(LOG_FILE_NAME);
    let text: String = (0..500).map(|i| format!("l{i}\n")).collect();
    fs::write(&path, text).unwrap();
    let tail = read_tail(&path).unwrap();
    let lines: Vec<&str> = tail.lines().collect();
    assert_eq!(lines.len(), LOG_TAIL_LINES);
    assert_eq!(lines[0], "l300");
    assert_eq!(lines[LOG_TAIL_LINES - 1], "l499");
}

#[test]
fn the_byte_cap_drops_the_partial_first_line() {
    let path = temp_dir().join(LOG_FILE_NAME);
    // Few, long lines: the byte budget bites before the line cap.
    let text: String = (0..100)
        .map(|i| format!("{i:03}{}\n", "x".repeat(997)))
        .collect();
    fs::write(&path, &text).unwrap();
    let tail = read_tail(&path).unwrap();
    assert!(tail.len() <= LOG_TAIL_BYTES);
    assert!(
        tail.lines().all(|line| line.len() == 1_000),
        "a partial line survived"
    );
    assert!(tail.ends_with(&format!("099{}\n", "x".repeat(997))));
}
