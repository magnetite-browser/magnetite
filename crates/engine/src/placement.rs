//! Where a frame window is, kept so the window opens the same way at the
//! next start: its size and place while not maximized, and whether it is
//! maximized. CEF tells a maximized window's bounds only as the screen's,
//! so the engine keeps the ones from before (`WindowPlacement::after`).

use cef::{ImplDisplay, Rect, display_get_alls, display_get_count, display_get_matching_bounds};
use serde::{Deserialize, Serialize};

/// The height of the window's top edge that has to be on a screen: the
/// strip with the sidebar's or top bar's drag area and the window's buttons.
const TOP_EDGE: i64 = 40;

/// A frame window's bounds while it is not maximized (DIP, screen
/// coordinates) and whether it is maximized over them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(default)]
    pub maximized: bool,
}

impl WindowPlacement {
    /// The placement once the window's bounds are `bounds`, maximized or
    /// not. A maximized window keeps the bounds it had before; one with
    /// none before (maximized before its first report) keeps the screen's.
    pub(crate) fn after(previous: Option<Self>, bounds: &Rect, maximized: bool) -> Self {
        match previous {
            Some(before) if maximized => Self {
                maximized,
                ..before
            },
            _ => Self {
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                height: bounds.height,
                maximized,
            },
        }
    }

    /// The bounds while not maximized.
    pub(crate) fn bounds(&self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }

    /// The placement to open the window at: as it is while its top edge is
    /// on the screens (across two of them or under the Dock too), else
    /// fitted into the work area of the screen it overlaps most, or the
    /// nearest one.
    pub(crate) fn on_screen(self) -> Self {
        let mut displays = (0..display_get_count()).map(|_| None).collect();
        display_get_alls(Some(&mut displays));
        let areas: Vec<Rect> = displays
            .into_iter()
            .flatten()
            .map(|d| d.work_area())
            .collect();
        if self.top_edge_on(&areas) {
            return self;
        }
        display_get_matching_bounds(Some(&self.bounds()), 0)
            .map(|d| d.work_area())
            .filter(|area| area.width > 0 && area.height > 0)
            .map_or(self, |area| self.fitted(&area))
    }

    /// The bounds to go back to of a window now maximized at `maximized`:
    /// moved along to that screen where a key moved the maximized window
    /// to another one.
    pub(crate) fn onto_screen_of(self, maximized: &Rect) -> Self {
        let display = |bounds: &Rect| display_get_matching_bounds(Some(bounds), 0);
        match (display(&self.bounds()), display(maximized)) {
            (Some(from), Some(to)) if from.id() != to.id() => {
                self.moved(&from.work_area(), &to.work_area())
            }
            _ => self,
        }
    }

    /// Whether the window's top edge lies on `areas` (the screens' work
    /// areas) over its whole width: then it can be grabbed and its
    /// buttons reached.
    fn top_edge_on(&self, areas: &[Rect]) -> bool {
        let (left, top) = (i64::from(self.x), i64::from(self.y));
        let right = left + i64::from(self.width);
        let covered: i64 = (areas.iter())
            .filter(|a| {
                top >= i64::from(a.y) && top + TOP_EDGE <= i64::from(a.y) + i64::from(a.height)
            })
            .map(|a| {
                let (a_left, a_right) = (i64::from(a.x), i64::from(a.x) + i64::from(a.width));
                (right.min(a_right) - left.max(a_left)).max(0)
            })
            .sum();
        covered >= i64::from(self.width)
    }

    /// Moved from the screen area `from` to `to`, as far from its corner,
    /// and fitted into it.
    fn moved(self, from: &Rect, to: &Rect) -> Self {
        Self {
            x: self.x.saturating_sub(from.x).saturating_add(to.x),
            y: self.y.saturating_sub(from.y).saturating_add(to.y),
            ..self
        }
        .fitted(to)
    }

    /// The bounds moved, and made smaller where they do not fit, into
    /// `area` (a screen's work area, not empty): a screen that is gone or
    /// smaller now never leaves the window out of reach.
    fn fitted(self, area: &Rect) -> Self {
        let width = self.width.min(area.width);
        let height = self.height.min(area.height);
        Self {
            x: self.x.clamp(area.x, area.x + area.width - width),
            y: self.y.clamp(area.y, area.y + area.height - height),
            width,
            height,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn placed(x: i32, y: i32, width: i32, height: i32) -> WindowPlacement {
        WindowPlacement {
            x,
            y,
            width,
            height,
            maximized: false,
        }
    }

    const PLACED: WindowPlacement = placed(100, 80, 1200, 800);

    #[test]
    fn a_maximized_window_keeps_the_bounds_it_goes_back_to() {
        let screen = placed(0, 25, 1728, 1080).bounds();
        let maximized = WindowPlacement::after(Some(PLACED), &screen, true);
        assert_eq!(
            maximized,
            WindowPlacement {
                maximized: true,
                ..PLACED
            }
        );
        // Without bounds from before, the screen's are all there is.
        assert_eq!(WindowPlacement::after(None, &screen, true).width, 1728);
        // Back to normal: the window's own bounds count again.
        let moved = placed(40, 50, 900, 700);
        assert_eq!(
            WindowPlacement::after(Some(maximized), &moved.bounds(), false),
            moved
        );
    }

    #[test]
    fn a_window_stays_where_it_was_while_its_top_edge_is_on_the_screens() {
        // Two screens side by side, the left one with a menu bar.
        let screens = [
            placed(0, 25, 1440, 875).bounds(),
            placed(1440, 0, 1920, 1080).bounds(),
        ];
        assert!(PLACED.top_edge_on(&screens));
        // Across both screens, and reaching below the left one's work area
        // (under the Dock).
        assert!(placed(1000, 100, 1400, 900).top_edge_on(&screens));
        // On a screen that is gone, partly off the screens, wider than
        // both, or with its top above the work area (under the menu bar).
        assert!(!placed(4000, 100, 1200, 800).top_edge_on(&screens));
        assert!(!placed(3000, 100, 1200, 800).top_edge_on(&screens));
        assert!(!placed(-10, 100, 4000, 800).top_edge_on(&screens));
        assert!(!placed(100, 10, 1200, 800).top_edge_on(&screens));
    }

    #[test]
    fn a_window_out_of_reach_opens_inside_a_screen() {
        let screen = placed(0, 25, 1440, 875).bounds();
        // Inside it already: as it was.
        assert_eq!(PLACED.fitted(&screen), PLACED);
        // On a screen that is gone (far to the right): moved onto this one,
        // maximized as it was.
        let away = WindowPlacement {
            x: 3000,
            maximized: true,
            ..PLACED
        };
        assert_eq!(
            away.fitted(&screen),
            WindowPlacement {
                x: 240,
                maximized: true,
                ..PLACED
            }
        );
        // From a larger screen: as large as this one, at its corner.
        assert_eq!(
            placed(-10, 0, 2560, 1400).fitted(&screen),
            placed(0, 25, 1440, 875)
        );
    }

    #[test]
    fn bounds_to_go_back_to_move_along_with_a_maximized_window() {
        let left = placed(0, 25, 1440, 875).bounds();
        let right = placed(1440, 0, 1920, 1080).bounds();
        // As far from the new screen's corner as from the old one's.
        assert_eq!(PLACED.moved(&left, &right), placed(1540, 55, 1200, 800));
        // Back onto the smaller screen: fitted into it.
        let large = placed(1500, 10, 1800, 1000);
        assert_eq!(large.moved(&right, &left), placed(0, 25, 1440, 875));
    }
}
