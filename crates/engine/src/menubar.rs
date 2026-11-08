//! The macOS menu bar.
//!
//! Items for our own commands send `EngineEvent::Shortcut` from no page, so
//! the app applies them to the page in front; with a mini window in front,
//! from its page. Text editing, windows and the
//! app menu use the standard `AppKit` actions, which reach the focused page or
//! window through the responder chain (copy and paste in pages need them).
//!
//! Keys keep their routes: a key our handlers or the page take never reaches
//! the menu, because Chromium treats a main-menu key as its own command only
//! for `commandDispatch:`, `performClose:` and `terminate:` items. The menu
//! acts on clicks, and on keys nothing else handled (⌘, inside a page).
//!
//! The Extensions menu is rebuilt each time it opens, from the extensions of
//! the profile the app names (`set_extensions`).

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOn, NSEventModifierFlags, NSImage, NSMenu, NSMenuDelegate,
    NSMenuItem,
};
use objc2_foundation::{NSData, NSSize, NSString};

use crate::engine::ContextProfile;
use crate::engine::Engine;
use crate::events::{EngineEvent, ExtensionMenuAction, Shortcut};

/// The product name in the app menu's items.
const APP: &str = "Magnetite";

define_class!(
    /// Receives the items for our commands; the item's tag says which.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct MagnetiteMenuTarget;

    impl MagnetiteMenuTarget {
        #[unsafe(method(runCommand:))]
        fn run_command(&self, sender: Option<&NSMenuItem>) {
            let Some(index) = sender.and_then(|s| usize::try_from(s.tag()).ok()) else {
                return;
            };
            if let Some(shortcut) = COMMANDS.with(|c| c.borrow().get(index).copied()) {
                // With a mini window in front, commands are for its page.
                let engine = Engine::global();
                if let Some(mini) = engine.key_mini() {
                    engine.emit(EngineEvent::Shortcut { from: Some(mini), shortcut });
                    return;
                }
                // "Close Tab" from a window that is not ours closes that
                // window, not the tab in front of the main one.
                if shortcut == Shortcut::CloseTab
                    && let Some(frame) = Engine::global().frame_window()
                    && crate::mac::close_key_window_outside(cef::ImplWindow::window_handle(&frame))
                {
                    return;
                }
                Engine::global().emit(EngineEvent::Shortcut {
                    from: None,
                    shortcut,
                });
            }
        }

        #[unsafe(method(openExtension:))]
        fn open_extension(&self, sender: Option<&NSMenuItem>) {
            extension_choice(sender, ExtensionMenuAction::Open);
        }

        #[unsafe(method(pinExtension:))]
        fn pin_extension(&self, sender: Option<&NSMenuItem>) {
            extension_choice(sender, ExtensionMenuAction::TogglePin);
        }

        #[unsafe(method(manageExtensions:))]
        fn manage_extensions(&self, _sender: Option<&NSMenuItem>) {
            Engine::global().emit(EngineEvent::ExtensionMenu(ExtensionMenuAction::Manage));
        }

        #[unsafe(method(extensionStore:))]
        fn extension_store(&self, _sender: Option<&NSMenuItem>) {
            Engine::global().emit(EngineEvent::ExtensionMenu(ExtensionMenuAction::Store));
        }
    }

    unsafe impl NSObjectProtocol for MagnetiteMenuTarget {}

    unsafe impl NSMenuDelegate for MagnetiteMenuTarget {
        // The Extensions menu, built fresh each time it opens.
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            fill_extensions(menu, self);
        }
    }
);

/// The extension of a dynamic item (its tag) with what to do with it.
fn extension_choice(sender: Option<&NSMenuItem>, action: fn(String) -> ExtensionMenuAction) {
    let Some(index) = sender.and_then(|s| usize::try_from(s.tag()).ok()) else {
        return;
    };
    if let Some(id) = LISTED.with(|e| e.borrow().get(index).cloned()) {
        Engine::global().emit(EngineEvent::ExtensionMenu(action(id)));
    }
}

