//! #152: Report files the issue with `gh` when it's ready - faked here by a
//! script on a scratch `PATH` - and opens the browser when it isn't, or when
//! `gh` fails.

use super::super::gh::REPO;
use super::super::gh::tests::fake_gh;
use super::super::{Form, GhPathOverride, ReportState, Route, check_route};
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};

struct Blank;

impl gpui_kit::Render for Blank {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        _cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        gpui_kit::div()
    }
}

/// A form filled in with a subject and description, on `route`, in a window.
fn form(cx: &mut TestAppContext, route: Route) -> (Form, VisualTestContext) {
    cx.update(crate::util::test_ui::init);
    // `open_window`, not a bare test window: closing the dialog needs the
    // component root it puts around the view.
    let window = cx.update(|cx| {
        gpui_kit::open_window(gpui_kit::WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| Blank)
        })
        .expect("test window opens")
        .0
    });
    let mut vcx = VisualTestContext::from_window(window, cx);
    let form = vcx.update(|window, cx| {
        let subject = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value("Pods panel crashes", window, cx);
            input
        });
        let description = cx.new(|cx| {
            let mut input = TextareaState::new(window, cx);
            input.set_value("Hit refresh twice.", window, cx);
            input
        });
        let state = cx.new(|_| ReportState {
            route,
            sending: false,
        });
        Form {
            subject,
            description,
            state,
            version: "1.0.0".into(),
            build: "2026-10-01, abc1234".into(),
            platform: "macos",
        }
    });
    (form, vcx)
}

/// Waits - `gh` runs as a real child process - until `done`.
fn wait_until(vcx: &mut VisualTestContext, done: impl Fn(&mut VisualTestContext) -> bool) {
    for _ in 0..400 {
        vcx.run_until_parked();
        if done(vcx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("never finished");
}

#[gpui_kit::test]
async fn with_gh_ready_report_files_through_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (dir, path) = fake_gh(|dir| {
        let dir = dir.display();
        format!(
            "echo \"$@\" > '{dir}/args'\ncat > '{dir}/body'\n\
             echo 'https://github.com/{REPO}/issues/170'"
        )
    });
    cx.update(|cx| cx.set_global(GhPathOverride(path)));
    let (form, mut vcx) = form(cx, Route::Gh);

    vcx.update(|window, cx| form.report(window, cx));
    wait_until(&mut vcx, |_| {
        dir.join("args").exists() && dir.join("body").exists()
    });
    // Let the task that reads gh's answer finish too.
    for _ in 0..20 {
        vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let body = std::fs::read_to_string(dir.join("body")).unwrap();
    assert!(body.starts_with("Hit refresh twice."), "{body}");
    assert!(body.contains("2026-10-01, abc1234"), "the build is in it");
    let args = std::fs::read_to_string(dir.join("args")).unwrap();
    assert!(args.contains("--title Pods panel crashes"), "{args}");
    assert_eq!(vcx.opened_url(), None, "no browser");
}

#[gpui_kit::test]
async fn when_gh_fails_report_opens_the_browser_instead(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (_, path) = fake_gh(|_| "cat > /dev/null\necho 'HTTP 401' >&2\nexit 1".into());
    cx.update(|cx| cx.set_global(GhPathOverride(path)));
    let (form, mut vcx) = form(cx, Route::Gh);

    vcx.update(|window, cx| form.report(window, cx));
    wait_until(&mut vcx, |vcx| vcx.opened_url().is_some());
    let url = vcx.opened_url().unwrap_or_default();
    assert!(url.contains("title=Pods%20panel%20crashes"), "{url}");
}

#[gpui_kit::test]
async fn without_gh_report_opens_the_browser(cx: &mut TestAppContext) {
    let (form, mut vcx) = form(cx, Route::Browser);
    vcx.update(|window, cx| form.report(window, cx));
    vcx.run_until_parked();
    let url = vcx.opened_url().expect("the browser opened");
    assert!(url.contains("title=Pods%20panel%20crashes"), "{url}");
}

#[gpui_kit::test]
async fn the_dialog_finds_out_which_way_report_goes(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (_, ready) = fake_gh(|_| "exit 0".into());
    cx.update(|cx| cx.set_global(GhPathOverride(ready)));
    let (form, mut vcx) = form(cx, Route::Checking);
    let state: Entity<ReportState> = form.state.clone();
    vcx.update(|window, cx| check_route(state.clone(), window.window_handle(), cx));
    wait_until(&mut vcx, |vcx| {
        vcx.update(|_, cx| state.read(cx).route) == Route::Gh
    });
}
