//! Page capture through the `DevTools` protocol.
//!
//! - `capture_frame` freezes what the page shows: a screenshot plus the
//!   layout boxes of its elements, taken back to back. If the page scrolled
//!   or resized in between, element picking is switched off for that frame
//!   (`element_ok`), so a box can never point at the wrong pixels.
//! - `capture_full_page` captures the whole document in tiles, up to a pixel
//!   budget; the caller stitches them.

use std::cell::RefCell;
use std::rc::Rc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

use crate::engine::Engine;
use crate::events::ContentId;

/// What the page showed at one moment, with element boxes in CSS pixels
/// relative to the visible area (`[x, y, width, height]`, smallest last).
pub struct FrozenFrame {
    pub png: Vec<u8>,
    pub css_width: f64,
    pub css_height: f64,
    pub boxes: Vec<[f64; 4]>,
    pub element_ok: bool,
}

pub struct PageTiles {
    /// PNG tiles from top to bottom, all the same width.
    pub tiles: Vec<Vec<u8>>,
    /// Set when the page is taller than the pixel budget: CSS px captured.
    pub cut_at: Option<f64>,
}

/// CSS pixels per tile (well below Chromium's texture limits).
const TILE_CSS: f64 = 4096.0;

fn num(v: &Value, path: &[&str]) -> f64 {
    path.iter().fold(v, |v, k| &v[*k]).as_f64().unwrap_or(0.0)
}

fn decode(v: &Value) -> Result<Vec<u8>, String> {
    B64.decode(v["data"].as_str().unwrap_or_default())
        .map_err(|e| e.to_string())
}

