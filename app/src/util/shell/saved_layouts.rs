//! The Save Panel Layout command (`saved-panel-layouts` tasks 2.1-2.3): the
//! naming dialog, and capturing a live window's dock, Resource panel state,
//! contexts and bounds into a [`SavedLayout`] on confirm. `layouts.manage`
//! (tasks 3.2) also lives here: it just opens [`SavedLayoutsPicker`] (`ui::
//! picker::saved_layouts`, which owns the picker view itself) as a dialog
//! over this window, in both `Workspace` and cluster-picker modes.
//!
//! `config::saved_layouts` owns the on-disk shape and plain file I/O; this
//! module is the one place that reaches into a live `MainWindow`'s
//! `WindowMode::Workspace` fields to build one, per design.md D3 - the
//! `pub(super)` fields it reads (`dock_area`, `contexts`, `resource_width`,
//! `resource_collapsed`) are visible here without widening them, since this
//! is a sibling module of `main_window.rs`/`window.rs` inside `util::shell`.
//! Loading a saved layout back (Add/Replace, tasks section 4) lives in the
//! sibling [`load`] module: `MainWindow::load_replace`/`load_add`, reached
//! through `ui::picker::saved_layouts::SavedLayoutsPicker`'s own
//! `selected()`/`main_window()` seam (that picker's `render`/`interaction`
//! own the two commands themselves and the `enter`/`secondary-enter`
//! wiring - see their own doc comments for why `enter` needs no second
//! `on_action` handler here).

mod load;

use super::*;
use crate::config::saved_layouts::{self, SavedLayout};
use crate::consts::SAVED_LAYOUT_SCHEMA_VERSION;
use crate::ui::confirm_dialog::{self, Confirmation, Severity};
use crate::ui::confirm_text::ConfirmText;
use crate::ui::picker::saved_layouts::SavedLayoutsPicker;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Enter, Input, InputEvent, InputState};
use gpui_kit::component::kbd::Kbd;

/// The name field's element id.
pub(super) const NAME_INPUT_ID: &str = "save-layout-name";
/// The Save button's element id.
pub(super) const SAVE_BUTTON_ID: &str = "save-layout-save";
/// The Cancel button's element id.
pub(super) const CANCEL_BUTTON_ID: &str = "save-layout-cancel";
/// The debug selector of the refusal line (an empty name).
pub(super) const ERROR_SELECTOR: &str = "save-layout-error";
/// The overwrite confirmation's button id prefix (`confirm_dialog::open`'s
/// `id_prefix`).
pub(super) const OVERWRITE_ID_PREFIX: &str = "save-layout-overwrite";

impl MainWindow {
    /// `layouts.save`: opens the naming dialog over a connected workspace. A
    /// no-op in `Picker` mode - the command's `Workspace` context keeps the
    /// keybinding from reaching this handler there at all, but a defensive
    /// check costs nothing if this is ever dispatched another way.
    pub(super) fn on_action_save_layout(
        &mut self,
        _: &SaveLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.mode, WindowMode::Workspace { .. }) {
            return;
        }
        open_save_dialog(cx.weak_entity(), window, cx);
    }

    /// `layouts.manage`: opens the saved layouts picker over this window - in
    /// both `Workspace` and cluster-picker modes (design.md D4's table:
    /// `context: None`), so a fresh, unconnected window can still load a
    /// saved layout straight away.
    pub(super) fn on_action_manage_layouts(
        &mut self,
        _: &ManageLayouts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        open_manage_dialog(cx.weak_entity(), window, cx);
    }
}

/// Opens the saved layouts picker, focused on its list.
fn open_manage_dialog(main_window: WeakEntity<MainWindow>, window: &mut Window, cx: &mut App) {
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let dir = layouts_dir(cx);
    let picker = cx.new(|cx| SavedLayoutsPicker::new(dir, main_window, window, cx));
    let key = Kbd::binding_for_action(&ManageLayouts, None, window).unwrap_or_else(|| {
        Kbd::new(Keystroke::parse(MANAGE_LAYOUTS_DEFAULT_BINDING).expect("a valid default key"))
    });
    let picker_for_focus = picker.clone();
    window.open_dialog(cx, move |built, _window, _cx| {
        let picker = picker.clone();
        built
            .w(px(420.))
            .title(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child("Saved Layouts…")
                    .child(key.clone()),
            )
            .content(move |content, _window, _cx| content.child(picker.clone()))
    });
    focus_picker_after_first_render(picker_for_focus, window);
}

