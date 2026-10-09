//! [`Foreground`]: how a tool handler, running on the tokio runtime, reaches
//! GPUI state - the cluster registry, windows, panels - which only the main
//! thread may touch (design.md: UI dispatch boundary).
//!
//! A handler hands a closure to [`Foreground::run`]; a GPUI task started by
//! [`Foreground::spawn_on`] runs each one on the main thread in order and
//! sends its result back. The queue is bounded, so a burst of tool calls
//! waits for the main thread rather than piling up.

use super::error::ToolError;
use crate::consts::MCP_FOREGROUND_QUEUE;
use gpui_kit::App;
use tokio::sync::{mpsc, oneshot};

type Job = Box<dyn FnOnce(&mut App) + Send>;

/// A handle to the main thread for tool handlers. Cheap to clone.
#[derive(Clone)]
pub(super) struct Foreground {
    jobs: mpsc::Sender<Job>,
}

impl Foreground {
    /// A handle whose jobs arrive on the returned receiver, for a caller that
    /// runs them itself. A handle whose receiver is dropped refuses every job
    /// with [`ToolError::UiUnavailable`].
    pub(super) fn channel() -> (Self, mpsc::Receiver<Job>) {
        let (jobs, rx) = mpsc::channel(MCP_FOREGROUND_QUEUE);
        (Self { jobs }, rx)
    }

    /// A handle whose jobs run on `cx`'s main thread for as long as the app
    /// does.
    pub(super) fn spawn_on(cx: &mut App) -> Self {
        let (foreground, mut rx) = Self::channel();
        cx.spawn(async move |cx| {
            while let Some(job) = rx.recv().await {
                cx.update(job);
            }
        })
        .detach();
        foreground
    }

    /// Queues `work` for the main thread without waiting for it - for a
    /// `Drop`, which can't wait. Dropped if the queue is full or the app is
    /// quitting.
    pub(super) fn post(&self, work: impl FnOnce(&mut App) + Send + 'static) {
        let _ = self.jobs.try_send(Box::new(work));
    }

    /// Runs `work` on the main thread and returns what it returned. Fails
    /// only when the main thread no longer takes jobs - the app is quitting.
    pub(super) async fn run<R: Send + 'static>(
        &self,
        work: impl FnOnce(&mut App) -> R + Send + 'static,
    ) -> Result<R, ToolError> {
        let (reply, result) = oneshot::channel();
        let job: Job = Box::new(move |cx| {
            // The caller may have gone (its client disconnected); the work
            // still ran, and there is no one left to tell.
            let _ = reply.send(work(cx));
        });
        self.jobs
            .send(job)
            .await
            .map_err(|_| ToolError::UiUnavailable)?;
        result.await.map_err(|_| ToolError::UiUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{Global, TestAppContext};

    struct Marker(u32);
    impl Global for Marker {}

    #[gpui_kit::test]
    async fn a_job_runs_on_the_main_thread_and_returns_its_result(cx: &mut TestAppContext) {
        // The job's reply crosses from the tokio runtime's thread back to this
        // one; GPUI's deterministic scheduler forbids that wait by default.
        cx.executor().allow_parking();
        cx.update(crate::runtime::init);
        cx.update(|cx| cx.set_global(Marker(7)));
        let foreground = cx.update(Foreground::spawn_on);
        let handle = cx.update(|cx| crate::runtime::handle(cx));

        let read =
            handle.spawn(async move { foreground.run(|cx| cx.global::<Marker>().0 + 1).await });
        assert_eq!(read.await.unwrap(), Ok(8));
    }

    #[tokio::test]
    async fn a_handle_with_no_main_thread_reports_the_ui_unavailable() {
        let (foreground, rx) = Foreground::channel();
        drop(rx);
        assert_eq!(foreground.run(|_| ()).await, Err(ToolError::UiUnavailable));
    }
}