/// Whose extensions to list, and which are in the sidebar (from the app).
pub(crate) fn set_extensions(profile: Option<ContextProfile>, pinned: Vec<String>) {
    SOURCE.with(|s| *s.borrow_mut() = (profile, pinned));
}

/// Fill the Extensions menu: each extension (opens it for the page in
/// front), which ones show in the sidebar, then managing and installing.
fn fill_extensions(menu: &NSMenu, target: &MagnetiteMenuTarget) {
    let mtm = target.mtm();
    menu.removeAllItems();
    let (profile, pinned) = SOURCE.with(|s| s.borrow().clone());
    let items = Engine::global().extensions(profile.as_ref());
    LISTED.with(|l| *l.borrow_mut() = items.iter().map(|e| e.id.clone()).collect());
    let target: &AnyObject = target;
    let add = |menu: &NSMenu, title: &str, action: Option<Sel>, tag: usize| {
        // SAFETY: the target implements every action used here.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                action,
                &NSString::new(),
            )
        };
        if action.is_some() {
            // SAFETY: as above; the target lives as long as the app.
            unsafe { item.setTarget(Some(target)) };
        }
        item.setTag(isize::try_from(tag).unwrap_or(isize::MAX));
        menu.addItem(&item);
        item
    };
    if items.is_empty() {
        add(menu, "No Extensions Installed", None, 0).setEnabled(false);
    }
    for (i, e) in items.iter().enumerate() {
        let item = add(menu, &e.name, Some(sel!(openExtension:)), i);
        // Read with the size limit every extension file gets (`icon_bytes`).
        if let Some(bytes) = Engine::global()
            .extension_icon(&e.id)
            .and_then(|path| crate::extensions::icon_bytes(&path))
            && let Some(image) =
                NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&bytes))
        {
            image.setSize(NSSize::new(16.0, 16.0));
            item.setImage(Some(&image));
        }
    }
    if !items.is_empty() {
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let pins =
            NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Show in Sidebar"));
        for (i, e) in items.iter().enumerate() {
            let item = add(&pins, &e.name, Some(sel!(pinExtension:)), i);
            if pinned.contains(&e.id) {
                item.setState(NSControlStateValueOn);
            }
        }
        add(menu, "Show in Sidebar", None, 0).setSubmenu(Some(&pins));
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    add(menu, "Manage Extensions…", Some(sel!(manageExtensions:)), 0);
    add(menu, "Chrome Web Store…", Some(sel!(extensionStore:)), 0);
}

