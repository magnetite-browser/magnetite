//! Small line icons for rows built from CEF Views (the mini window's), drawn
//! here so they look the same on every system and need no image files.
//! Shapes are polylines on a 24-unit grid, stroked 2 units wide with round
//! ends, like the sidebar's icons.

use cef::*;

/// The icons there are.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Glyph {
    Back,
    Forward,
    Reload,
    Down,
    Close,
}

type Point = (f32, f32);

impl Glyph {
    /// The strokes of the icon on the 24-unit grid.
    fn lines(self) -> Vec<Vec<Point>> {
        match self {
            Self::Back => vec![vec![(15.0, 18.0), (9.0, 12.0), (15.0, 6.0)]],
            Self::Forward => vec![vec![(9.0, 18.0), (15.0, 12.0), (9.0, 6.0)]],
            Self::Down => vec![vec![(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]],
            Self::Close => vec![
                vec![(18.0, 6.0), (6.0, 18.0)],
                vec![(6.0, 6.0), (18.0, 18.0)],
            ],
            Self::Reload => {
                // Most of a circle, clockwise from the right to the upper
                // right, then the arrow head at its end.
                let arc = (0..=40_u8).map(|step| {
                    let angle = (f32::from(step) / 40.0 * 318.0).to_radians();
                    (12.0 + 9.0 * angle.cos(), 12.0 + 9.0 * angle.sin())
                });
                vec![
                    arc.chain([(21.0, 8.0)]).collect(),
                    vec![(21.0, 3.0), (21.0, 8.0), (16.0, 8.0)],
                ]
            }
        }
    }

    /// The icon `size` pixels wide and high in `argb`, as BGRA pixels with
    /// premultiplied alpha. With a `disc` colour the icon lies, smaller and
    /// thinner, on a filled circle as wide as the picture.
    // The casts are of values kept between 0 and 255.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn pixels(self, size: u16, argb: u32, disc: Option<u32>) -> Vec<u8> {
        let scale = f32::from(size) / 24.0;
        // On a disc the strokes shrink towards the middle.
        let (shrink, half) = if disc.is_some() {
            (0.56, 0.7)
        } else {
            (1.0, 1.0)
        };
        let lines: Vec<Vec<Point>> = (self.lines().into_iter())
            .map(|line| {
                let towards = |n: f32| 12.0 + (n - 12.0) * shrink;
                line.into_iter()
                    .map(|(x, y)| (towards(x), towards(y)))
                    .collect()
            })
            .collect();
        let mut out = Vec::with_capacity(usize::from(size).pow(2) * 4);
        for row in 0..size {
            for column in 0..size {
                // The pixel's centre on the grid, and how far the nearest
                // stroke is from it.
                let centre = |n: u16| (f32::from(n) + 0.5) / scale;
                let point = (centre(column), centre(row));
                let near = lines
                    .iter()
                    .flat_map(|line| line.windows(2))
                    .map(|ends| distance(point, ends[0], ends[1]))
                    .fold(f32::MAX, f32::min);
                // To the stroke's edge, softened over a pixel; the disc
                // reaches the picture's edge.
                let stroke = ((half - near) * scale + 0.5).clamp(0.0, 1.0);
                let middle = distance(point, (12.0, 12.0), (12.0, 12.0));
                let round = ((12.0 - middle) * scale + 0.5).clamp(0.0, 1.0);
                // B, G, R, A of a colour covering that much of the pixel.
                let ink = |argb: u32, cover: f32| {
                    let [alpha, red, green, blue] = argb.to_be_bytes();
                    let opacity = cover * f32::from(alpha) / 255.0;
                    [blue, green, red, 255].map(|c| f32::from(c) * opacity)
                };
                let over = ink(argb, stroke);
                let under = disc.map_or([0.0; 4], |d| ink(d, round));
                let rest = 1.0 - over[3] / 255.0;
                out.extend((0..4).map(|c| (over[c] + under[c] * rest).round() as u8));
            }
        }
        out
    }

    /// The icon as a CEF image `dip` points wide, for plain and sharp
    /// (2x) screens.
    pub(crate) fn image(self, dip: u16, argb: u32) -> Option<Image> {
        self.drawn(dip, argb, None)
    }

    /// The icon in `argb` on a filled circle in `disc`, `dip` points wide.
    pub(crate) fn image_on_disc(self, dip: u16, argb: u32, disc: u32) -> Option<Image> {
        self.drawn(dip, argb, Some(disc))
    }

    fn drawn(self, dip: u16, argb: u32, disc: Option<u32>) -> Option<Image> {
        let image = image_create()?;
        for factor in [1_u16, 2] {
            let pixels = dip * factor;
            image.add_bitmap(
                f32::from(factor),
                i32::from(pixels),
                i32::from(pixels),
                ColorType::BGRA_8888,
                AlphaType::PREMULTIPLIED,
                Some(&self.pixels(pixels, argb, disc)),
            );
        }
        Some(image)
    }
}

/// How far point `p` is from the segment `a`–`b`.
fn distance(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx * dx + dy * dy;
    let along = if len == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len).clamp(0.0, 1.0)
    };
    ((p.0 - a.0 - along * dx).powi(2) + (p.1 - a.1 - along * dy).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::Glyph;

    const SIZE: usize = 32;

    fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
        &pixels[(y * SIZE + x) * 4..][..4]
    }

    #[test]
    fn an_icon_is_a_thin_line_in_the_asked_colour_on_nothing() {
        let cross = Glyph::Close.pixels(32, 0xFF80_4020, None);
        assert_eq!(cross.len(), SIZE * SIZE * 4);
        // The cross goes through the middle, in B, G, R, A.
        assert_eq!(pixel(&cross, 16, 16), [0x20, 0x40, 0x80, 0xFF]);
        // Corners and the middle of an edge are empty.
        assert_eq!(pixel(&cross, 0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&cross, 16, 2), [0, 0, 0, 0]);
        // Mostly empty: a line, not a blob.
        let inked = cross.chunks(4).filter(|p| p[3] > 0).count();
        assert!(inked * 5 < SIZE * SIZE, "{inked} pixels inked");
        // Back and Forward mirror each other.
        let back = Glyph::Back.pixels(32, !0, None);
        let forward = Glyph::Forward.pixels(32, !0, None);
        for y in 0..SIZE {
            for x in 0..SIZE {
                assert_eq!(pixel(&back, x, y), pixel(&forward, SIZE - 1 - x, y));
            }
        }
    }
}
