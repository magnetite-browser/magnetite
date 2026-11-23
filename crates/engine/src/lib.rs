//! Browser engine integration on top of CEF (via `cef-rs`).
//!
//! Architecture (ADR-0002, ADR-0004): one *frame* window owned by CEF Views
//! holds the sidebar (an Alloy-style browser view rendering our own UI) and
//! an empty content panel. Every loaded tab is a *content window*: a
//! frameless child window with a Chrome-style browser, positioned over the
//! content panel. The engine hides and shows content windows to switch tabs.
//!
//! All engine calls and delegate callbacks happen on the CEF UI thread. The
//! engine never calls the delegate re-entrantly: events are posted to the
//! message loop and delivered on the next iteration.

// The cef-rs wrap_* macros expand to code that trips these pedantic lints.
#![allow(
    clippy::missing_safety_doc,
    clippy::transmute_ptr_to_ptr,
    clippy::wildcard_imports,
    clippy::borrow_as_ptr
)]

mod applink;
mod autofill;
mod blocker;
mod bootstrap;
mod caption;
mod capture;
mod cef_app;
mod client;
#[cfg(target_os = "linux")]
mod clipboard;
mod config;
mod content;
mod default_browser;
mod devdock;
mod device;
mod devpipe;
mod devtools;
mod downloads;
mod empty;
mod engine;
mod events;
mod extensions;
mod favicon;
mod floatbar;
mod frame;
mod glyphs;
mod keymap;
mod keys;
mod languages;
#[cfg(target_os = "macos")]
mod mac;
mod media;
#[cfg(target_os = "macos")]
mod menubar;
mod mini;
mod notifications;
mod overlay;
mod panel_close;
#[cfg(target_os = "linux")]
mod panel_stack;
#[cfg(target_os = "linux")]
mod peek_buttons;
#[cfg(target_os = "macos")]
mod pip;
mod privacy;
mod prompts;
mod protected;
mod router;
mod scheme;
mod scriptlets;
mod shadow;
mod site;
mod sitecode;
mod sitedata;
mod splitui;
mod stray;
mod strings;
mod system;
mod tabshare;
#[cfg(target_os = "windows")]
mod win;

pub use autofill::AutofillPrefs;
pub use blocker::{Filters, List as FilterList, set_allowed_sites, set_filters};
#[cfg(target_os = "windows")]
pub use bootstrap::set_sandbox_info;
pub use bootstrap::{reserve_devtools_pipe, run, run_helper, started_by_bootstrap};
pub use capture::{FrozenFrame, PageTiles};
pub use client::host_of;
pub use config::EngineConfig;
pub use content::SPLIT_GAP;
pub use default_browser::DefaultBrowser;
pub use devdock::{DevToolsDock, DevToolsSize};
pub use downloads::DownloadPrefs;
pub use engine::{ContextProfile, Engine, EngineDelegate, WindowId};
pub use events::{
    ContentId, DevPanel, DevToolsAction, DividerAction, DownloadReport, DownloadState, EngineEvent,
    ExtensionMenuAction, MemoryPressure, Shortcut, UrlSource,
};
pub use extensions::{ExtensionInfo, is_extension_id};
pub use frame::PageFrame;
pub use keymap::{Action, Combo, First, KeysError, Overrides, Priorities};
pub use keys::set_page_first_sites;
pub use languages::LanguagePrefs;
pub use media::{MediaAction, MediaState};
pub use privacy::set_global_privacy_control;
pub use prompts::{PermissionPrompt, PromptAnswer, PromptKind};
pub use router::ReplyTicket;
pub use scheme::user_image_dir;
pub use site::{SiteState, ZOOM_STEPS, zoom_step};
pub use sitecode::SiteCode;
pub use sitedata::{
    ClearReport, PermissionEntry, PermissionKind, PermissionRow, PermissionValue, in_domain,
    registrable_domain,
};
#[cfg(target_os = "windows")]
pub use win::installer_ready;

/// The Chromium and CEF versions Magnetite is built on (Settings → About):
/// those of the CEF build it is linked against (`build.rs`), else those the
/// bindings were made from.
pub fn engine_versions() -> (String, String) {
    use cef::sys;
    let chromium = option_env!("MAGNETITE_CHROMIUM_VERSION").map_or_else(
        || {
            format!(
                "{}.{}.{}.{}",
                sys::CHROME_VERSION_MAJOR,
                sys::CHROME_VERSION_MINOR,
                sys::CHROME_VERSION_BUILD,
                sys::CHROME_VERSION_PATCH
            )
        },
        str::to_string,
    );
    let cef = option_env!("MAGNETITE_CEF_VERSION").map_or_else(
        || {
            format!(
                "{}.{}.{}",
                sys::CEF_VERSION_MAJOR,
                sys::CEF_VERSION_MINOR,
                sys::CEF_VERSION_PATCH
            )
        },
        str::to_string,
    );
    (chromium, cef)
}

/// Debug tracing to stderr when `MAGNETITE_LOG` is set.
pub(crate) fn trace(msg: &str) {
    if std::env::var_os("MAGNETITE_LOG").is_some() {
        eprintln!("engine: {msg}");
    }
}
