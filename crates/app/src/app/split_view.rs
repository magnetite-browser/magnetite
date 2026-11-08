//! Split view: several pages on screen, their sizes, and dragging a tab
//! into a split.

use std::collections::HashMap;

use magnetite_core::split::{Side, SplitNode};
use magnetite_engine::{ContentId, Engine};
use serde_json::json;

use crate::lock;

use super::{MagnetiteApp, global};

impl MagnetiteApp {
    /// Move divider `index` of the split at `path` to screen position `pos`
    /// (both neighbours keep at least 200 px).
    pub(super) fn split_resize(&self, engine: &Engine, path: &[usize], index: usize, pos: i32) {
        let area = engine.split_area();
        let moved = self.with_state(|s| {
            s.visible_split_mut().is_some_and(|t| {
                t.drag_divider(path, index, area, magnetite_engine::SPLIT_GAP, pos, 200)
            })
        });
        if moved {
            self.retile_split(engine);
        }
    }

    /// Give every page of the split at `path` the same size.
    pub(super) fn split_equalize(&self, engine: &Engine, path: &[usize]) {
        let area = engine.split_area();
        self.with_state(|s| {
            if let Some(t) = s.visible_split_mut() {
                t.equalize(path, area, magnetite_engine::SPLIT_GAP);
            }
        });
        self.retile_split(engine);
        self.push_state(engine);
    }

    /// Add `tab` beside `target` (default: the active tab) on `side`.
    pub(super) fn split_add(&self, engine: &Engine, target: Option<&str>, tab: &str, side: Side) {
        let (added, full) = self.with_state(|s| {
            let full = s.split_is_full(target, tab);
            (s.split_insert(target, tab, side), full)
        });
        // The split is what the user looks at now: a peek over it ends.
        if added {
            self.close_peek(engine);
        }
        if !added && full {
            Self::ui_call(
                engine,
                "notice",
                &json!({ "text": "Split view holds up to four pages" }),
            );
        }
        self.present(engine);
    }

    /// The visible split with window ids (`panes` and `contents` are
    /// parallel), plus the window to focus; `None` when one page shows.
    pub(super) fn split_on_screen(
        &self,
        panes: &[String],
        contents: &[ContentId],
    ) -> Option<(SplitNode<ContentId>, Option<ContentId>)> {
        let tree = self.with_state(|s| s.visible_split())?;
        let by_tab: HashMap<&str, ContentId> = panes
            .iter()
            .map(String::as_str)
            .zip(contents.iter().copied())
            .collect();
        let focus = self.with_state(|s| s.active_tab_id().and_then(|t| by_tab.get(t).copied()));
        Some((
            tree.map(&|t: &String| by_tab.get(t.as_str()).copied().unwrap_or_default()),
            focus,
        ))
    }

    /// Screenshot the visible pages, then show drop zones over them.
    pub(super) fn split_drag_start(&self, engine: &Engine, tab: String) {
        let pages: Vec<(ContentId, String, [i32; 4])> = self.with_state(|s| {
            engine
                .visible_page_bounds()
                .into_iter()
                .filter_map(|(c, r)| {
                    Some((
                        c,
                        s.content_to_tab.get(&c)?.clone(),
                        [r.x, r.y, r.width, r.height],
                    ))
                })
                .collect()
        });
        let generation = {
            let mut drag = lock(&self.split_drag);
            drag.generation += 1;
            drag.tab = Some(tab);
            drag.panes = pages
                .iter()
                .map(|(_, t, r)| (t.clone(), *r, None))
                .collect();
            drag.generation
        };
        if pages.is_empty() {
            return;
        }
        let pending = std::rc::Rc::new(std::cell::Cell::new(pages.len()));
        for (c, tab, _) in pages {
            let pending = pending.clone();
            engine.snapshot_page(
                c,
                Box::new(move |image| {
                    let Some(app) = global() else { return };
                    let open = {
                        let mut drag = lock(&app.split_drag);
                        if drag.generation != generation || drag.tab.is_none() {
                            return;
                        }
                        if let Some(p) = drag.panes.iter_mut().find(|p| p.0 == tab) {
                            p.2 = image;
                        }
                        pending.set(pending.get() - 1);
                        let open = pending.get() == 0;
                        drag.overlay_open |= open;
                        open
                    };
                    if open {
                        Engine::global().open_area_overlay("magnetite://ui/splitdrop");
                    }
                }),
            );
        }
    }

    /// End a tab drag: close the drop zones; returns the dragged tab.
    pub(super) fn split_drag_end(&self, engine: &Engine) -> Option<String> {
        let (tab, was_open) = {
            let mut drag = lock(&self.split_drag);
            drag.panes.clear();
            (drag.tab.take(), std::mem::take(&mut drag.overlay_open))
        };
        if was_open {
            engine.close_overlay();
        }
        tab
    }

    /// Tile the split again after a divider moved (no raising or focusing).
    pub(super) fn retile_split(&self, engine: &Engine) {
        let windows = self.with_state(|s| {
            let tree = s.visible_split()?;
            let ids: Option<Vec<ContentId>> = tree
                .ids()
                .iter()
                .map(|t| s.runtime.get(t).and_then(|r| r.content))
                .collect();
            Some((tree, ids?))
        });
        if let Some((tree, contents)) = windows {
            let panes = tree.ids();
            let by_tab: HashMap<&str, ContentId> =
                panes.iter().map(String::as_str).zip(contents).collect();
            engine.update_split(
                &tree.map(&|t: &String| by_tab.get(t.as_str()).copied().unwrap_or_default()),
            );
        }
    }
}
