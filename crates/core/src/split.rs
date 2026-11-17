//! Split view layout: a small tiling tree. Leaves are pages; inner nodes
//! divide their area into a row (side by side) or a column (stacked) with
//! relative sizes. Pages are added beside an existing one on any side, so
//! mixed layouts (two stacked pages next to a tall one) are possible. The
//! tree is generic over the page id: the app keeps tab ids, the engine maps
//! them to its windows.

use serde::{Deserialize, Serialize};

/// Most pages one split shows.
pub const MAX_PANES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// Children side by side (vertical dividers).
    Row,
    /// Children stacked (horizontal dividers).
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    /// The name the UI uses, as serialised (`"left"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Row,
            Self::Top | Self::Bottom => Axis::Column,
        }
    }
    fn after(self) -> bool {
        matches!(self, Self::Right | Self::Bottom)
    }
}

/// A rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SplitNode<T> {
    Pane {
        id: T,
    },
    Split {
        axis: Axis,
        children: Vec<SplitNode<T>>,
        sizes: Vec<f64>,
    },
}

/// The gap between two neighbours of a split, which can be dragged.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Divider {
    /// Child indices from the root to the split this divider belongs to.
    pub path: Vec<usize>,
    /// Sits after child `index`.
    pub index: usize,
    pub axis: Axis,
    pub bounds: Bounds,
}

fn extent(b: Bounds, axis: Axis) -> i32 {
    match axis {
        Axis::Row => b.width,
        Axis::Column => b.height,
    }
}

/// Split `area` along `axis` into parts of relative `sizes`, `gap` apart.
#[allow(clippy::cast_possible_truncation)] // rounded parts of an i32 extent
fn parts(area: Bounds, axis: Axis, sizes: &[f64], gap: i32) -> Vec<Bounds> {
    let n = i32::try_from(sizes.len()).unwrap_or(1).max(1);
    let total = (extent(area, axis) - gap * (n - 1)).max(n);
    let sum: f64 = sizes.iter().sum::<f64>().max(f64::EPSILON);
    let mut out = Vec::with_capacity(sizes.len());
    let mut offset = 0;
    for (i, s) in sizes.iter().enumerate() {
        let len = if i + 1 == sizes.len() {
            total - offset
        } else {
            (f64::from(total) * s / sum).round() as i32
        };
        let start = offset + gap * i32::try_from(i).unwrap_or(0);
        out.push(match axis {
            Axis::Row => Bounds {
                x: area.x + start,
                width: len,
                ..area
            },
            Axis::Column => Bounds {
                y: area.y + start,
                height: len,
                ..area
            },
        });
        offset += len;
    }
    out
}

impl<T: Clone + PartialEq> SplitNode<T> {
    pub fn pane(id: T) -> Self {
        Self::Pane { id }
    }

    /// The same pages as an even grid, in reading order: rows of two, stacked
    /// (four pages make a 2×2, three leave the last one a full-width row).
    #[must_use]
    pub fn grid(&self) -> Self {
        fn even<T>(axis: Axis, mut children: Vec<SplitNode<T>>) -> SplitNode<T> {
            if children.len() == 1 {
                return children.remove(0);
            }
            #[allow(clippy::cast_precision_loss)]
            let sizes = vec![1.0 / children.len() as f64; children.len()];
            SplitNode::Split {
                axis,
                children,
                sizes,
            }
        }
        let rows = self
            .ids()
            .chunks(2)
            .map(|row| even(Axis::Row, row.iter().cloned().map(Self::pane).collect()))
            .collect();
        even(Axis::Column, rows)
    }

    /// Page ids in reading order (left to right, top to bottom).
    pub fn ids(&self) -> Vec<T> {
        match self {
            Self::Pane { id } => vec![id.clone()],
            Self::Split { children, .. } => children.iter().flat_map(Self::ids).collect(),
        }
    }

    /// How many pages the tree shows (at least one).
    pub fn pane_count(&self) -> usize {
        match self {
            Self::Pane { .. } => 1,
            Self::Split { children, .. } => children.iter().map(Self::pane_count).sum(),
        }
    }

    pub fn contains(&self, id: &T) -> bool {
        match self {
            Self::Pane { id: p } => p == id,
            Self::Split { children, .. } => children.iter().any(|c| c.contains(id)),
        }
    }

    /// The same tree with other page ids (tab ids → window ids).
    pub fn map<U>(&self, f: &impl Fn(&T) -> U) -> SplitNode<U> {
        match self {
            Self::Pane { id } => SplitNode::Pane { id: f(id) },
            Self::Split {
                axis,
                children,
                sizes,
            } => SplitNode::Split {
                axis: *axis,
                children: children.iter().map(|c| c.map(f)).collect(),
                sizes: sizes.clone(),
            },
        }
    }

