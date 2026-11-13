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
    /// Make the dividers match `tree` (none when there is no split).
    pub(crate) fn update_dividers(&self, tree: Option<&SplitNode<ContentId>>) {
        let dividers = tree
            .filter(|t| t.pane_count() > 1)
            .map(|t| t.dividers(self.split_area(), SPLIT_GAP))
            .unwrap_or_default();
        #[cfg(target_os = "macos")]
        {
            let frame = self.frame_window();
            let wanted: Vec<_> = dividers
                .into_iter()
                .map(|d| {
                    let key = crate::mac::DividerKey {
                        row: d.axis == Axis::Row,
                        path: d.path,
                        index: d.index,
                    };
                    (rect_of(d.bounds), key)
                })
                .collect();
            if let Some(frame) = frame {
                crate::mac::set_dividers(frame.window_handle(), &wanted);
            }
        }
        #[cfg(target_os = "windows")]
        {
            let frame = self.frame_window();
            let wanted: Vec<_> = dividers
                .into_iter()
                .map(|d| {
                    let key = crate::win::DividerKey {
                        row: d.axis == Axis::Row,
                        path: d.path,
                        index: d.index,
                    };
                    // Windows places its windows in pixels.
                    (
                        display_convert_screen_rect_to_pixels(Some(&rect_of(d.bounds))),
                        key,
                    )
                })
                .collect();
            if let Some(frame) = frame {
                crate::win::set_dividers(frame.window_handle(), &wanted);
            }
        }
        // Linux: no dividers yet (sizes still even out on request).
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = dividers;
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
            match inner.split.as_ref() {
                Some(t) if t.pane_count() > 1 => t.ids(),
                _ => inner
                    .contents
                    .iter()
                    .filter(|(_, c)| c.mini.is_none())
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
