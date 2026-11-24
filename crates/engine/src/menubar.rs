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
use crate::events::DevPanel;
use crate::events::{EngineEvent, ExtensionMenuAction, Shortcut};
use crate::keymap::{Action, Combo};
use magnetite_ui::t;

/// The app menu's title (macOS shows the app's name there).
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
                // The last window is closed: a command brings it back, and
                // that is all a new window or a close asks for.
                if engine.frame_hidden() {
                    engine.activate_frame();
                    if matches!(
                        shortcut,
                        Shortcut::NewWindow | Shortcut::CloseWindow | Shortcut::CloseTab
                    ) {
                        return;
                    }
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
        add(menu, t!("menu.extensions.none"), None, 0).setEnabled(false);
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
        let pins = NSMenu::initWithTitle(
            NSMenu::alloc(mtm),
            &NSString::from_str(t!("menu.extensions.show_in_sidebar")),
        );
        for (i, e) in items.iter().enumerate() {
            let item = add(&pins, &e.name, Some(sel!(pinExtension:)), i);
            if pinned.contains(&e.id) {
                item.setState(NSControlStateValueOn);
            }
        }
        add(menu, t!("menu.extensions.show_in_sidebar"), None, 0).setSubmenu(Some(&pins));
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    add(
        menu,
        t!("menu.extensions.manage"),
        Some(sel!(manageExtensions:)),
        0,
    );
    add(
        menu,
        t!("menu.extensions.store"),
        Some(sel!(extensionStore:)),
        0,
    );
}