    /// Put `new` beside `target` on `side`, taking half of `target`'s space.
    /// Refused if `target` is missing, `new` is already shown, or the split is full.
    pub fn insert(&mut self, target: &T, new: T, side: Side) -> bool {
        if !self.contains(target) || self.contains(&new) || self.pane_count() >= MAX_PANES {
            return false;
        }
        self.insert_at(target, new, side);
        true
    }

    fn insert_at(&mut self, target: &T, new: T, side: Side) {
        match self {
            Self::Pane { id } if id == target => {
                let old = Self::pane(id.clone());
                let pair = if side.after() {
                    vec![old, Self::pane(new)]
                } else {
                    vec![Self::pane(new), old]
                };
                *self = Self::Split {
                    axis: side.axis(),
                    children: pair,
                    sizes: vec![0.5, 0.5],
                };
            }
            Self::Pane { .. } => {}
            Self::Split {
                axis,
                children,
                sizes,
            } => {
                let Some(i) = children.iter().position(|c| c.contains(target)) else {
                    return;
                };
                let is_leaf = matches!(&children[i], Self::Pane { id } if id == target);
                if is_leaf && *axis == side.axis() {
                    // Same direction: become a sibling, sharing the target's size.
                    let half = sizes[i] / 2.0;
                    sizes[i] = half;
                    let at = if side.after() { i + 1 } else { i };
                    children.insert(at, Self::pane(new));
                    sizes.insert(at, half);
                } else {
                    children[i].insert_at(target, new, side);
                }
            }
        }
    }

    /// Move page `id`, already shown, beside `target` on `side`. The space it
    /// leaves goes to its old neighbours.
    pub fn relocate(&mut self, id: &T, target: &T, side: Side) -> bool {
        if id == target || !self.contains(id) || !self.contains(target) {
            return false;
        }
        let Some(mut rest) = self.clone().remove(id) else {
            return false;
        };
        rest.insert_at(target, id.clone(), side);
        *self = rest;
        true
    }

    /// Move page `id` before or after `target` (`side`) in reading order. The
    /// layout and sizes stay; the pages between the two shift by one place.
    pub fn reorder(&mut self, id: &T, target: &T, side: Side) -> bool {
        if id == target || !self.contains(id) {
            return false;
        }
        let mut ids = self.ids();
        ids.retain(|t| t != id);
        let Some(at) = ids.iter().position(|t| t == target) else {
            return false;
        };
        ids.insert(at + usize::from(side.after()), id.clone());
        let next = std::cell::RefCell::new(ids.into_iter());
        *self = self.map(&|t| next.borrow_mut().next().unwrap_or_else(|| t.clone()));
        true
    }

    /// Let pages `a` and `b` trade places; the layout and sizes stay.
    pub fn swap(&mut self, a: &T, b: &T) -> bool {
        if a == b || !self.contains(a) || !self.contains(b) {
            return false;
        }
        *self = self.map(&|t| {
            if t == a {
                b.clone()
            } else if t == b {
                a.clone()
            } else {
                t.clone()
            }
        });
        true
    }

    /// The tree without page `id` (`None` if nothing is left). Splits left
    /// with one child are replaced by it; the others share the freed space.
    pub fn remove(self, id: &T) -> Option<Self> {
        match self {
            Self::Pane { id: p } => (p != *id).then_some(Self::Pane { id: p }),
            Self::Split {
                axis,
                children,
                sizes,
            } => {
                let mut kept: Vec<(Self, f64)> = children
                    .into_iter()
                    .zip(sizes)
                    .filter_map(|(c, s)| c.remove(id).map(|c| (c, s)))
                    .collect();
                match kept.len() {
                    0 => None,
                    1 => kept.pop().map(|(c, _)| c),
                    _ => {
                        let sum: f64 = kept.iter().map(|(_, s)| s).sum::<f64>().max(f64::EPSILON);
                        let (children, sizes) = kept.into_iter().map(|(c, s)| (c, s / sum)).unzip();
                        Some(Self::Split {
                            axis,
                            children,
                            sizes,
                        })
                    }
                }
            }
        }
    }

    /// Where each page goes inside `area`, `gap` pixels apart.
    pub fn layout(&self, area: Bounds, gap: i32) -> Vec<(T, Bounds)> {
        match self {
            Self::Pane { id } => vec![(id.clone(), area)],
            Self::Split {
                axis,
                children,
                sizes,
            } => children
                .iter()
                .zip(parts(area, *axis, sizes, gap))
                .flat_map(|(c, b)| c.layout(b, gap))
                .collect(),
        }
    }

