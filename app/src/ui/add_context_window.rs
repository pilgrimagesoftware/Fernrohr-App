//! The Add Context window (`add-context-window`, Fernrohr-App#232): the cluster
//! picker, in its own window, for adding a context to a workspace window that
//! already has one.
//!
//! It is a window rather than a dialog so it never blocks the workspace it adds
//! to: a slow connection or a manual tunnel waiting on the user leaves every panel
//! usable. A window with no context shows the full-window picker instead, which is
//! not this.
//!
//! One per workspace window, kept by the status bar (`StatusBarView::add_window`).
//! The context is added through the opener's own window handle: the registry hold
//! is keyed by that window, and the Pods panel opens in its dock.

use crate::command::{Command, CommandRegistry};
use crate::ui::picker::{ClusterPicker, MIN_WINDOW_SIZE, PickerEvent};
use crate::util::shell::MainWindow;
use gpui_kit::*;

actions!(add_context, [CloseAddContext]);

/// The window's key context, which scopes [`CloseAddContext`]'s Escape to it.
const KEY_CONTEXT: &str = "AddContextWindow";

/// Closing the window, as a command: Escape here, and in the palette while it has
/// focus, as Escape closed the dialog this replaces.
pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: "add_context.close",
        title: "Add Context: Close",
        default_binding: "escape",
        context: Some(KEY_CONTEXT),
        action: Box::new(CloseAddContext),
        menu: None,
    });
}

/// The window's starting size: wider than the picker's 480px card, so a long
/// generated context name has room beside its tunnel selector.
const ADD_CONTEXT_WINDOW_SIZE: Size<Pixels> = Size {
    width: px(720.),
    height: px(680.),
};

/// The window's title.
const ADD_CONTEXT_WINDOW_TITLE: &str = "Add a Context";

/// Opens an Add Context window for `main_window`, the workspace in `opener`,
/// offering every context but `exclude`. `None` if the platform refused the window.
pub(crate) fn open(
    opener: AnyWindowHandle,
    main_window: WeakEntity<MainWindow>,
    exclude: &[String],
    cx: &mut App,
) -> Option<AnyWindowHandle> {
    let options = WindowOptions {
        app_id: Some(crate::consts::APP_ID.into()),
        titlebar: Some(TitlebarOptions {
            title: Some(ADD_CONTEXT_WINDOW_TITLE.into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            ADD_CONTEXT_WINDOW_SIZE,
            cx,
        ))),
        window_min_size: Some(MIN_WINDOW_SIZE),
        ..Default::default()
    };
    let exclude = exclude.to_vec();
    match gpui_kit::open_window(options, cx, move |window, cx| {
        crate::ui::theme::watch_window(window, cx);
        cx.new(|cx| AddContextWindow::new(opener, main_window, &exclude, window, cx))
    }) {
        Ok((window, _)) => Some(window),
        Err(error) => {
            eprintln!("failed to open the Add Context window: {error}");
            None
        }
    }
}

pub(crate) struct AddContextWindow {
    picker: Entity<ClusterPicker>,
    /// Whether the picker has been given focus yet - done on first render, once
    /// its list exists to take it.
    focused: bool,
    _connected: Subscription,
}

impl AddContextWindow {
    fn new(
        opener: AnyWindowHandle,
        main_window: WeakEntity<MainWindow>,
        exclude: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let picker = cx.new(|cx| {
            let mut picker = ClusterPicker::new(window, cx);
            picker.exclude(exclude);
            picker
        });
        // `PickerEvent::Connected` fires only once the chosen context connects, so
        // a failed add stays here, showing its failure, and adds nothing.
        let connected = cx.subscribe_in(&picker, window, move |_, _, event, window, cx| {
            let PickerEvent::Connected { context_name } = event;
            let context_name = context_name.clone();
            let main_window = main_window.clone();
            // Fails only once the workspace window has closed: nothing to add to.
            let _ = opener.update(cx, |_, opener_window, cx| {
                let _ = main_window.update(cx, |main_window, cx| {
                    main_window.add_context(context_name, opener_window, cx);
                });
                opener_window.activate_window();
            });
            window.remove_window();
        });
        Self {
            picker,
            focused: false,
            _connected: connected,
        }
    }

    /// Test-only: the picker this window hosts.
    #[cfg(test)]
    pub(crate) fn picker(&self) -> &Entity<ClusterPicker> {
        &self.picker
    }
}

impl Render for AddContextWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused {
            self.focused = true;
            let focus = self.picker.read(cx).initial_focus_handle(cx);
            window.focus(&focus, cx);
        }
        div()
            .size_full()
            .key_context(KEY_CONTEXT)
            .on_action(|_: &CloseAddContext, window, _| window.remove_window())
            .child(self.picker.clone())
    }
}
