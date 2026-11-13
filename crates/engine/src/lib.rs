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

mod autofill;
mod bootstrap;
mod capture;
mod cef_app;
mod client;
mod config;
mod content;
mod default_browser;
mod devdock;
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
mod keys;
#[cfg(target_os = "macos")]
mod mac;
mod media;
#[cfg(target_os = "macos")]
mod menubar;
mod mini;
mod notifications;
mod overlay;
#[cfg(target_os = "macos")]
mod pip;
mod prompts;
mod router;
mod scheme;
mod site;
mod sitedata;
mod splitui;
mod stray;

pub use autofill::AutofillPrefs;
#[cfg(target_os = "windows")]
pub use bootstrap::set_sandbox_info;
pub use bootstrap::{run, run_helper, started_by_bootstrap};
pub use capture::{FrozenFrame, PageTiles};
pub use client::host_of;
pub use config::EngineConfig;
pub use content::SPLIT_GAP;
pub use default_browser::DefaultBrowser;
pub use devdock::DevToolsDock;
pub use engine::{ContextProfile, Engine, EngineDelegate};
pub use events::{
    ContentId, DevToolsAction, DividerAction, DownloadReport, DownloadState, EngineEvent,
    ExtensionMenuAction, MemoryPressure, Shortcut, UrlSource,
};
pub use extensions::ExtensionInfo;
pub use media::{MediaAction, MediaState};
pub use prompts::{PermissionPrompt, PromptAnswer, PromptKind};
pub use router::ReplyTicket;
pub use site::{SiteState, ZOOM_STEPS, zoom_step};
pub use sitedata::{
    ClearReport, PermissionKind, PermissionRow, PermissionValue, in_domain, registrable_domain,
};

/// The Chromium and CEF versions Magnetite is built on (Settings → About).
pub fn engine_versions() -> (String, String) {
    use cef::sys;
    (
        format!(
            "{}.{}.{}.{}",
            sys::CHROME_VERSION_MAJOR,
            sys::CHROME_VERSION_MINOR,
            sys::CHROME_VERSION_BUILD,
            sys::CHROME_VERSION_PATCH
        ),
        format!(
            "{}.{}.{}",
            sys::CEF_VERSION_MAJOR,
            sys::CEF_VERSION_MINOR,
            sys::CEF_VERSION_PATCH
        ),
    )
}

/// Debug tracing to stderr when `MAGNETITE_LOG` is set.
pub(crate) fn trace(msg: &str) {
    if std::env::var_os("MAGNETITE_LOG").is_some() {
        eprintln!("engine: {msg}");
    }
}