thread_local! {
    /// Our commands by item tag, and their target (menu items hold targets weakly).
    static COMMANDS: RefCell<Vec<Shortcut>> = const { RefCell::new(Vec::new()) };
    /// The items of our commands by tag, each with the keys it was made with.
    static ITEMS: RefCell<Vec<(Retained<NSMenuItem>, String, NSEventModifierFlags)>> = const { RefCell::new(Vec::new()) };
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
    items: Vec<(Retained<NSMenuItem>, String, NSEventModifierFlags)>,
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
        self.items.push((item, key.to_string(), mods));
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
        items: Vec::new(),
    };
    let app = NSApplication::sharedApplication(mtm);

    b.menu(APP);
    b.standard(
        t!("menu.app.about"),
        sel!(orderFrontStandardAboutPanel:),
        "",
        NONE,
    );
    b.separator();
    b.command(t!("menu.app.settings"), ",", CMD, S::Settings);
    b.separator();
    let services = b.submenu(t!("menu.app.services"));
    app.setServicesMenu(Some(&services));
    b.separator();
    b.standard(t!("menu.app.hide"), sel!(hide:), "h", CMD);
    b.standard(
        t!("menu.app.hide_others"),
        sel!(hideOtherApplications:),
        "h",
        OPT_CMD,
    );
    b.standard(
        t!("menu.app.show_all"),
        sel!(unhideAllApplications:),
        "",
        NONE,
    );
    b.separator();
    b.standard(t!("menu.app.quit"), sel!(terminate:), "q", CMD);

    b.menu(t!("menu.file.title"));
    b.command(t!("menu.file.new_tab"), "t", CMD, S::NewTab);
    b.command(t!("menu.file.new_window"), "n", CMD, S::NewWindow);
    b.command(t!("menu.file.new_space"), "n", SHIFT_CMD, S::NewSpace);
    b.command(
        t!("menu.file.private_space"),
        "p",
        SHIFT_CMD,
        S::PrivateSpace,
    );
    b.command(
        t!("menu.file.hide_private_space"),
        "d",
        SHIFT_CMD,
        S::HidePrivateSpace,
    );
    b.command(
        t!("menu.file.close_private_space"),
        "x",
        SHIFT_CMD,
        S::ClosePrivateSpace,
    );
    b.command(t!("menu.file.new_split"), "=", SHIFT_CMD, S::NewSplit);
    b.command(t!("menu.file.new_mini"), "n", OPT_CMD, S::NewMini);
    b.command(t!("menu.file.open_location"), "l", CMD, S::FocusAddress);
    b.separator();
    b.command(t!("menu.file.screenshot"), "2", SHIFT_CMD, S::Capture);
    b.command(t!("menu.file.print"), "p", CMD, S::Print);
    b.separator();
    b.command(t!("menu.file.close_tab"), "w", CMD, S::CloseTab);
    b.command(t!("menu.file.close_window"), "w", SHIFT_CMD, S::CloseWindow);

    b.menu(t!("menu.edit.title"));
    b.standard(t!("menu.edit.undo"), sel!(undo:), "z", CMD);
    b.standard(t!("menu.edit.redo"), sel!(redo:), "z", SHIFT_CMD);
    b.separator();
    b.standard(t!("menu.edit.cut"), sel!(cut:), "x", CMD);
    b.standard(t!("menu.edit.copy"), sel!(copy:), "c", CMD);
    b.standard(t!("menu.edit.paste"), sel!(paste:), "v", CMD);
    b.standard(
        t!("menu.edit.paste_match_style"),
        sel!(pasteAndMatchStyle:),
        "v",
        OPT_SHIFT_CMD,
    );
    b.standard(t!("menu.edit.delete"), sel!(delete:), "", NONE);
    b.standard(t!("menu.edit.select_all"), sel!(selectAll:), "a", CMD);
    b.separator();
    b.command(t!("menu.edit.copy_link"), "c", SHIFT_CMD, S::CopyLink);
    b.separator();
    b.command(t!("menu.edit.find"), "f", CMD, S::Find);
    b.command(t!("menu.edit.find_next"), "g", CMD, S::FindNext);
    b.command(
        t!("menu.edit.find_previous"),
        "g",
        SHIFT_CMD,
        S::FindPrevious,
    );

    b.menu(t!("menu.view.title"));
    b.command(t!("menu.view.toggle_sidebar"), "s", CMD, S::ToggleSidebar);
    b.command(t!("menu.view.toggle_web_panel"), "p", OPT_CMD, S::WebPanel);
    b.command(t!("menu.view.reload"), "r", CMD, S::Reload);
    b.command(
        t!("menu.view.reload_without_cache"),
        "r",
        SHIFT_CMD,
        S::HardReload,
    );
    b.separator();
    b.command(t!("menu.view.zoom_in"), "+", CMD, S::ZoomIn);
    b.command(t!("menu.view.zoom_out"), "-", CMD, S::ZoomOut);
    b.command(t!("menu.view.actual_size"), "0", CMD, S::ZoomReset);
    b.separator();
    b.command(
        t!("menu.view.developer_tools"),
        "i",
        OPT_CMD,
        S::ToggleDevTools,
    );
    b.command(
        t!("menu.view.console"),
        "j",
        OPT_CMD,
        S::DevPanel(DevPanel::Console),
    );
    b.command(
        t!("menu.view.pick_element"),
        "c",
        OPT_CMD,
        S::DevPanel(DevPanel::Picker),
    );
    b.command(
        t!("menu.view.network"),
        "e",
        OPT_CMD,
        S::DevPanel(DevPanel::Network),
    );
    b.command(t!("menu.view.reset_page"), "r", OPT_CMD, S::DevReset);

    b.menu(t!("menu.history.title"));
    b.command(t!("menu.history.back"), "[", CMD, S::Back);
    b.command(t!("menu.history.forward"), "]", CMD, S::Forward);
    b.separator();
    b.command(t!("menu.history.show_history"), "y", CMD, S::History);
    b.command(
        t!("menu.history.reopen_closed_tab"),
        "t",
        SHIFT_CMD,
        S::ReopenClosed,
    );

    b.menu(t!("menu.tabs.title"));
    b.command(t!("menu.tabs.next_tab"), "\t", CTRL, S::NextTab);
    b.command(t!("menu.tabs.previous_tab"), "\t", CTRL_SHIFT, S::PrevTab);
    b.separator();
    b.command(t!("menu.tabs.toggle_pin"), "d", CMD, S::TogglePin);
    b.command(t!("menu.tabs.site_controls"), "o", SHIFT_CMD, S::SitePanel);

    b.menu(t!("menu.spaces.title"));
    b.command(t!("menu.spaces.next_space"), RIGHT, OPT_CMD, S::NextSpace);
    b.command(
        t!("menu.spaces.previous_space"),
        LEFT,
        OPT_CMD,
        S::PrevSpace,
    );
    b.command(t!("menu.spaces.show_all"), "0", CTRL, S::SpacePicker);

    let extensions = b.menu(t!("menu.extensions.title"));
    extensions.setDelegate(Some(ProtocolObject::from_ref(&*target)));
    fill_extensions(&extensions, &target);

    let window = b.menu(t!("menu.window.title"));
    b.standard(
        t!("menu.window.minimize"),
        sel!(performMiniaturize:),
        "m",
        CMD,
    );
    b.standard(t!("menu.window.zoom"), sel!(performZoom:), "", NONE);
    b.separator();
    b.standard(
        t!("menu.window.bring_all_to_front"),
        sel!(arrangeInFront:),
        "",
        NONE,
    );
    b.separator();
    b.command(t!("menu.window.downloads"), "l", OPT_CMD, S::Downloads);
    app.setWindowsMenu(Some(&window));

    // AppKit adds its search field to the help menu.
    let help = b.menu(t!("menu.help.title"));
    app.setHelpMenu(Some(&help));

    app.setMainMenu(Some(&b.bar));
    COMMANDS.with(|c| *c.borrow_mut() = b.commands);
    ITEMS.with(|i| *i.borrow_mut() = b.items);
    TARGET.with(|t| *t.borrow_mut() = Some(target));
    apply_keys();
}

