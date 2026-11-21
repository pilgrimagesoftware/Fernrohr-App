//! The ETXTBSY retry: busy attempts are retried, other failures and
//! successes return at once, and a busy error is found wrapped.

use super::{ATTEMPTS, caused_by_busy, past_busy, past_busy_blocking};
use std::cell::Cell;
use std::io;

fn busy() -> io::Error {
    io::Error::from(io::ErrorKind::ExecutableFileBusy)
}

#[test]
fn a_busy_run_is_retried_until_it_starts() {
    let tries = Cell::new(0);
    let result = past_busy_blocking(|| {
        tries.set(tries.get() + 1);
        if tries.get() < 3 {
            Err(busy())
        } else {
            Ok("ran")
        }
    });
    assert_eq!(result.unwrap(), "ran");
    assert_eq!(tries.get(), 3);
}

#[test]
fn another_failure_is_not_retried() {
    let tries = Cell::new(0);
    let result: io::Result<()> = past_busy_blocking(|| {
        tries.set(tries.get() + 1);
        Err(io::Error::from(io::ErrorKind::NotFound))
    });
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    assert_eq!(tries.get(), 1);
}

#[tokio::test]
async fn a_file_that_stays_busy_is_given_up_on() {
    let tries = Cell::new(0);
    let result: io::Result<()> = past_busy(|| {
        tries.set(tries.get() + 1);
        std::future::ready(Err(busy()))
    })
    .await;
    assert_eq!(
        result.unwrap_err().kind(),
        io::ErrorKind::ExecutableFileBusy
    );
    assert_eq!(tries.get(), ATTEMPTS);
}

/// A library's error wrapping the busy one, as `kube`'s exec auth does.
#[derive(Debug)]
struct Wrapped(io::Error);
impl std::fmt::Display for Wrapped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("couldn't start the plugin")
    }
}
impl std::error::Error for Wrapped {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[test]
fn a_wrapped_busy_error_is_recognised() {
    assert!(caused_by_busy(&Wrapped(busy())));
    assert!(!caused_by_busy(&Wrapped(io::Error::from(
        io::ErrorKind::NotFound
    ))));
}