    /// The draggable gaps of the layout in `area`.
    pub fn dividers(&self, area: Bounds, gap: i32) -> Vec<Divider> {
        let mut out = Vec::new();
        self.collect_dividers(area, gap, &mut Vec::new(), &mut out);
        out
    }

    fn collect_dividers(
        &self,
        area: Bounds,
        gap: i32,
        path: &mut Vec<usize>,
        out: &mut Vec<Divider>,
    ) {
        let Self::Split {
            axis,
            children,
            sizes,
        } = self
        else {
            return;
        };
        let rects = parts(area, *axis, sizes, gap);
        for (i, pair) in rects.windows(2).enumerate() {
            let bounds = match axis {
                Axis::Row => Bounds {
                    x: pair[0].x + pair[0].width,
                    width: gap,
                    ..area
                },
                Axis::Column => Bounds {
                    y: pair[0].y + pair[0].height,
                    height: gap,
                    ..area
                },
            };
            out.push(Divider {
                path: path.clone(),
                index: i,
                axis: *axis,
                bounds,
            });
        }
        for (i, (c, r)) in children.iter().zip(rects).enumerate() {
            path.push(i);
            c.collect_dividers(r, gap, path, out);
            path.pop();
        }
    }

    /// The split at `path` with its area (for dragging its dividers).
    fn split_at(&mut self, path: &[usize], area: Bounds, gap: i32) -> Option<(&mut Self, Bounds)> {
        let Some((&first, rest)) = path.split_first() else {
            return Some((self, area));
        };
        let Self::Split {
            axis,
            children,
            sizes,
        } = self
        else {
            return None;
        };
        let tile = *parts(area, *axis, sizes, gap).get(first)?;
        children.get_mut(first)?.split_at(rest, tile, gap)
    }

    /// Move the divider after child `index` of the split at `path` to
    /// position `pos` (pixels along the split's axis, same space as `area`),
    /// keeping both neighbours at least `min` pixels.
    #[allow(clippy::cast_precision_loss)]
    pub fn drag_divider(
        &mut self,
        path: &[usize],
        index: usize,
        area: Bounds,
        gap: i32,
        pos: i32,
        min: i32,
    ) -> bool {
        let Some((node, rect)) = self.split_at(path, area, gap) else {
            return false;
        };
        let Self::Split { axis, sizes, .. } = node else {
            return false;
        };
        if index + 1 >= sizes.len() {
            return false;
        }
        let rects = parts(rect, *axis, sizes, gap);
        let (a, b) = (rects[index], rects[index + 1]);
        let start = match axis {
            Axis::Row => a.x,
            Axis::Column => a.y,
        };
        let both = extent(a, *axis) + extent(b, *axis);
        let first = (pos - gap / 2 - start).clamp(min, (both - min).max(min));
        let combined = sizes[index] + sizes[index + 1];
        sizes[index] = combined * f64::from(first) / f64::from(both.max(1));
        sizes[index + 1] = combined - sizes[index];
        true
    }

    /// The page at `path` (child indices from the root), if a page is there.
    pub fn pane_at(&self, path: &[usize]) -> Option<&T> {
        match (self, path.split_first()) {
            (Self::Pane { id }, None) => Some(id),
            (Self::Split { children, .. }, Some((first, rest))) => {
                children.get(*first)?.pane_at(rest)
            }
            _ => None,
        }
    }

    /// The tree with `new` right of page `id`, in that page's place: `new`
    /// gets `px` pixels of its width, and both keep at least `min` (half
    /// the place each where it is too narrow for that). The tree as it is
    /// if `id` is not in it.
    #[must_use]
    pub fn with_beside(&self, id: &T, new: T, px: i32, area: Bounds, gap: i32, min: i32) -> Self {
        let Some((_, place)) = self.layout(area, gap).into_iter().find(|(p, _)| p == id) else {
            return self.clone();
        };
        let both = (place.width - gap).max(1);
        // Too narrow for both to keep `min`: they share the place evenly.
        let px = if both < 2 * min {
            both / 2
        } else {
            px.clamp(min, both - min)
        };
        let share = f64::from(px) / f64::from(both);
        let pair = Self::Split {
            axis: Axis::Row,
            children: vec![Self::pane(id.clone()), Self::pane(new)],
            sizes: vec![1.0 - share, share],
        };
        self.replaced(id, &pair)
    }

