//! Split view chrome around the pages: dividers in the gaps between split
//! pages (native windows on macOS and Windows, `MagnetiteDividerView` in `mac/chrome.rs` and
//! `win/dividers.rs`: drag to resize,
//! double-click to even out), and screenshots for the drop-zone overlay
//! shown while a tab is dragged over the page area.

use cef::*;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use magnetite_core::split::Axis;
use magnetite_core::split::SplitNode;

use crate::content::SPLIT_GAP;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::content::rect_of;
use crate::engine::Engine;
use crate::events::ContentId;

impl Engine {
    /// Make the split dividers match `tree` (none when there is no split).
    pub(crate) fn update_dividers(&self, tree: Option<&SplitNode<ContentId>>) {
        let dividers = tree
            .filter(|t| t.pane_count() > 1)
            .map(|t| t.dividers(self.split_area(), SPLIT_GAP))
            .unwrap_or_default();
        self.lock().win.split_dividers = dividers;
        self.show_dividers();
    }

    /// Show the split dividers and one beside each docked `DevTools` on
    /// screen, and the resize handles of pages in device mode (native
    /// windows on macOS and Windows; Linux has none yet).
    pub(crate) fn show_dividers(&self) {
        #[cfg(target_os = "macos")]
        {
            let (wanted, frame, peek) = {
                let inner = self.lock();
                let row = inner.devtools_dock != crate::DevToolsDock::Bottom;
                let split = inner.win.split_dividers.iter().map(|d| {
                    let key = crate::mac::DividerKey::Split {
                        row: d.axis == Axis::Row,
                        path: d.path.clone(),
                        index: d.index,
                    };
                    (rect_of(d.bounds), key)
                });
                let devtools = (inner.docked.iter())
                    .filter(|(_, d)| d.shown && !d.separate)
                    .filter_map(|(id, d)| {
                        Some((
                            d.gap.clone()?,
                            crate::mac::DividerKey::DevTools { id: *id, row },
                        ))
                    });
                // The resize handles of pages in device mode ("Responsive").
                let handles = (inner.docked.iter())
                    .filter(|(_, d)| d.shown)
                    .filter_map(|(id, d)| Some((*id, d.device.as_ref()?)))
                    .flat_map(|(id, m)| {
                        let key = move |edge| crate::mac::DividerKey::Device { id, edge };
                        m.handles
                            .iter()
                            .map(move |(r, edge)| (r.clone(), key(*edge)))
                    });
                let wanted: Vec<_> = split.chain(devtools).chain(handles).collect();
                let frame = inner.win.frame.as_ref().map(|f| f.window.clone());
                (wanted, frame, inner.win.peek.is_some())
            };
            // The grip beside an open web panel, to resize it, and the
            // button to close it.
            let mut wanted = wanted;
            let panel = self.web_panel_bounds();
            let side = self.sidebar_side();
            wanted.extend((panel.as_ref()).map(|p| {
                (
                    crate::content::web_panel_grip(p, side),
                    crate::mac::DividerKey::WebPanel,
                )
            }));
            if let Some(frame) = frame {
                crate::mac::set_dividers(frame.window_handle(), &wanted);
                // Not over a peek and the dimmed pages behind it.
                let button = panel.as_ref().filter(|_| !peek);
                crate::mac::set_panel_close(frame.window_handle(), button, side);
            }
        }
        #[cfg(target_os = "windows")]
        {
            let (wanted, frame) = {
                let inner = self.lock();
                let row = inner.devtools_dock != crate::DevToolsDock::Bottom;
                let split = inner.win.split_dividers.iter().map(|d| {
                    let key = crate::win::DividerKey::Split {
                        row: d.axis == Axis::Row,
                        path: d.path.clone(),
                        index: d.index,
                    };
                    (rect_of(d.bounds), key)
                });
                let devtools = (inner.docked.iter())
                    .filter(|(_, d)| d.shown && !d.separate)
                    .filter_map(|(id, d)| {
                        Some((
                            d.gap.clone()?,
                            crate::win::DividerKey::DevTools { id: *id, row },
                        ))
                    });
                // The resize handles of pages in device mode ("Responsive").
                let handles = (inner.docked.iter())
                    .filter(|(_, d)| d.shown)
                    .filter_map(|(id, d)| Some((*id, d.device.as_ref()?)))
                    .flat_map(|(id, m)| {
                        let key = move |edge| crate::win::DividerKey::Device { id, edge };
                        m.handles
                            .iter()
                            .map(move |(r, edge)| (r.clone(), key(*edge)))
                    });
                // Windows places its windows in pixels.
                let wanted: Vec<_> = split
                    .chain(devtools)
                    .chain(handles)
                    .map(|(r, key)| (display_convert_screen_rect_to_pixels(Some(&r)), key))
                    .collect();
                (wanted, inner.win.frame.as_ref().map(|f| f.window.clone()))
            };
            if let Some(frame) = frame {
                crate::win::set_dividers(frame.window_handle(), &wanted);
            }
        }
        self.web_panel_above_dividers();
        self.sync_panel_close();
        // Every change of what is on screen, and where, comes by here.
        self.update_page_shadows();
    }

    /// JPEG screenshot of page `id` as a data URL (drop-zone overlay).
    pub fn snapshot_page(&self, id: ContentId, done: Box<dyn FnOnce(Option<String>)>) {
        let params = serde_json::json!({ "format": "jpeg", "quality": 70 });
        self.devtools(
            id,
            "Page.captureScreenshot",
            &params,
            Box::new(move |shot| {
                done(shot.ok().and_then(|s| {
                    s["data"]
                        .as_str()
                        .map(|d| format!("data:image/jpeg;base64,{d}"))
                }));
            }),
        );
    }

    /// Screen bounds of the visible pages: `(window id, bounds relative to the
    /// page area)`, for the drop-zone overlay.
    pub fn visible_page_bounds(&self) -> Vec<(ContentId, Rect)> {
        let area = self.content_rect();
        let ids: Vec<ContentId> = {
            let inner = self.lock();
            match inner.win.split.as_ref() {
                Some(t) if t.pane_count() > 1 => t.ids(),
                _ => inner
                    .contents
                    .iter()
                    .filter(|(_, c)| c.mini.is_none() && c.home == inner.current)
                    .filter(|(_, c)| c.window.as_ref().is_some_and(|w| w.is_visible() != 0))
                    .map(|(id, _)| *id)
                    .collect(),
            }
        };
        ids.into_iter()
            .filter_map(|id| {
                let b = self.content_bounds(id)?;
                Some((
                    id,
                    Rect {
                        x: b.x - area.x,
                        y: b.y - area.y,
                        ..b
                    },
                ))
            })
            .collect()
    }

    /// Open `url` as an overlay covering the whole page area without taking
    /// focus (the drop-zone overlay during a drag).
    pub fn open_area_overlay(&self, url: &str) -> bool {
        self.open_overlay_with(self.content_rect(), url, 10.0, false, false)
    }
}