/// The character `AppKit` wants as the key equivalent of `combo`'s key.
fn key_equivalent(combo: Combo) -> Option<String> {
    let name = combo.key_name();
    Some(match name.as_str() {
        "Tab" => "\t".into(),
        "Enter" => "\r".into(),
        "Space" => " ".into(),
        "Backspace" => "\u{8}".into(),
        "Delete" => "\u{7F}".into(),
        "Up" => "\u{F700}".into(),
        "Down" => "\u{F701}".into(),
        "Left" => LEFT.into(),
        "Right" => RIGHT.into(),
        "Home" => "\u{F729}".into(),
        "End" => "\u{F72B}".into(),
        "PageUp" => "\u{F72C}".into(),
        "PageDown" => "\u{F72D}".into(),
        _ => match name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
            // NSF1FunctionKey and the eleven after it.
            Some(n) => char::from_u32(0xF704 + n - 1)?.to_string(),
            None => name.to_lowercase(),
        },
    })
}

/// Show the user's keys beside the items (Settings → Shortcuts): an item
/// whose action has other keys, or none, gets those. While Settings records
/// keys no item of ours has any, so the menu bar takes none of them.
pub fn apply_keys() {
    let recording = crate::keys::paused();
    let commands = COMMANDS.with(|c| c.borrow().clone());
    ITEMS.with(|items| {
        for ((item, key, mods), shortcut) in items.borrow().iter().zip(commands) {
            let own = match Action::of(shortcut) {
                Some(action) => crate::keys::combo_of(action).and_then(|combo| {
                    let mut mods = NSEventModifierFlags::empty();
                    for (held, flag) in [
                        (combo.primary, NSEventModifierFlags::Command),
                        (combo.ctrl, NSEventModifierFlags::Control),
                        (combo.alt, NSEventModifierFlags::Option),
                        (combo.shift, NSEventModifierFlags::Shift),
                    ] {
                        if held {
                            mods |= flag;
                        }
                    }
                    Some((key_equivalent(combo)?, mods))
                }),
                None => Some((key.clone(), *mods)),
            };
            let (key, mods) = own.filter(|_| !recording).unwrap_or((String::new(), NONE));
            item.setKeyEquivalent(&NSString::from_str(&key));
            item.setKeyEquivalentModifierMask(mods);
        }
    });
}