    /// The tree with `node` in the place of page `id`.
    fn replaced(&self, id: &T, node: &Self) -> Self {
        match self {
            Self::Pane { id: p } if p == id => node.clone(),
            Self::Pane { .. } => self.clone(),
            Self::Split {
                axis,
                children,
                sizes,
            } => Self::Split {
                axis: *axis,
                children: children.iter().map(|c| c.replaced(id, node)).collect(),
                sizes: sizes.clone(),
            },
        }
    }

    /// Give all children of the split at `path` the same size.
    pub fn equalize(&mut self, path: &[usize], area: Bounds, gap: i32) -> bool {
        let Some((Self::Split { sizes, .. }, _)) = self.split_at(path, area, gap) else {
            return false;
        };
        #[allow(clippy::cast_precision_loss)]
        let each = 1.0 / sizes.len() as f64;
        for s in sizes.iter_mut() {
            *s = each;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Bounds = Bounds {
        x: 0,
        y: 0,
        width: 1008,
        height: 808,
    };

    fn tree(ids: &[(&'static str, Side)]) -> SplitNode<&'static str> {
        let mut t = SplitNode::pane("a");
        let mut last = "a";
        for (id, side) in ids {
            assert!(t.insert(&last, id, *side));
            last = id;
        }
        t
    }

    #[test]
    fn insert_builds_mixed_layouts_up_to_four() {
        // a | b, then c below b: a tall left page and two stacked on the right.
        let mut t = tree(&[("b", Side::Right), ("c", Side::Bottom)]);
        assert_eq!(t.ids(), vec!["a", "b", "c"]);
        let rects = t.layout(AREA, 8);
        assert_eq!(
            rects[0].1,
            Bounds {
                x: 0,
                y: 0,
                width: 500,
                height: 808
            }
        );
        assert_eq!(
            rects[1].1,
            Bounds {
                x: 508,
                y: 0,
                width: 500,
                height: 400
            }
        );
        assert_eq!(
            rects[2].1,
            Bounds {
                x: 508,
                y: 408,
                width: 500,
                height: 400
            }
        );
        // Same direction becomes a sibling: d left of a halves a's space.
        assert!(t.insert(&"a", "d", Side::Left));
        assert_eq!(t.ids(), vec!["d", "a", "b", "c"]);
        assert!(!t.insert(&"a", "e", Side::Top), "four pages at most");
        assert!(!t.insert(&"a", "b", Side::Top), "already shown");
    }

    #[test]
    fn a_page_gets_another_beside_it_in_its_own_place() {
        // One page alone: the other takes 390 pixels of the area.
        let t = SplitNode::pane("a").with_beside(&"a", "phone", 390, AREA, 8, 200);
        let rects = t.layout(AREA, 8);
        assert_eq!((rects[0].1.width, rects[1].1.width), (610, 390));
        // In a split, inside its page's place only; the split's own
        // dividers keep their paths, the new one is where the page was.
        let split = tree(&[("b", Side::Right), ("c", Side::Bottom)]);
        let t = split.with_beside(&"c", "phone", 300, AREA, 8, 200);
        assert_eq!(t.ids(), vec!["a", "b", "c", "phone"]);
        let rects = t.layout(AREA, 8);
        assert_eq!((rects[2].1.width, rects[3].1.width), (200, 292));
        assert_eq!(rects[0].1.width, 500);
        assert_eq!(split.pane_at(&[1, 1]), Some(&"c"));
        assert_eq!(split.pane_at(&[1]), None, "a split, not a page");
        assert_eq!(SplitNode::pane("a").pane_at(&[]), Some(&"a"));
        // A place too narrow for both: shared evenly, neither is gone.
        let narrow = Bounds { width: 308, ..AREA };
        let t = SplitNode::pane("a").with_beside(&"a", "phone", 390, narrow, 8, 200);
        let rects = t.layout(narrow, 8);
        assert_eq!((rects[0].1.width, rects[1].1.width), (150, 150));
        // A page that is not there: nothing changes.
        assert_eq!(split.with_beside(&"x", "phone", 300, AREA, 8, 200), split);
    }

    #[test]
    fn remove_collapses_and_redistributes() {
        let t = tree(&[("b", Side::Right), ("c", Side::Bottom)]);
        let t = t.remove(&"c").unwrap();
        assert_eq!(t.ids(), vec!["a", "b"]);
        assert_eq!(
            t.layout(AREA, 8)[1].1.height,
            808,
            "b takes the column back"
        );
        let t = t.remove(&"b").unwrap();
        assert_eq!(t, SplitNode::pane("a"));
        assert!(t.remove(&"a").is_none());
    }

    #[test]
    fn dividers_and_dragging_with_limits() {
        let mut t = tree(&[("b", Side::Right), ("c", Side::Bottom)]);
        let d = t.dividers(AREA, 8);
        assert_eq!(d.len(), 2);
        assert_eq!(
            d[0].bounds,
            Bounds {
                x: 500,
                y: 0,
                width: 8,
                height: 808
            }
        );
        assert_eq!((d[1].path.clone(), d[1].axis), (vec![1], Axis::Column));
        // Drag the vertical divider to x = 304: a gets 300 px.
        assert!(t.drag_divider(&[], 0, AREA, 8, 304, 200));
        assert_eq!(t.layout(AREA, 8)[0].1.width, 300);
        // Too far: clamped so b keeps 200 px.
        assert!(t.drag_divider(&[], 0, AREA, 8, 2000, 200));
        assert_eq!(t.layout(AREA, 8)[1].1.width, 200);
        assert!(t.equalize(&[], AREA, 8));
        assert_eq!(t.layout(AREA, 8)[0].1.width, 500);
    }

    #[test]
    fn grid_arranges_pages_in_even_rows_of_two() {
        // Four in a row with uneven sizes become four quarters.
        let mut t = tree(&[("b", Side::Right), ("c", Side::Right), ("d", Side::Right)]);
        assert!(t.drag_divider(&[], 0, AREA, 8, 304, 200));
        let g = t.grid();
        assert_eq!(g.ids(), vec!["a", "b", "c", "d"]);
        let rects: Vec<_> = g.layout(AREA, 8).into_iter().map(|(_, b)| b).collect();
        assert!(rects.iter().all(|b| (b.width, b.height) == (500, 400)));
        assert_eq!((rects[1].x, rects[1].y), (508, 0));
        assert_eq!((rects[2].x, rects[2].y), (0, 408));
        // Three: the last page takes the whole lower row. Two: side by side.
        let three = tree(&[("b", Side::Bottom), ("c", Side::Bottom)]).grid();
        assert_eq!(three.layout(AREA, 8)[2].1.width, 1008);
        let two = tree(&[("b", Side::Bottom)]).grid();
        assert_eq!(two, tree(&[("b", Side::Right)]));
    }

    #[test]
    fn pages_move_and_swap_inside_a_split() {
        let mut t = tree(&[("b", Side::Right), ("c", Side::Right), ("d", Side::Right)]).grid();
        // Swapping keeps the grid: d and a trade corners.
        assert!(t.swap(&"d", &"a"));
        assert_eq!(t.ids(), vec!["d", "b", "c", "a"]);
        assert_eq!(t.layout(AREA, 8)[3].1.width, 500);
        // Moving a above d stacks the two in d's corner; c is left alone below.
        assert!(t.relocate(&"a", &"d", Side::Top));
        assert_eq!(t.ids(), vec!["a", "d", "b", "c"]);
        assert_eq!(
            t.layout(AREA, 8)[3].1.width,
            1008,
            "c has the lower row alone"
        );
        assert!(!t.relocate(&"a", &"a", Side::Left));
        // Reordering keeps the shape: c goes first, the others shift.
        let mut g = tree(&[("b", Side::Right), ("c", Side::Right), ("d", Side::Right)]).grid();
        assert!(g.reorder(&"c", &"a", Side::Left));
        assert_eq!(g.ids(), vec!["c", "a", "b", "d"]);
        assert!(g.reorder(&"c", &"d", Side::Right));
        assert_eq!(g.ids(), vec!["a", "b", "d", "c"]);
        assert!(g.layout(AREA, 8).iter().all(|(_, b)| b.width == 500));
        assert!(!t.swap(&"a", &"x"));
        // Two pages side by side turned into two stacked ones.
        let mut two = tree(&[("b", Side::Right)]);
        assert!(two.relocate(&"a", &"b", Side::Bottom));
        assert_eq!(two, tree(&[("b", Side::Top)]));
    }

    #[test]
    fn serialises_as_tagged_tree() {
        let t = tree(&[("b", Side::Right)]);
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains(r#""kind":"split""#) && json.contains(r#""axis":"row""#));
        let back: SplitNode<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ids(), vec!["a".to_string(), "b".to_string()]);
        for side in [Side::Left, Side::Right, Side::Top, Side::Bottom] {
            let json = serde_json::to_string(&side).unwrap();
            assert_eq!(json, format!("\"{}\"", side.as_str()));
        }
    }
}
