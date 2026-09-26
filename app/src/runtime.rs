use gpui_kit::{App, Global};
use std::future::Future;
use tokio::runtime::{Handle, Runtime};
use tokio::sync::mpsc;

/// The app-owned tokio runtime. Kept alive for the process lifetime via a
/// GPUI global; `kube-rs` work runs as tasks on its handle, bridged back to
/// GPUI's foreground executor over bounded channels (see [`spawn_stream`]).
struct TokioRuntime(Runtime);

impl Global for TokioRuntime {}

pub fn init(cx: &mut App) {
    let runtime = Runtime::new().expect("failed to start the tokio runtime");
    cx.set_global(TokioRuntime(runtime));
}

pub fn handle(cx: &App) -> Handle {
    cx.global::<TokioRuntime>().0.handle().clone()
}

/// Spawns `task(tx)` on the tokio runtime. Returns the receiving half so a
/// GPUI foreground task can drain it with [`drain`]. The channel is bounded:
/// a slow foreground applies backpressure to the producer during a burst.
pub fn spawn_stream<T, F, Fut>(cx: &App, capacity: usize, task: F) -> mpsc::Receiver<T>
where
    T: Send + 'static,
    F: FnOnce(mpsc::Sender<T>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (tx, rx) = mpsc::channel(capacity);
    handle(cx).spawn(task(tx));
    rx
}

/// Drains `rx` on the calling (foreground) executor, applying each item in
/// order, until the sender is dropped.
pub async fn drain<T>(mut rx: mpsc::Receiver<T>, mut apply: impl FnMut(T)) {
    while let Some(item) = rx.recv().await {
        apply(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[gpui_kit::test]
    async fn foreground_drain_receives_items_in_order(cx: &mut TestAppContext) {
        // The drain genuinely blocks on a cross-thread wakeup from the tokio
        // runtime's background thread; GPUI's deterministic test scheduler
        // forbids that by default.
        cx.executor().allow_parking();
        cx.update(init);

        let rx = cx.update(|cx| {
            spawn_stream(cx, 8, |tx| async move {
                for i in 0..5 {
                    tx.send(i).await.unwrap();
                }
            })
        });

        let received = Rc::new(RefCell::new(Vec::new()));
        let received_clone = received.clone();
        let task = cx.update(|cx| {
            cx.spawn(async move |_cx| {
                drain(rx, |item| received_clone.borrow_mut().push(item)).await;
            })
        });
        task.await;

        assert_eq!(*received.borrow(), vec![0, 1, 2, 3, 4]);
    }
}