/// Focuses `picker`'s list once its first real frame has rendered, rather
/// than right away.
///
/// `SavedLayoutsPicker::focus_handle` delegates to
/// `gpui_kit`'s `CommandState::focus_handle`, which is conditional: with no
/// query field (`searchable(false)`, set in `ui::picker::saved_layouts`'s
/// own `Render` impl) it returns the list's own handle, but that branch
/// isn't live until `CommandState`'s model has actually taken that option -
/// which happens only when the `Command` builder renders once, not at
/// construction. Reading the handle synchronously, right after `cx.new`,
/// still sees the *default* (searchable) branch: the query input's handle -
/// one that is never mounted here at all, since nothing with `searchable:
/// true`'s shape ever paints. Focusing an unmounted handle has nothing to
/// fail loudly: `window.focus` happily records it as the window's current
/// focus, but no element in the next rendered frame's dispatch tree carries
/// that id, so every subsequent keystroke's context stack resolves to the
/// window root - empty - and every one of this view's own key-bound
/// commands (and even `Command`'s own built-in up/down/enter) silently goes
/// nowhere. Deferring to `on_next_frame`, after `open_dialog` schedules the
/// content but before a test or the user sends a key, lets that first real
/// render settle the model and expose the handle that is actually on
/// screen.
fn focus_picker_after_first_render(picker: Entity<SavedLayoutsPicker>, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        let focus = picker.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
}

/// Where saved layouts are read from and written to: a test override
/// (`SavedLayoutsDir`, set once per test the way `WorkspacePath` is) when
/// one is set, otherwise [`default_saved_layouts_dir`].
///
/// `pub(crate)` (re-exported as `util::shell::layouts_dir`) rather than
/// `pub(super)`: `ui::settings::layouts` (`saved-panel-layouts` tasks 6.1)
/// needs the same directory this module's own save/manage dialogs read and
/// write, and it isn't a descendant of `util::shell` - this is the one
/// function widened for that, not `SavedLayoutsDir` itself or the
/// `saved_layouts` module path.
pub(crate) fn layouts_dir(cx: &App) -> PathBuf {
    cx.try_global::<SavedLayoutsDir>()
        .map(|dir| dir.0.clone())
        .unwrap_or_else(default_saved_layouts_dir)
}

/// Bumped after every change to the saved layouts on disk - a save, an
/// overwrite, a rename or a removal, from any window - so a view that caches
/// the list (the Settings window's Layouts section) knows to read it again,
/// off the main thread, rather than reading the directory as it renders.
#[derive(Default)]
pub(crate) struct SavedLayoutsChanged(pub(crate) u64);

impl Global for SavedLayoutsChanged {}

/// Records that the saved layouts on disk changed ([`SavedLayoutsChanged`]).
pub(crate) fn note_layouts_changed(cx: &mut App) {
    cx.default_global::<SavedLayoutsChanged>().0 += 1;
}

/// Opens the naming dialog, focused on its name field.
fn open_save_dialog(main_window: WeakEntity<MainWindow>, window: &mut Window, cx: &mut App) {
    if !matches!(window.root::<Root>(), Some(Some(_))) {
        return;
    }
    let dialog = cx.new(|cx| SaveLayoutDialog::new(main_window, window, cx));
    let name_input = dialog.read(cx).name_input.clone();
    // `layouts.save` is registered `context: None` (section 7's menu-greying
    // investigation - see its registration in `util::shell::app` for why), so
    // its binding is global like `ManageLayouts`'s own lookup just above.
    let key = Kbd::binding_for_action(&SaveLayout, None, window).unwrap_or_else(|| {
        Kbd::new(Keystroke::parse(SAVE_LAYOUT_DEFAULT_BINDING).expect("a valid default key"))
    });
    window.open_dialog(cx, move |built, _window, _cx| {
        let dialog = dialog.clone();
        built
            .w(px(420.))
            .title(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child("Save Panel Layout…")
                    .child(key.clone()),
            )
            .content(move |content, _window, _cx| content.child(dialog.clone()))
    });
    let focus = name_input.read(cx).focus_handle(cx);
    window.focus(&focus, cx);
}

/// The naming dialog's own view: a name field, a refusal line for an empty
/// name, and Cancel/Save - Tab reaches every control, Enter in the field (or
/// clicking Save) confirms, Escape cancels (the dialog component's own
/// default).
struct SaveLayoutDialog {
    main_window: WeakEntity<MainWindow>,
    name_input: Entity<InputState>,
    error: Option<SharedString>,
    _subscription: Subscription,
}

impl SaveLayoutDialog {
    fn new(
        main_window: WeakEntity<MainWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Layout name"));
        let subscription = cx.subscribe_in(
            &name_input,
            window,
            |this: &mut Self, _, event, _window, cx| {
                if let InputEvent::Change = event {
                    this.error = None;
                    cx.notify();
                }
            },
        );
        Self {
            main_window,
            name_input,
            error: None,
            _subscription: subscription,
        }
    }

