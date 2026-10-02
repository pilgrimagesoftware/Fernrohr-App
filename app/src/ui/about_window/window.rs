//! The About window entity and how it is opened: the action handler that owns
//! the single-instance handle, and the state the render tree reads.
//!
//! What it draws lives in `super::pane`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Context, FocusHandle, TitlebarOptions, WindowBounds,
    WindowOptions, size,
};

use crate::ui::menu::About;

/// Registers the `About` handler over the window handle it keeps.
///
/// The handle lives in this closure rather than in `init`, so nothing outside
/// can open a second About window past the single-instance check.
pub(crate) fn register_about_action(cx: &mut App) {
    let handle: Rc<RefCell<Option<AnyWindowHandle>>> = Rc::new(RefCell::new(None));
    cx.on_action(move |_: &About, cx| {
        open_about_window(&handle, cx);
    });
}

/// Raises the open About window, or opens one.
///
/// Whether the last window is still open is asked by updating it: a closed
/// window fails its update, which is the same test a single-instance window
/// needs and needs no close observer to clear the handle.
fn open_about_window(handle: &Rc<RefCell<Option<AnyWindowHandle>>>, cx: &mut App) {
    if let Some(existing) = *handle.borrow()
        && existing
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    match cx.open_window(about_window_options(cx), |window, cx| {
        crate::ui::theme::watch_window(window, cx);
        cx.new(AboutWindow::new)
    }) {
        Ok(window) => *handle.borrow_mut() = Some(window.into()),
        Err(error) => eprintln!("failed to open the About window: {error}"),
    }
}

/// The About window's fixed size. Not resizable and not minimizable: its
/// content neither reflows usefully nor is worth keeping in the Dock.
///
/// The titlebar carries no text on macOS: the system's own About panel has
/// none, and the window already names the app in its body. Elsewhere a
/// titled window is the expectation, so the title is shown there.
///
/// `titlebar` must stay `Some` on every platform, even when `title` is
/// `None`: gpui's macOS backend only adds `NSClosableWindowMask` (the close
/// button) when a `TitlebarOptions` is present at all - a `None` titlebar
/// drops the close button along with the title text, leaving the window with
/// no system chrome to close it from.
fn about_window_options(cx: &App) -> WindowOptions {
    let titlebar = Some(TitlebarOptions {
        title: (!cfg!(target_os = "macos")).then(|| "About Fernrohr".into()),
        ..Default::default()
    });
    let bounds = Bounds::centered(None, size(gpui_kit::px(360.), gpui_kit::px(420.)), cx);
    WindowOptions {
        titlebar,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        is_resizable: false,
        is_minimizable: false,
        ..Default::default()
    }
}

pub(super) struct AboutWindow {
    /// Focused on first render so the window has a key target for `Escape`.
    /// A window with nothing focused never sees the key event at all.
    pub(super) focus: FocusHandle,
}

impl AboutWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::menu::About;

    /// Activating About twice opens exactly one window - the second
    /// dispatch raises the existing one rather than stacking a second.
    #[gpui_kit::test]
    fn about_opens_one_window_no_matter_how_many_times_its_dispatched(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            super::register_about_action(cx);
        });

        let before = cx.update(|cx| cx.windows().len());
        cx.update(|cx| cx.dispatch_action(&About));
        cx.run_until_parked();
        let after_first = cx.update(|cx| cx.windows().len());
        assert_eq!(after_first, before + 1, "the first dispatch opens a window");

        cx.update(|cx| cx.dispatch_action(&About));
        cx.run_until_parked();
        let after_second = cx.update(|cx| cx.windows().len());
        assert_eq!(
            after_second, after_first,
            "the second dispatch raised the existing window rather than opening another"
        );
    }
}
