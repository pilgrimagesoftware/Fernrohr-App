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

/// Drains `rx`, coalescing consecutive updates that share a key before
/// applying them: if two updates for the same key arrive back-to-back with
/// nothing else in between, only the later one is applied. This bounds the
/// work done per item during a relist burst without dropping updates to
/// other keys or reordering across keys.
pub async fn drain_coalescing<T, K: PartialEq>(
    mut rx: mpsc::Receiver<T>,
    key: impl Fn(&T) -> K,
    mut apply: impl FnMut(T),
) {
    let mut pending: Option<T> = None;
    while let Some(item) = rx.recv().await {
        match &pending {
            Some(prev) if key(prev) == key(&item) => pending = Some(item),
            Some(_) => apply(pending.replace(item).unwrap()),
            None => pending = Some(item),
        }
    }
    if let Some(last) = pending {
        apply(last);
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

    #[derive(Debug, Clone, PartialEq)]
    enum Delta {
        Add(u32, &'static str),
        Update(u32, &'static str),
        Delete(u32),
    }

    fn key(delta: &Delta) -> u32 {
        match delta {
            Delta::Add(k, _) | Delta::Update(k, _) | Delta::Delete(k) => *k,
        }
    }

    #[gpui_kit::test]
    async fn coalescing_drain_folds_consecutive_same_key_updates(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        cx.update(init);

        // Interleaved burst: two consecutive updates to key 1 collapse into
        // one; key 2's add and delete are consecutive and collapse into the
        // delete; key 1's final state stays untouched by key 2 in between.
        let burst = vec![
            Delta::Add(1, "a"),
            Delta::Update(1, "b"),
            Delta::Add(2, "x"),
            Delta::Update(2, "y"),
            Delta::Delete(2),
            Delta::Update(1, "c"),
        ];

        let rx = cx.update(|cx| {
            spawn_stream(cx, 8, move |tx| async move {
                for delta in burst {
                    tx.send(delta).await.unwrap();
                }
            })
        });

        let applied = Rc::new(RefCell::new(Vec::new()));
        let applied_clone = applied.clone();
        let task = cx.update(|cx| {
            cx.spawn(async move |_cx| {
                drain_coalescing(rx, key, |item| applied_clone.borrow_mut().push(item)).await;
            })
        });
        task.await;

        assert_eq!(
            *applied.borrow(),
            vec![Delta::Update(1, "b"), Delta::Delete(2), Delta::Update(1, "c")]
        );
    }
}
