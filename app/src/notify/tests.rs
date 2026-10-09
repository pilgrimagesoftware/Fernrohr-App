//! [`post`] against a fake backend: what reaches it, that a failure stays a
//! log line, that activation focuses the window it names, and that posting
//! never waits for the backend.

use super::*;
use gpui_kit::{Context, IntoElement, Render, TestAppContext, Window, WindowHandle, div};
use parking_lot::Mutex;
use std::sync::mpsc;
use std::time::Duration;

/// What the fake does with a notification.
#[derive(Clone, Copy)]
enum Outcome {
    Shown,
    Fails,
    Activated,
    Dismissed,
}

struct Fake {
    outcome: Outcome,
    posted: Mutex<Vec<Note>>,
    /// Released by the test; until then, posting blocks.
    hold: Mutex<Option<mpsc::Receiver<()>>>,
    /// Told when a notification's whole life - post, then wait - is over.
    finished: Mutex<mpsc::Sender<()>>,
}

impl Fake {
    fn new(outcome: Outcome) -> (Arc<Self>, mpsc::Receiver<()>) {
        let (finished, done) = mpsc::channel();
        let fake = Arc::new(Self {
            outcome,
            posted: Mutex::new(Vec::new()),
            hold: Mutex::new(None),
            finished: Mutex::new(finished),
        });
        (fake, done)
    }

    fn posted(&self) -> Vec<Note> {
        self.posted.lock().clone()
    }
}

impl Backend for Fake {
    fn post(&self, note: &Note) -> Result<Posted, String> {
        if let Some(hold) = self.hold.lock().take() {
            let _ = hold.recv();
        }
        self.posted.lock().push(note.clone());
        let finished = self.finished.lock().clone();
        let result = match self.outcome {
            Outcome::Shown => Ok(Posted::Shown),
            Outcome::Fails => Err("no notification service".to_string()),
            Outcome::Activated | Outcome::Dismissed => {
                let activated = matches!(self.outcome, Outcome::Activated);
                return Ok(Posted::Watching(Box::new(move || {
                    let _ = finished.send(());
                    activated
                })));
            }
        };
        let _ = finished.send(());
        result
    }
}

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// The app runtime, with `fake` as the backend.
fn app(cx: &mut TestAppContext, fake: &Arc<Fake>) {
    // The posting task runs on a Tokio thread and wakes this one.
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let backend: Arc<dyn Backend> = fake.clone();
    cx.update(|cx| set_backend(backend, cx));
}

fn window(cx: &mut TestAppContext) -> WindowHandle<Blank> {
    let window = cx.add_window(|_, _| Blank);
    cx.run_until_parked();
    window
}

/// Waits for the fake to finish with one notification, then lets the main
/// thread run what that woke.
fn settle(cx: &mut TestAppContext, done: &mpsc::Receiver<()>) {
    done.recv_timeout(Duration::from_secs(5))
        .expect("the backend was called");
    // The posting task replies just after the backend returns.
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(2));
        cx.run_until_parked();
    }
}

/// An `on_activate` that records it ran.
fn recorder() -> (Arc<Mutex<bool>>, impl FnOnce(&mut App) + 'static) {
    let ran = Arc::new(Mutex::new(false));
    let flag = ran.clone();
    (ran, move |_: &mut App| *flag.lock() = true)
}

#[gpui_kit::test]
async fn a_notification_reaches_the_backend_with_its_title_and_body(cx: &mut TestAppContext) {
    let (fake, done) = Fake::new(Outcome::Shown);
    app(cx, &fake);
    let (ran, on_activate) = recorder();

    cx.update(|cx| post("Confirm the connection", "dev is waiting", on_activate, cx));
    settle(cx, &done);

    assert_eq!(
        fake.posted(),
        [Note {
            title: "Confirm the connection".into(),
            body: "dev is waiting".into(),
        }]
    );
    assert!(!*ran.lock(), "nothing was activated");
}

#[gpui_kit::test]
async fn a_failed_notification_is_only_logged(cx: &mut TestAppContext) {
    let (fake, done) = Fake::new(Outcome::Fails);
    app(cx, &fake);
    let (ran, on_activate) = recorder();

    cx.update(|cx| post("title", "body", on_activate, cx));
    settle(cx, &done);

    assert!(!*ran.lock());
    // The app carries on: the next one is posted as usual.
    cx.update(|cx| post("again", "body", |_| {}, cx));
    settle(cx, &done);
    assert_eq!(fake.posted().len(), 2);
}

#[gpui_kit::test]
async fn activating_a_notification_focuses_the_window_it_names(cx: &mut TestAppContext) {
    let (fake, done) = Fake::new(Outcome::Activated);
    app(cx, &fake);
    let origin = window(cx);
    let other = window(cx);
    cx.update(|cx| {
        other
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| cx.active_window()).map(|w| w.window_id()),
        Some(other.window_id())
    );

    cx.update(|cx| post("title", "body", focus(origin.into()), cx));
    settle(cx, &done);

    assert_eq!(
        cx.update(|cx| cx.active_window()).map(|w| w.window_id()),
        Some(origin.window_id()),
        "the notification's window came to the front"
    );
}

#[gpui_kit::test]
async fn a_dismissed_notification_runs_nothing(cx: &mut TestAppContext) {
    let (fake, done) = Fake::new(Outcome::Dismissed);
    app(cx, &fake);
    let (ran, on_activate) = recorder();

    cx.update(|cx| post("title", "body", on_activate, cx));
    settle(cx, &done);

    assert!(!*ran.lock());
}

#[gpui_kit::test]
async fn posting_never_waits_for_the_backend(cx: &mut TestAppContext) {
    let (fake, done) = Fake::new(Outcome::Shown);
    let (release, hold) = mpsc::channel();
    *fake.hold.lock() = Some(hold);
    app(cx, &fake);

    // Returns while the backend is still blocked.
    cx.update(|cx| post("title", "body", |_| {}, cx));
    assert!(fake.posted().is_empty(), "the backend hasn't been released");

    release.send(()).unwrap();
    settle(cx, &done);
    assert_eq!(fake.posted().len(), 1);
}
