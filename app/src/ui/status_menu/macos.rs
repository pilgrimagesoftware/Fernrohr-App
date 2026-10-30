//! The status menu as an AppKit `NSStatusItem` (quiet mode, #236): a
//! template-image icon in the menu bar, its menu built from
//! [`super::items`], from launch until the app quits. gpui has no status item
//! of its own, so this is the app's one direct AppKit code beside the accent
//! colour; `docs/adr/0001-macos-status-item.md` records why and what it relies
//! on.
//!
//! AppKit calls back on the main thread: a chosen entry and the menu about to
//! open each go into a channel the app's foreground task drains, so nothing
//! here runs app code from inside AppKit's callback. The kubeconfig's contexts
//! are read off the main thread each time the menu opens, and the menu is
//! rebuilt when they changed - an edit shows the next time it opens.

use super::{CONTEXTS, Choice, Item, NO_CONTEXTS, choose, items};
use gpui_kit::{App, Global};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSImage, NSMenu, NSMenuDelegate, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSObjectProtocol, NSString};
use std::cell::RefCell;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

/// The SF Symbol the icon shows: a telescope's nearest kin. Drawn as a
/// template image, so it follows the menu bar's light or dark appearance.
const SYMBOL: &str = "binoculars";
/// What VoiceOver and a missing symbol say instead.
const NAME: &str = "Fernrohr";

/// What AppKit reported.
enum Event {
    Chosen(Choice),
    Opening,
}

/// The menu target's state: where events go, and what each item's tag
/// stands for.
struct TargetState {
    events: UnboundedSender<Event>,
    choices: RefCell<Vec<Choice>>,
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and `Target`
    // overrides none of its methods.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "FernrohrStatusMenuTarget"]
    #[ivars = TargetState]
    struct Target;

    impl Target {
        // SAFETY: `choose:` is the action every actionable item sends, with
        // the item as its sender and no return value - this signature.
        #[unsafe(method(choose:))]
        fn choose_item(&self, item: &NSMenuItem) {
            let choice = usize::try_from(item.tag())
                .ok()
                .and_then(|ix| self.ivars().choices.borrow().get(ix).cloned());
            if let Some(choice) = choice {
                let _ = self.ivars().events.send(Event::Chosen(choice));
            }
        }
    }

    // SAFETY: `NSObjectProtocol` asks nothing beyond being an `NSObject`.
    unsafe impl NSObjectProtocol for Target {}

    // SAFETY: `menuWillOpen:` matches `NSMenuDelegate`'s declaration: the menu,
    // no return value.
    unsafe impl NSMenuDelegate for Target {
        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, _menu: &NSMenu) {
            let _ = self.ivars().events.send(Event::Opening);
        }
    }
);

impl Target {
    fn new(events: UnboundedSender<Event>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetState {
            events,
            choices: RefCell::new(Vec::new()),
        });
        // SAFETY: `init` is `NSObject`'s designated initializer, called once
        // on a freshly allocated instance whose ivars are set.
        unsafe { msg_send![super(this), init] }
    }
}

/// The status item and what keeps its menu working: AppKit holds a menu
/// item's target and the menu's delegate weakly, so the target lives here,
/// for the app's lifetime, with the item.
struct StatusItem {
    _item: Retained<NSStatusItem>,
    menu: Retained<NSMenu>,
    target: Retained<Target>,
    /// The contexts the menu lists now; `None` until first read.
    contexts: Option<Vec<String>>,
}

impl Global for StatusItem {}

/// Puts Fernrohr's icon in the menu bar, with its menu. Called once at launch.
pub fn install(cx: &mut App) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("the status item can only be made on the main thread");
        return;
    };
    let (events, mut received) = unbounded_channel();
    let target = Target::new(events, mtm);
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    if let Some(button) = item.button(mtm) {
        let symbol = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(SYMBOL),
            Some(&NSString::from_str(NAME)),
        );
        match symbol {
            Some(image) => {
                image.setTemplate(true);
                button.setImage(Some(&image));
            }
            None => button.setTitle(&NSString::from_str(NAME)),
        }
    }
    let menu = NSMenu::new(mtm);
    // Entries stay as built: the one disabled entry is "no contexts".
    menu.setAutoenablesItems(false);
    menu.setDelegate(Some(ProtocolObject::from_ref(&*target)));
    item.setMenu(Some(&menu));
    fill(&menu, &target, &[], mtm);
    cx.set_global(StatusItem {
        _item: item,
        menu,
        target,
        contexts: None,
    });
    refresh(cx);
    cx.spawn(async move |cx| {
        while let Some(event) = received.recv().await {
            cx.update(|cx| match event {
                Event::Chosen(choice) => choose(&choice, cx),
                Event::Opening => refresh(cx),
            });
        }
    })
    .detach();
}

/// Reads the kubeconfig's contexts off the main thread, then rebuilds the
/// menu if they changed.
fn refresh(cx: &mut App) {
    let read = cx.background_executor().spawn(async {
        crate::k8s::cluster::kubeconfig::list_context_names(None).unwrap_or_default()
    });
    cx.spawn(async move |cx| {
        let contexts = read.await;
        cx.update(|cx| rebuild(contexts, cx));
    })
    .detach();
}

fn rebuild(contexts: Vec<String>, cx: &mut App) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(status) = cx.try_global::<StatusItem>() else {
        return;
    };
    if status.contexts.as_ref() == Some(&contexts) {
        return;
    }
    fill(&status.menu, &status.target, &contexts, mtm);
    cx.global_mut::<StatusItem>().contexts = Some(contexts);
}

/// Replaces `menu`'s entries with [`items`] for `contexts`, each actionable
/// one sending `choose:` to `target` with its index into the target's
/// choices as its tag.
fn fill(menu: &NSMenu, target: &Target, contexts: &[String], mtm: MainThreadMarker) {
    menu.removeAllItems();
    let mut choices = Vec::new();
    for item in items(contexts) {
        match item {
            Item::Contexts(names) => {
                let parent = entry(CONTEXTS, None, target, mtm);
                let submenu =
                    NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(CONTEXTS));
                submenu.setAutoenablesItems(false);
                if names.is_empty() {
                    let none = entry(NO_CONTEXTS, None, target, mtm);
                    none.setEnabled(false);
                    submenu.addItem(&none);
                }
                for name in names {
                    choices.push(Choice::Context(name.clone()));
                    submenu.addItem(&entry(&name, Some(choices.len() - 1), target, mtm));
                }
                parent.setSubmenu(Some(&submenu));
                menu.addItem(&parent);
            }
            Item::Command { title, choice } => {
                choices.push(choice);
                menu.addItem(&entry(title, Some(choices.len() - 1), target, mtm));
            }
            Item::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
        }
    }
    *target.ivars().choices.borrow_mut() = choices;
}

/// A menu entry titled `title`: one sending `choose:` to `target` with `tag`,
/// or - with none - a submenu's parent or a label.
fn entry(
    title: &str,
    tag: Option<usize>,
    target: &Target,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let action = tag.map(|_| sel!(choose:));
    // SAFETY: the action, when there is one, is `choose:`, which `Target`
    // implements and which every item given it targets below.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(""),
        )
    };
    if let Some(tag) = tag.and_then(|tag| isize::try_from(tag).ok()) {
        item.setTag(tag);
        // SAFETY: an item holds its target weakly; `target` lives in the
        // `StatusItem` global for the app's lifetime, so it outlives every
        // item, and it answers `choose:`.
        unsafe { item.setTarget(Some(target)) };
    }
    item
}
