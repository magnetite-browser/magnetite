//! Capture tool, app side. One session at a time: the engine freezes a frame,
//! the overlay page picks an element or region (or asks for the visible area
//! or the full page), and the image work runs on a worker thread. The result
//! is kept in memory only, as PNG, until it is replaced, dismissed or Magnetite
//! quits; it is never written to disk unless the user saves it.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

/// An RGBA image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn decode(png_bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("unexpected palette image".into()),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

pub fn encode(img: &Image) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, img.width, img.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&img.rgba)
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Cut out `[x, y, w, h]` (pixels), clamped to the image.
pub fn crop(img: &Image, x: u32, y: u32, w: u32, h: u32) -> Image {
    // At least one pixel, and inside the image: a start on its far edge
    // would read past the row.
    let x = x.min(img.width.saturating_sub(1));
    let y = y.min(img.height.saturating_sub(1));
    let w = w.clamp(1, (img.width - x).max(1));
    let h = h.clamp(1, (img.height - y).max(1));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for row in y..(y + h).min(img.height) {
        let start = ((row * img.width + x) * 4) as usize;
        rgba.extend_from_slice(&img.rgba[start..start + (w * 4) as usize]);
    }
    let height = u32::try_from(rgba.len() / (w as usize * 4)).unwrap_or(0);
    Image {
        width: w,
        height,
        rgba,
    }
}

/// Stack tiles vertically; all are cut to the narrowest width.
pub fn stack(tiles: &[Image]) -> Image {
    let width = tiles.iter().map(|t| t.width).min().unwrap_or(1);
    let mut rgba = Vec::new();
    let mut height = 0;
    for t in tiles {
        let t = if t.width == width {
            t.clone()
        } else {
            crop(t, 0, 0, width, t.height)
        };
        rgba.extend_from_slice(&t.rgba);
        height += t.height;
    }
    Image {
        width,
        height,
        rgba,
    }
}

/// Downscale to at most `max_w` wide by averaging boxes of pixels.
pub fn thumbnail(img: &Image, max_w: u32) -> Image {
    if img.width <= max_w {
        return img.clone();
    }
    let factor = img.width.div_ceil(max_w);
    let (w, h) = (img.width / factor, (img.height / factor).max(1));
    // An image lower than one box of pixels is averaged over the rows it has.
    let rows = factor.min(img.height);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for ty in 0..h {
        for tx in 0..w {
            let mut sum = [0u32; 4];
            for dy in 0..rows {
                for dx in 0..factor {
                    let i = (((ty * factor + dy) * img.width + tx * factor + dx) * 4) as usize;
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += u32::from(img.rgba[i + c]);
                    }
                }
            }
            let n = rows * factor;
            rgba.extend(sum.iter().map(|s| u8::try_from(s / n).unwrap_or(255)));
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}

pub fn data_url(png_bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", B64.encode(png_bytes))
}

/// A finished capture: PNG bytes plus what the sidebar shows.
pub struct Capture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Full page only: the page was taller than the budget; CSS px captured.
    pub cut_at: Option<u32>,
    pub thumb: String,
}

/// Region in CSS pixels of the frozen frame → crop of its PNG.
pub fn finish_region(frame_png: &[u8], css_width: f64, rect: [f64; 4]) -> Result<Capture, String> {
    let img = decode(frame_png)?;
    let scale = f64::from(img.width) / css_width.max(1.0);
    // The part of the region that is in the frame: an element scrolled
    // partly out at the top or left must not grow at its other end.
    let [x, y, w, h] = rect;
    let (left, top) = (x.max(0.0), y.max(0.0));
    let inside = [left, top, (x + w).max(left) - left, (y + h).max(top) - top];
    // Pixel coordinates: rounded and clamped by `crop`, so the casts are safe.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let px = inside.map(|v| (v * scale).round() as u32);
    finish(&crop(&img, px[0], px[1], px[2], px[3]), None)
}

pub fn finish_tiles(tiles: &[Vec<u8>], cut_at: Option<f64>) -> Result<Capture, String> {
    let images = tiles
        .iter()
        .map(|t| decode(t))
        .collect::<Result<Vec<_>, _>>()?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    finish(&stack(&images), cut_at.map(|c| c as u32))
}

fn finish(img: &Image, cut_at: Option<u32>) -> Result<Capture, String> {
    let png = encode(img)?;
    let thumb = data_url(&encode(&thumbnail(img, 480))?);
    Ok(Capture {
        png,
        width: img.width,
        height: img.height,
        cut_at,
        thumb,
    })
}

/// "Magnetite capture 2026-11-06 at 16.20.11.png" in local time.
pub fn file_name() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let t = libc::time_t::try_from(now).unwrap_or(0);
    // SAFETY: a plain C struct; localtime writes into this one, which we own.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    #[cfg(unix)]
    unsafe {
        libc::localtime_r(&raw const t, &raw mut tm)
    };
    #[cfg(windows)]
    unsafe {
        libc::localtime_s(&raw mut tm, &raw const t)
    };
    format!(
        "Magnetite capture {:04}-{:02}-{:02} at {:02}.{:02}.{:02}.png",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Image {
        let rgba = (0..h)
            .flat_map(|y| {
                (0..w).flat_map(move |x| {
                    [
                        u8::try_from(x % 256).unwrap(),
                        u8::try_from(y % 256).unwrap(),
                        0,
                        255,
                    ]
                })
            })
            .collect();
        Image {
            width: w,
            height: h,
            rgba,
        }
    }

    #[test]
    fn regions_at_the_edges_stay_inside_the_frame() {
        // A start on the far edge still gives one column, not a panic.
        let c = crop(&gradient(40, 30), 40, 0, 10, 30);
        assert_eq!((c.width, c.height), (1, 30));
        // A wide, very low picture can be made smaller.
        let t = thumbnail(&gradient(1200, 2), 480);
        assert_eq!((t.width, t.height), (400, 1));
        // An element scrolled out at the top and left: only what is in the
        // frame, not as much again below and to the right. The frame is
        // 200 x 100 px for 100 x 50 CSS px.
        let frame = encode(&gradient(200, 100)).unwrap();
        let cut = finish_region(&frame, 100.0, [-10.0, -5.0, 30.0, 15.0]).unwrap();
        assert_eq!((cut.width, cut.height), (40, 20));
    }

    #[test]
    fn png_roundtrip_crop_stack_and_thumbnail() {
        let img = gradient(40, 30);
        let back = decode(&encode(&img).unwrap()).unwrap();
        assert_eq!(back, img);

        let c = crop(&img, 10, 5, 8, 4);
        assert_eq!((c.width, c.height), (8, 4));
        assert_eq!(
            &c.rgba[..4],
            &[10, 5, 0, 255],
            "top-left pixel comes from (10, 5)"
        );
        let clamped = crop(&img, 35, 28, 100, 100);
        assert_eq!((clamped.width, clamped.height), (5, 2));

        let s = stack(&[gradient(40, 10), gradient(38, 5)]);
        assert_eq!((s.width, s.height), (38, 15));

        let t = thumbnail(&gradient(100, 50), 30);
        assert!(t.width <= 30 && t.height >= 1);
    }

    #[test]
    fn regions_scale_from_css_to_device_pixels() {
        let frame = encode(&gradient(200, 100)).unwrap(); // 2x of a 100x50 CSS frame
        let cap = finish_region(&frame, 100.0, [10.0, 5.0, 20.0, 10.0]).unwrap();
        assert_eq!((cap.width, cap.height), (40, 20));
        assert!(cap.thumb.starts_with("data:image/png;base64,"));
    }
}
