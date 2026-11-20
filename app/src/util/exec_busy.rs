//! Running an executable a test has only just written (#412, #413): Linux
//! can refuse it with "Text file busy" (ETXTBSY) for a moment.
//!
//! Tests run in parallel threads. A process another thread forks while a test
//! still has its fake executable (a `helm`, a `gh`, a kubeconfig exec plugin)
//! open for writing inherits that descriptor until it execs, and Linux won't
//! exec a file anything holds open for writing. Writing to a temporary
//! name and renaming doesn't help: the inherited descriptor is the same
//! inode's. So the run is retried for up to about a second.
//!
//! Only tests retry. A real binary isn't freshly written, so in the app each
//! attempt runs once, and a busy file reports like any other failure.

use std::io;

/// `attempt`, again while it fails with "Text file busy" - in tests only.
pub(crate) async fn past_busy<T, F>(attempt: impl FnMut() -> F) -> io::Result<T>
where
    F: Future<Output = io::Result<T>>,
{
    #[cfg(test)]
    {
        retry_while(attempt, is_busy).await
    }
    #[cfg(not(test))]
    {
        let mut attempt = attempt;
        attempt().await
    }
}

/// [`past_busy`] for a blocking `attempt`, such as `Command::status`.
pub(crate) fn past_busy_blocking<T>(mut attempt: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    #[cfg(test)]
    {
        let mut tries = 1;
        loop {
            match attempt() {
                Err(error) if is_busy(&error) && tries < ATTEMPTS => {
                    tries += 1;
                    std::thread::sleep(PAUSE);
                }
                result => return result,
            }
        }
    }
    #[cfg(not(test))]
    attempt()
}

/// How many times a busy run is tried, and the pause between tries: about a
/// second in all.
#[cfg(test)]
const ATTEMPTS: u32 = 100;
#[cfg(test)]
const PAUSE: std::time::Duration = std::time::Duration::from_millis(10);

#[cfg(test)]
fn is_busy(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::ExecutableFileBusy
}

/// `attempt`, again while `busy` says its error is "Text file busy" - for a
/// test whose executable some library runs, so the error arrives wrapped in
/// the library's own (see [`caused_by_busy`]).
#[cfg(test)]
pub(crate) async fn retry_while<T, E, F>(
    mut attempt: impl FnMut() -> F,
    busy: impl Fn(&E) -> bool,
) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    let mut tries = 1;
    loop {
        match attempt().await {
            Err(error) if busy(&error) && tries < ATTEMPTS => {
                tries += 1;
                tokio::time::sleep(PAUSE).await;
            }
            result => return result,
        }
    }
}

/// Whether `error`, or any error in its source chain, is "Text file busy".
#[cfg(test)]
pub(crate) fn caused_by_busy(error: &(dyn std::error::Error + 'static)) -> bool {
    std::iter::successors(Some(error), |error| error.source())
        .any(|error| error.downcast_ref::<io::Error>().is_some_and(is_busy))
}

#[cfg(test)]
mod tests;
