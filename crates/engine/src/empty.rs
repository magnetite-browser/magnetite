//! What the page area shows behind the pages: our own page
//! `magnetite://ui/empty`. A browser view filling the frame's content panel,
//! always shown: around the pages it is the window's frame (with the look of
//! the space), and while no page is on screen it shows the logo and how to
//! open a tab, or the user's background. The page itself tells the two apart
//! from the state it is sent, so the logo never flashes up between two tabs.

use cef::*;

use crate::client::MagnetiteClient;
use crate::engine::Engine;

/// The view in the content panel.
pub(crate) struct EmptyPage {
    view: BrowserView,
}

impl EmptyPage {
    /// Fill `panel` with the page. `argb` is what it shows until it has
    /// loaded. Call without the engine's lock held.
    pub(crate) fn new(panel: &Panel, argb: u32) -> Option<Self> {
        panel.set_to_fill_layout()?;
        let settings = BrowserSettings {
            background_color: argb,
            ..Default::default()
        };
        // Next to the sidebar's page, also on a development server.
        let ui = &Engine::global().config().ui_url;
        let url = format!("{}/empty", ui.trim_end_matches('/'));
        let view = browser_view_create(
            Some(&mut MagnetiteClient::overlay()),
            Some(&CefString::from(url.as_str())),
            Some(&settings),
            None,
            None,
            Some(&mut EmptyViewDelegate::new()),
        )?;
        let mut child = View::from(&view);
        panel.add_child_view(Some(&mut child));
        Some(Self { view })
    }

    pub(crate) fn browser(&self) -> Option<Browser> {
        self.view.browser()
    }
}

impl Engine {
    /// The empty page's browser in the current window.
    pub(crate) fn empty_browser(&self) -> Option<Browser> {
        (self.lock().win.frame.as_ref()).and_then(|f| f.empty.as_ref()?.browser())
    }
}

wrap_browser_view_delegate! {
    struct EmptyViewDelegate {}
    impl ViewDelegate {}
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}
