//! The About window entity and how it is opened: the action handler that owns
//! the single-instance handle, and the state the render tree reads.
//!
//! What it draws lives in `super::pane`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Context, FocusHandle, Pixels, TitlebarOptions,
    WindowBounds, WindowOptions, size,
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
    /// The height the window was last asked to fit its content to, so it
    /// asks once per change rather than every frame - see
    /// `pane::fit_window_to_content`.
    pub(super) fitted_height: Rc<Cell<Option<Pixels>>>,
}

impl AboutWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            fitted_height: Rc::default(),
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

    /// `resource-kind-icons` 4.1: the About window credits the Kubernetes
    /// icon set and names its licence.
    #[test]
    fn the_credits_name_the_kubernetes_icon_set_and_its_licence() {
        use super::super::pane::KUBERNETES_ICONS_CREDIT;
        use crate::ui::typography::recorder::with_recorded_text;

        assert!(KUBERNETES_ICONS_CREDIT.contains("Kubernetes"));
        assert!(KUBERNETES_ICONS_CREDIT.contains("Apache License 2.0"));
        with_recorded_text(|cx, recorded| {
            cx.update(|cx| {
                gpui_kit::init(cx);
                super::register_about_action(cx);
            });
            cx.update(|cx| cx.dispatch_action(&About));
            cx.run_until_parked();
            assert!(
                recorded.families_of(KUBERNETES_ICONS_CREDIT).is_some(),
                "the credit line is drawn"
            );
            assert!(recorded.families_of("Icons").is_some(), "under its heading");
        });
    }

    /// Opens the About window through its action and lets it settle: the
    /// icon's background resample lands and the window fits its content.
    fn open_about(cx: &mut gpui_kit::TestAppContext) -> gpui_kit::VisualTestContext {
        cx.update(|cx| {
            gpui_kit::init(cx);
            super::register_about_action(cx);
        });
        cx.update(|cx| cx.dispatch_action(&About));
        cx.run_until_parked();
        let window = cx.update(|cx| *cx.windows().last().expect("About opened a window"));
        let mut vcx = gpui_kit::VisualTestContext::from_window(window, cx);
        settle(&mut vcx);
        vcx
    }

    /// A few frames: the resample landing, the fit to content, the redraw.
    fn settle(vcx: &mut gpui_kit::VisualTestContext) {
        use gpui_kit::test::TestWindowExt as _;
        for _ in 0..3 {
            vcx.run_until_parked();
            vcx.update(|window, cx| window.render_frame(cx));
        }
        vcx.run_until_parked();
    }

    /// The bug: the icon was drawn 128 wide but squeezed to 9px tall, because
    /// the column overflowed the window. It is drawn at its full 128x128.
    #[gpui_kit::test]
    fn the_icon_is_drawn_at_its_full_size(cx: &mut gpui_kit::TestAppContext) {
        use super::super::pane::ICON_SIZE;
        use gpui_kit::px;

        let mut vcx = open_about(cx);
        for selector in ["about-icon", "about-icon-raster"] {
            let icon = vcx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} is drawn"));
            assert_eq!(icon.size.width, px(ICON_SIZE), "{selector}'s width");
            assert_eq!(icon.size.height, px(ICON_SIZE), "{selector}'s height");
        }
    }

    /// The icon's raster is `round(128 x scale)` device pixels at 1x and 2x,
    /// so it samples 1:1 rather than leaving the GPU to shrink a 1254px source.
    #[test]
    fn the_icon_raster_is_its_device_size_at_1x_and_2x() {
        use super::super::pane::{ICON, ICON_SIZE};
        use crate::ui::raster::{device_width, rasterize};

        for scale in [1., 2.] {
            let width = device_width(ICON_SIZE, scale);
            assert_eq!(width, (ICON_SIZE * scale).round() as u32);
            let raster = rasterize(ICON, width).expect("the embedded icon decodes");
            let size = raster.size(0);
            assert_eq!(size.width.0 as u32, width, "at {scale}x");
            assert_eq!(size.height.0 as u32, width, "square at {scale}x");
        }
    }

    /// The window fits its content - icon, version, credits (the Icons credit
    /// included) - with the bottom inset to spare, at the default text size
    /// and at 150%, so nothing overflows and nothing is squeezed.
    #[gpui_kit::test]
    fn the_window_fits_its_content_at_every_text_size(cx: &mut gpui_kit::TestAppContext) {
        use super::super::pane::ICON_SIZE;
        use crate::config::ui::TextSize;
        use gpui_kit::px;

        let mut vcx = open_about(cx);
        for size in [TextSize::DEFAULT, TextSize::MAX] {
            vcx.update(|_, cx| crate::ui::text_size::set(size, cx));
            settle(&mut vcx);
            // The window's own bounds: the test platform applies a resize to
            // them, though not to the cached viewport size a real window's
            // resize callback refreshes.
            let viewport = vcx.update(|window, _| window.bounds().size);
            let credits = vcx
                .debug_bounds("about-credits")
                .expect("the credits are drawn");
            let inset = vcx.update(|_, cx| crate::ui::space::spacing(cx).panel_inset * 2.);
            assert!(
                credits.bottom() + inset <= viewport.height + px(1.),
                "at {}% the credits end at {:?}, past the {:?} window",
                size.percent(),
                credits.bottom(),
                viewport.height
            );
            let icon = vcx.debug_bounds("about-icon").expect("the icon is drawn");
            assert_eq!(icon.size.height, px(ICON_SIZE), "at {}%", size.percent());
        }
    }
}