/// Element boxes of the main document, visible part only, in viewport
/// coordinates; the root and near-full-viewport boxes are left out.
#[allow(clippy::many_single_char_names)] // x, y, w, h of a box
fn element_boxes(snapshot: &Value, scroll: (f64, f64), view: (f64, f64)) -> Vec<[f64; 4]> {
    let doc = &snapshot["documents"][0];
    let node_types = doc["nodes"]["nodeType"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let indices = doc["layout"]["nodeIndex"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let bounds = doc["layout"]["bounds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut boxes: Vec<[f64; 4]> = indices
        .iter()
        .zip(bounds.iter())
        .filter(|(i, _)| {
            let i = usize::try_from(i.as_u64().unwrap_or(0)).unwrap_or(usize::MAX);
            node_types.get(i).and_then(Value::as_u64) == Some(1)
        })
        .filter_map(|(_, b)| {
            let b = b.as_array()?;
            let [x, y, w, h] =
                [0, 1, 2, 3].map(|k| b.get(k).and_then(Value::as_f64).unwrap_or(0.0));
            let (x, y) = (x - scroll.0, y - scroll.1);
            let visible = x < view.0 && y < view.1 && x + w > 0.0 && y + h > 0.0;
            (visible && w >= 4.0 && h >= 4.0 && w * h < view.0 * view.1 * 0.95)
                .then_some([x, y, w, h])
        })
        .collect();
    // Largest first, so the overlay picks the last (smallest) box under the pointer.
    boxes.sort_by(|a, b| (b[2] * b[3]).total_cmp(&(a[2] * a[3])));
    boxes.truncate(8000);
    boxes
}

impl Engine {
    /// Put a PNG on the clipboard (`MAGNETITE_CLIPBOARD_FILE` redirects it in tests).
    pub fn copy_png(&self, png: &[u8]) -> bool {
        if let Some(path) = std::env::var_os("MAGNETITE_CLIPBOARD_FILE") {
            return std::fs::write(path, png).is_ok();
        }
        #[cfg(target_os = "macos")]
        return crate::mac::copy_png(png);
        #[cfg(target_os = "windows")]
        return crate::win::copy_png(png);
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        false
    }

    /// System share menu for a PNG at a point of the frame window.
    pub fn share_png(&self, png: &[u8], x: f64, y: f64) {
        let window = self.frame_window();
        #[cfg(target_os = "macos")]
        if let Some(w) = window {
            use cef::ImplWindow;
            crate::mac::share_png(w.window_handle(), png, x, y);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (window, png, x, y);
    }

    /// Save dialog for a PNG; returns where it was written.
    #[allow(clippy::unused_self)] // an Engine method like its siblings
    pub fn save_png(&self, png: &[u8], name: &str) -> Option<std::path::PathBuf> {
        #[cfg(target_os = "macos")]
        return crate::mac::save_png(png, name);
        #[cfg(target_os = "windows")]
        {
            use cef::ImplWindow;
            let frame = self.frame_window()?;
            crate::win::save_png(frame.window_handle(), png, name)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = (png, name);
            None
        }
    }

    /// Freeze page `id`'s visible frame. `done` runs on the UI thread.
    pub fn capture_frame(&self, id: ContentId, done: Box<dyn FnOnce(Result<FrozenFrame, String>)>) {
        let done = Rc::new(RefCell::new(Some(done)));
        let fail = {
            let done = done.clone();
            move |e: String| {
                if let Some(d) = done.borrow_mut().take() {
                    d(Err(e));
                }
            }
        };
        let fail2 = fail.clone();
        self.devtools(
            id,
            "Page.getLayoutMetrics",
            &json!({}),
            Box::new(move |metrics| {
                let metrics = match metrics {
                    Ok(m) => m,
                    Err(e) => return fail(e),
                };
                let before = (
                    num(&metrics, &["cssLayoutViewport", "pageX"]),
                    num(&metrics, &["cssLayoutViewport", "pageY"]),
                );
                let view = (
                    num(&metrics, &["cssLayoutViewport", "clientWidth"]),
                    num(&metrics, &["cssLayoutViewport", "clientHeight"]),
                );
                Engine::global().devtools(
                    id,
                    "Page.captureScreenshot",
                    &json!({ "format": "png" }),
                    Box::new(move |shot| {
                        let png = match shot.and_then(|s| decode(&s)) {
                            Ok(p) => p,
                            Err(e) => return fail2(e),
                        };
                        let params = json!({ "computedStyles": [] });
                        Engine::global().devtools(
                            id,
                            "DOMSnapshot.captureSnapshot",
                            &params,
                            Box::new(move |snap| {
                                let (boxes, element_ok) = match snap {
                                    Ok(s) => {
                                        let doc = &s["documents"][0];
                                        let scroll = (
                                            num(doc, &["scrollOffsetX"]),
                                            num(doc, &["scrollOffsetY"]),
                                        );
                                        let coherent = (scroll.0 - before.0).abs() < 1.0
                                            && (scroll.1 - before.1).abs() < 1.0;
                                        (element_boxes(&s, scroll, view), coherent)
                                    }
                                    // Without a layout snapshot only region and visible capture work.
                                    Err(_) => (Vec::new(), false),
                                };
                                if let Some(d) = done.borrow_mut().take() {
                                    d(Ok(FrozenFrame {
                                        png,
                                        css_width: view.0,
                                        css_height: view.1,
                                        boxes,
                                        element_ok,
                                    }));
                                }
                            }),
                        );
                    }),
                );
            }),
        );
    }

    /// Capture the whole document of page `id` in tiles, up to `max_pixels`
    /// device pixels. `done` runs on the UI thread.
    pub fn capture_full_page(
        &self,
        id: ContentId,
        max_pixels: f64,
        done: Box<dyn FnOnce(Result<PageTiles, String>)>,
    ) {
        self.devtools(
            id,
            "Page.getLayoutMetrics",
            &json!({}),
            Box::new(move |metrics| {
                let metrics = match metrics {
                    Ok(m) => m,
                    Err(e) => return done(Err(e)),
                };
                let width = num(&metrics, &["cssLayoutViewport", "clientWidth"]).max(1.0);
                let height = num(&metrics, &["cssContentSize", "height"]).max(1.0);
                let expr = json!({ "expression": "devicePixelRatio", "returnByValue": true });
                Engine::global().devtools(
                    id,
                    "Runtime.evaluate",
                    &expr,
                    Box::new(move |dpr| {
                        let dpr = dpr
                            .ok()
                            .and_then(|v| v["result"]["value"].as_f64())
                            .unwrap_or(2.0)
                            .max(1.0);
                        let budget_height = (max_pixels / (width * dpr * dpr)).floor();
                        let target = height.min(budget_height);
                        let cut_at = (height > budget_height).then_some(target);
                        let tops: Vec<f64> =
                            std::iter::successors(Some(0.0), |y| Some(y + TILE_CSS))
                                .take_while(|y| *y < target)
                                .collect();
                        let job = Tiling {
                            id,
                            tops,
                            width,
                            target,
                            cut_at,
                            max_pixels,
                        };
                        next_tile(Rc::new(job), 0, Vec::new(), 0.0, done);
                    }),
                );
            }),
        );
    }
}

/// A full-page capture in progress: what stays the same from tile to tile.
struct Tiling {
    id: ContentId,
    /// Top edge of each tile, in CSS pixels.
    tops: Vec<f64>,
    width: f64,
    /// Where the capture ends, in CSS pixels.
    target: f64,
    /// Set when the plan already leaves out the page's end.
    cut_at: Option<f64>,
    /// Most device pixels the tiles may hold together.
    max_pixels: f64,
}

/// Width times height of the PNG `png`, from its header. Not a PNG: more
/// than any budget.
fn png_pixels(png: &[u8]) -> f64 {
    let field = |at: usize| {
        let bytes: Option<[u8; 4]> = png.get(at..at + 4).and_then(|b| b.try_into().ok());
        bytes.map(u32::from_be_bytes)
    };
    match (png.starts_with(b"\x89PNG\r\n\x1a\n"), field(16), field(20)) {
        (true, Some(width), Some(height)) => f64::from(width) * f64::from(height),
        _ => f64::INFINITY,
    }
}

/// Capture tile `i` and go on with the next. `used`: device pixels of the
/// tiles so far.
///
/// The budget is counted on the pictures themselves. The plan above rests on
/// the pixel ratio the page reports, which the page can set to anything: a
/// page that claims 1 on a screen with 2 would otherwise get four times the
/// budget, and the stitched picture would need gigabytes.
fn next_tile(
    job: Rc<Tiling>,
    i: usize,
    mut tiles: Vec<Vec<u8>>,
    used: f64,
    done: Box<dyn FnOnce(Result<PageTiles, String>)>,
) {
    let Some(&top) = job.tops.get(i) else {
        return done(Ok(PageTiles {
            tiles,
            cut_at: job.cut_at,
        }));
    };
    let h = (job.target - top).min(TILE_CSS);
    let params = json!({
        "format": "png",
        "captureBeyondViewport": true,
        "clip": { "x": 0, "y": top, "width": job.width, "height": h, "scale": 1 },
    });
    Engine::global().devtools(
        job.id,
        "Page.captureScreenshot",
        &params,
        Box::new(move |shot| match shot.and_then(|s| decode(&s)) {
            Ok(png) => {
                let used = used + png_pixels(&png);
                if used > job.max_pixels {
                    // The page ends here for the picture; a first tile that
                    // is too large alone leaves nothing to show.
                    return done(if tiles.is_empty() {
                        Err("the page is too large to capture".into())
                    } else {
                        Ok(PageTiles {
                            tiles,
                            cut_at: Some(top),
                        })
                    });
                }
                tiles.push(png);
                next_tile(job, i + 1, tiles, used, done);
            }
            Err(e) => done(Err(e)),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_is_measured_by_its_own_header() {
        // Signature, the IHDR chunk's length and name, then width and height.
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(3000u32.to_be_bytes());
        png.extend(8000u32.to_be_bytes());
        assert!((png_pixels(&png) - 24_000_000.0).abs() < 1.0);
        // Cut short, or no PNG at all: counts as more than any budget.
        assert!(png_pixels(&png[..20]).is_infinite());
        assert!(png_pixels(b"GIF89a, and then twenty more bytes").is_infinite());
    }

    #[test]
    fn boxes_are_visible_elements_in_viewport_coordinates() {
        let snap = json!({ "documents": [{
            "nodes": { "nodeType": [9, 1, 3, 1, 1] },
            "layout": {
                "nodeIndex": [1, 2, 3, 4],
                "bounds": [[0, 0, 800, 3000], [10, 10, 50, 10], [10, 120, 100, 40], [10, 5000, 100, 40]]
            }
        }]});
        let boxes = element_boxes(&snap, (0.0, 100.0), (800.0, 600.0));
        // The root is too big, the text node is skipped, the last box is off screen.
        assert_eq!(boxes, vec![[10.0, 20.0, 100.0, 40.0]]);
    }
}