thread_local! {
    /// Our commands by item tag, and their target (menu items hold targets weakly).
    static COMMANDS: RefCell<Vec<Shortcut>> = const { RefCell::new(Vec::new()) };
    static TARGET: RefCell<Option<Retained<MagnetiteMenuTarget>>> = const { RefCell::new(None) };
    /// The Extensions menu's source: profile and pinned ids (from the app).
    static SOURCE: RefCell<(Option<ContextProfile>, Vec<String>)> = const { RefCell::new((None, Vec::new())) };
    /// The listed extensions' ids by item tag.
    static LISTED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

const CMD: NSEventModifierFlags = NSEventModifierFlags::Command;
const SHIFT_CMD: NSEventModifierFlags =
    NSEventModifierFlags::Command.union(NSEventModifierFlags::Shift);
const OPT_CMD: NSEventModifierFlags =
    NSEventModifierFlags::Command.union(NSEventModifierFlags::Option);
const CTRL: NSEventModifierFlags = NSEventModifierFlags::Control;
const CTRL_SHIFT: NSEventModifierFlags =
    NSEventModifierFlags::Control.union(NSEventModifierFlags::Shift);
const OPT_SHIFT_CMD: NSEventModifierFlags = OPT_CMD.union(NSEventModifierFlags::Shift);
const NONE: NSEventModifierFlags = NSEventModifierFlags::empty();

/// The function-key characters `AppKit` uses for the arrow keys.
const LEFT: &str = "\u{F702}";
const RIGHT: &str = "\u{F703}";

/// Builds the menus: items are added to the menu last started.
struct Builder {
    mtm: MainThreadMarker,
    bar: Retained<NSMenu>,
    menu: Option<Retained<NSMenu>>,
    target: Retained<MagnetiteMenuTarget>,
    commands: Vec<Shortcut>,
}

impl Builder {
    /// Start the menu `title`; returns it for registering with the app.
    fn menu(&mut self, title: &str) -> Retained<NSMenu> {
        let title = NSString::from_str(title);
        let menu = NSMenu::initWithTitle(NSMenu::alloc(self.mtm), &title);
        let item = NSMenuItem::new(self.mtm);
        item.setTitle(&title);
        item.setSubmenu(Some(&menu));
        self.bar.addItem(&item);
        self.menu = Some(menu.clone());
        menu
    }

    /// An item for one of our commands.
    fn command(&mut self, title: &str, key: &str, mods: NSEventModifierFlags, shortcut: Shortcut) {
        let item = self.item(title, sel!(runCommand:), key, mods);
        let target: &AnyObject = &self.target;
        // SAFETY: the target implements runCommand: and lives as long as the app.
        unsafe { item.setTarget(Some(target)) };
        item.setTag(isize::try_from(self.commands.len()).unwrap_or(isize::MAX));
        self.commands.push(shortcut);
    }

    /// An item for an `AppKit` action, sent along the responder chain.
    fn standard(&mut self, title: &str, action: Sel, key: &str, mods: NSEventModifierFlags) {
        self.item(title, action, key, mods);
    }

    fn item(
        &mut self,
        title: &str,
        action: Sel,
        key: &str,
        mods: NSEventModifierFlags,
    ) -> Retained<NSMenuItem> {
        // SAFETY: AppKit only sends `action` to objects that respond to it.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        item.setKeyEquivalentModifierMask(mods);
        if let Some(menu) = &self.menu {
            menu.addItem(&item);
        }
        item
    }

    fn separator(&self) {
        if let Some(menu) = &self.menu {
            menu.addItem(&NSMenuItem::separatorItem(self.mtm));
        }
    }

    /// A submenu item in the current menu.
    fn submenu(&self, title: &str) -> Retained<NSMenu> {
        let title = NSString::from_str(title);
        let menu = NSMenu::initWithTitle(NSMenu::alloc(self.mtm), &title);
        let item = NSMenuItem::new(self.mtm);
        item.setTitle(&title);
        item.setSubmenu(Some(&menu));
        if let Some(current) = &self.menu {
            current.addItem(&item);
        }
        menu
    }
}

/// Put Magnetite's menus in the menu bar. Called once at start.
pub fn install(mtm: MainThreadMarker) {
    use Shortcut as S;
    // SAFETY: init is NSObject's designated initialiser.
    let target: Retained<MagnetiteMenuTarget> =
        unsafe { msg_send![MagnetiteMenuTarget::alloc(mtm), init] };
    let mut b = Builder {
        mtm,
        bar: NSMenu::new(mtm),
        menu: None,
        target: target.clone(),
        commands: Vec::new(),
    };
    let app = NSApplication::sharedApplication(mtm);

    b.menu(APP);
    b.standard(
        &format!("About {APP}"),
        sel!(orderFrontStandardAboutPanel:),
        "",
        NONE,
    );
    b.separator();
    b.command("Settings…", ",", CMD, S::Settings);
    b.separator();
    let services = b.submenu("Services");
    app.setServicesMenu(Some(&services));
    b.separator();
    b.standard(&format!("Hide {APP}"), sel!(hide:), "h", CMD);
    b.standard("Hide Others", sel!(hideOtherApplications:), "h", OPT_CMD);
    b.standard("Show All", sel!(unhideAllApplications:), "", NONE);
    b.separator();
    b.standard(&format!("Quit {APP}"), sel!(terminate:), "q", CMD);

    b.menu("File");
    b.command("New Tab", "t", CMD, S::NewTab);
    b.command("New Space…", "n", SHIFT_CMD, S::NewSpace);
    b.command("New Split View…", "=", SHIFT_CMD, S::NewSplit);
    b.command("New Mini Window", "n", OPT_CMD, S::NewMini);
    b.command("Open Location…", "l", CMD, S::FocusAddress);
    b.separator();
    b.command("Take Screenshot…", "2", SHIFT_CMD, S::Capture);
    b.command("Print…", "p", CMD, S::Print);
    b.separator();
    b.command("Close Tab", "w", CMD, S::CloseTab);

    b.menu("Edit");
    b.standard("Undo", sel!(undo:), "z", CMD);
    b.standard("Redo", sel!(redo:), "z", SHIFT_CMD);
    b.separator();
    b.standard("Cut", sel!(cut:), "x", CMD);
    b.standard("Copy", sel!(copy:), "c", CMD);
    b.standard("Paste", sel!(paste:), "v", CMD);
    b.standard(
        "Paste and Match Style",
        sel!(pasteAndMatchStyle:),
        "v",
        OPT_SHIFT_CMD,
    );
    b.standard("Delete", sel!(delete:), "", NONE);
    b.standard("Select All", sel!(selectAll:), "a", CMD);
    b.separator();
    b.command("Copy Link", "c", SHIFT_CMD, S::CopyLink);

    b.menu("View");
    b.command("Toggle Sidebar", "s", CMD, S::ToggleSidebar);
    b.command("Reload Page", "r", CMD, S::Reload);
    b.separator();
    b.command("Zoom In", "+", CMD, S::ZoomIn);
    b.command("Zoom Out", "-", CMD, S::ZoomOut);
    b.command("Actual Size", "0", CMD, S::ZoomReset);
    b.separator();
    b.command("Developer Tools", "i", OPT_CMD, S::ToggleDevTools);

    b.menu("History");
    b.command("Back", "[", CMD, S::Back);
    b.command("Forward", "]", CMD, S::Forward);
    b.separator();
    b.command("Show History", "y", CMD, S::History);
    b.command("Reopen Closed Tab", "t", SHIFT_CMD, S::ReopenClosed);

    b.menu("Tabs");
    b.command("Next Tab", "\t", CTRL, S::NextTab);
    b.command("Previous Tab", "\t", CTRL_SHIFT, S::PrevTab);
    b.separator();
    b.command("Pin or Unpin Tab", "d", CMD, S::TogglePin);
    b.command("Site Controls…", "o", SHIFT_CMD, S::SitePanel);

    b.menu("Spaces");
    b.command("Next Space", RIGHT, OPT_CMD, S::NextSpace);
    b.command("Previous Space", LEFT, OPT_CMD, S::PrevSpace);

    let extensions = b.menu("Extensions");
    extensions.setDelegate(Some(ProtocolObject::from_ref(&*target)));
    fill_extensions(&extensions, &target);

    let window = b.menu("Window");
    b.standard("Minimize", sel!(performMiniaturize:), "m", CMD);
    b.standard("Zoom", sel!(performZoom:), "", NONE);
    b.separator();
    b.standard("Bring All to Front", sel!(arrangeInFront:), "", NONE);
    b.separator();
    b.command("Downloads", "l", OPT_CMD, S::Downloads);
    app.setWindowsMenu(Some(&window));

    // AppKit adds its search field to the help menu.
    let help = b.menu("Help");
    app.setHelpMenu(Some(&help));

    app.setMainMenu(Some(&b.bar));
    COMMANDS.with(|c| *c.borrow_mut() = b.commands);
    TARGET.with(|t| *t.borrow_mut() = Some(target));
}