    /// Validates the name, then either writes the layout at once or - a
    /// case-insensitive match against an already-saved name - asks before
    /// overwriting it (spec "Saving over an existing name").
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.error = Some("Enter a name for this layout.".into());
            cx.notify();
            return;
        }
        let Some(main_window) = self.main_window.upgrade() else {
            window.close_dialog(cx);
            return;
        };
        let dir = layouts_dir(cx);
        let collision = saved_layouts::load_all(&dir)
            .0
            .into_iter()
            .find(|layout| layout.name.eq_ignore_ascii_case(&name));
        window.close_dialog(cx);
        match collision {
            Some(existing) => confirm_overwrite(existing, main_window, dir, name, window, cx),
            None => {
                let created_at = jiff::Timestamp::now().to_string();
                if let Some(layout) = capture_layout(&main_window, window, cx, name, created_at) {
                    write_layout(&dir, layout, cx);
                }
            }
        }
    }
}

impl Render for SaveLayoutDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let danger = cx.theme().danger;
        div()
            .flex()
            .flex_col()
            .gap_3()
            // Enter in the name field saves; the input lets Enter bubble on,
            // and the dialog would otherwise take it as its own confirm and
            // close without saving.
            .child(
                div()
                    .on_action(cx.listener(|this, _: &Enter, window, cx| this.save(window, cx)))
                    .child(Input::new(&self.name_input).id(NAME_INPUT_ID)),
            )
            .children(self.error.clone().map(|error| {
                div()
                    .debug_selector(|| ERROR_SELECTOR.into())
                    .text_sm()
                    .text_color(danger)
                    .child(error)
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new(CANCEL_BUTTON_ID)
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new(SAVE_BUTTON_ID)
                            .label("Save")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}

/// Asks before overwriting `existing`: losing a saved layout's panels, size
/// and position this way can't be undone (there is no saved-layout undo or
/// trash, matching D6's reasoning for deleting one), so this is
/// `Severity::Irreversible` - Enter alone cancels, and only a deliberate
/// confirm (click, Tab+Enter/Space, or `dialog.confirm_irreversible`)
/// overwrites it, the same tier `ui/tunnels/editor/actions.rs::request_delete`
/// already uses for a full tunnel delete.
fn confirm_overwrite(
    existing: SavedLayout,
    main_window: Entity<MainWindow>,
    dir: PathBuf,
    name: String,
    window: &mut Window,
    cx: &mut App,
) {
    let confirmation = Confirmation {
        title: "Overwrite Saved Layout?".into(),
        body: ConfirmText::new()
            .text("Overwriting ")
            .name(&existing.name)
            .text(" replaces its saved panels and window size. This can't be undone."),
        confirm: "Overwrite".into(),
        id_prefix: OVERWRITE_ID_PREFIX,
        severity: Severity::Irreversible,
    };
    let created_at = existing.created_at.clone();
    confirm_dialog::open(
        confirmation,
        move |window, cx| {
            if let Some(layout) =
                capture_layout(&main_window, window, cx, name.clone(), created_at.clone())
            {
                write_layout(&dir, layout, cx);
            }
        },
        window,
        cx,
    );
}

/// Builds a [`SavedLayout`] from `main_window`'s live state: its dock dump,
/// contexts, Resource panel width and visibility (`None` when collapsed,
/// `Some(width)` otherwise - design.md D2's same meaning as
/// `WindowLayout.resource_panel_width`), and the window's current bounds
/// (the same [`restorable_bounds`] the automatic restore's own save uses).
/// `None` for a window that left `Workspace` mode between the command firing
/// and this running (closed or disconnected while the dialog was open).
fn capture_layout(
    main_window: &Entity<MainWindow>,
    window: &Window,
    cx: &App,
    name: String,
    created_at: String,
) -> Option<SavedLayout> {
    let main_window = main_window.read(cx);
    let WindowMode::Workspace {
        dock_area,
        contexts,
        resource_width,
        resource_collapsed,
        ..
    } = &main_window.mode
    else {
        return None;
    };
    let dock = dock_area.read(cx).dump(cx);
    let resource_panel_width = (!*resource_collapsed).then(|| f32::from(*resource_width));
    let contexts = contexts.clone();
    let bounds = restorable_bounds(window.bounds(), window.viewport_size());
    Some(SavedLayout {
        version: SAVED_LAYOUT_SCHEMA_VERSION,
        name,
        created_at,
        updated_at: jiff::Timestamp::now().to_string(),
        contexts,
        dock,
        resource_panel_width,
        window_width: f32::from(bounds.size.width),
        window_height: f32::from(bounds.size.height),
    })
}

/// Writes `layout`, logging rather than failing the dialog's close on an I/O
/// error - the same "report, don't crash" treatment every other save in this
/// module gives a write that fails - and notes the change.
fn write_layout(dir: &Path, layout: SavedLayout, cx: &mut App) {
    if let Err(error) = saved_layouts::save(dir, &layout) {
        log::warn!("failed to save layout {:?}: {error}", layout.name);
    }
    note_layouts_changed(cx);
}

#[cfg(test)]
mod manage_tests;
#[cfg(test)]
mod round_trip_tests;
#[cfg(test)]
mod tests;
