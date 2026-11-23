//! What the page area shows behind the pages: our own page
//! `magnetite://ui/empty`. A browser view filling the frame's content panel,
//! always shown: around the pages it is the window's frame (with the look of
//! the space), and while no page is on screen it shows the logo and how to
//! open a tab, or the user's background. The page itself tells the two apart
//! from the state it is sent, so the logo never flashes up between two tabs.

use cef::*;

use crate::client::MagnetiteClient;
use crate::engine::Engine;

/// The app icon as an image for a window (taskbar, Alt+Tab).
#[cfg(not(target_os = "macos"))]
pub(crate) fn app_icon() -> Option<Image> {
    let image = image_create()?;
    let png = include_bytes!("../../ui/web/src/assets/icon.png");
    (image.add_png(1.0, Some(png)) != 0).then_some(image)
}

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

    /// Tell the empty page where the pages on screen are (and their docked
    /// DevTools), so it draws a shadow under each: the pages are windows
    /// over it and have none of their own. Rectangles are in the content
    /// panel, whose size goes along: the page can then hold on to the
    /// panel's edges while the window is resized. A peek and a web panel
    /// over the pages are left out: they lie on pages, not on the frame. A
    /// docked web panel, or a floating one with no page under it, lies on
    /// the frame and has a shadow like them.
    pub(crate) fn update_page_shadows(&self) {
        let Some(frame) = self.empty_browser().and_then(|b| b.main_frame()) else {
            return;
        };
        // Its own statement: it locks too.
        let panel = self.content_panel_in_screen();
        let windows: Vec<Window> = {
            let inner = self.lock();
            let win = &inner.win;
            let mut ids = match win.split.as_ref() {
                Some(t) if t.pane_count() > 1 => t.ids(),
                _ => win.shown.into_iter().collect(),
            };
            if crate::content::WEB_PANEL_DOCKED || (!cfg!(target_os = "macos") && ids.is_empty()) {
                ids.extend(win.web_panel);
            }
            ids.iter()
                .flat_map(|id| {
                    let page = inner.contents.get(id).and_then(|c| c.window.clone());
                    let tools = (inner.docked.get(id))
                        .filter(|d| d.shown && !d.separate)
                        .and_then(|d| d.window.clone());
                    [page, tools]
                })
                .flatten()
                .collect()
        };
        let cards: Vec<[i32; 4]> = windows
            .iter()
            .map(|w| {
                let b = w.bounds();
                [b.x - panel.x, b.y - panel.y, b.width, b.height]
            })
            .collect();
        let at =
            serde_json::json!({ "width": panel.width, "height": panel.height, "cards": cards });
        self.eval_in_ui(
            &frame,
            &format!("window.__magnetite && window.__magnetite.pageCards({at})"),
        );
    }
}

wrap_browser_view_delegate! {
    struct EmptyViewDelegate {}
    impl ViewDelegate {}
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}
