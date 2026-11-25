//! Where a frame window is, kept so the window opens the same way at the
//! next start: its size and place while not maximized, and whether it is
//! maximized. CEF tells a maximized window's bounds only as the screen's,
//! so the engine keeps the ones from before (`WindowPlacement::after`).

use cef::{ImplDisplay, Rect, display_get_matching_bounds};
use serde::{Deserialize, Serialize};

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

    /// The placement fitted into the work area of the screen its bounds
    /// are on most, or the nearest one (`fitted`).
    pub(crate) fn on_screen(self) -> Self {
        display_get_matching_bounds(Some(&self.bounds()), 0)
            .map_or(self, |d| self.fitted(&d.work_area()))
    }

    /// The bounds moved, and made smaller where they do not fit, into
    /// `area` (a screen's work area): a screen that is gone or smaller now
    /// never leaves the window out of reach.
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
    fn a_window_opens_inside_a_screen_that_is_there() {
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
}
