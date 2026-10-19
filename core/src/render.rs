use std::cmp::Reverse;

use image::RgbImage;

use crate::geometry::Quad;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleHints {
    pub bg: [u8; 3],
    pub fg: [u8; 3],
    pub angle: f32,
}

const RING: i64 = 3;
const INK_FRACTION: f32 = 0.1;
const MAX_SAMPLES: usize = 20_000;
const MIN_LUMA_CONTRAST: f32 = 60.0;

fn median(mut v: Vec<u8>) -> u8 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn dist2(a: [u8; 3], b: [u8; 3]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x as i32 - y as i32).pow(2))
        .sum()
}

fn luma(p: [u8; 3]) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

fn black_or_white(bg: [u8; 3]) -> [u8; 3] {
    if luma(bg) > 127.0 {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}

/// Pushes roughly `budget` evenly strided pixels of the inclusive rect, clipped to the image.
fn sample_rect(
    img: &RgbImage,
    (x0, y0, x1, y1): (i64, i64, i64, i64),
    budget: usize,
    out: &mut Vec<[u8; 3]>,
) {
    let (x0, y0) = (x0.max(0), y0.max(0));
    let (x1, y1) = (
        x1.min(img.width() as i64 - 1),
        y1.min(img.height() as i64 - 1),
    );
    if x0 > x1 || y0 > y1 {
        return;
    }
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let budget = budget.max(1) as i64;
    let sy = ((w * h) as f64 / budget as f64).sqrt().ceil().max(1.0) as i64;
    let rows = (h + sy - 1) / sy;
    let sx = sy.max((w * rows + budget - 1) / budget);
    for (r, y) in (y0..=y1).step_by(sy as usize).enumerate() {
        // Shifting each row's start keeps 1px vertical strokes from falling between sampled columns.
        let start = x0 + r as i64 % sx;
        for x in (start..=x1).step_by(sx as usize) {
            out.push(img.get_pixel(x as u32, y as u32).0);
        }
    }
}

/// Returns (ring, interior) samples for the quad's bounding box.
fn sample_pixels(img: &RgbImage, quad: &Quad) -> (Vec<[u8; 3]>, Vec<[u8; 3]>) {
    let b = quad.bbox();
    let (x0, y0) = (b.x0.floor() as i64, b.y0.floor() as i64);
    let (x1, y1) = (b.x1.ceil() as i64, b.y1.ceil() as i64);
    let (ox0, oy0) = (x0.saturating_sub(RING), y0.saturating_sub(RING));
    let (ox1, oy1) = (x1.saturating_add(RING), y1.saturating_add(RING));
    let strip = MAX_SAMPLES / 4;
    let mut ring = Vec::new();
    sample_rect(img, (ox0, oy0, ox1, y0.saturating_sub(1)), strip, &mut ring);
    sample_rect(img, (ox0, y1.saturating_add(1), ox1, oy1), strip, &mut ring);
    sample_rect(img, (ox0, y0, x0.saturating_sub(1), y1), strip, &mut ring);
    sample_rect(img, (x1.saturating_add(1), y0, ox1, y1), strip, &mut ring);
    let mut interior = Vec::new();
    sample_rect(img, (x0, y0, x1, y1), MAX_SAMPLES, &mut interior);
    (ring, interior)
}

fn foreground(pixels: &[[u8; 3]], bg: [u8; 3]) -> Option<[u8; 3]> {
    let mut d: Vec<(i32, [u8; 3])> = pixels.iter().map(|p| (dist2(*p, bg), *p)).collect();
    let k = ((d.len() as f32 * INK_FRACTION) as usize).max(1);
    if d.len() > k {
        d.select_nth_unstable_by_key(k - 1, |e| Reverse(e.0));
    }
    let top = &d[..k.min(d.len())];
    let max = top.iter().map(|e| e.0).max().filter(|m| *m > 0)?;
    // Keep only pixels at least half as far from bg as the strongest ink, dropping anti-aliased
    // edges and background noise that dominate the top fraction for thin strokes.
    let ink: Vec<[u8; 3]> = top.iter().filter(|e| e.0 * 4 >= max).map(|e| e.1).collect();
    Some([0, 1, 2].map(|c| (ink.iter().map(|p| p[c] as u32).sum::<u32>() / ink.len() as u32) as u8))
}

pub fn style_hints(img: &RgbImage, quad: &Quad) -> StyleHints {
    let (ring, interior) = sample_pixels(img, quad);
    let bg_samples = if ring.is_empty() { &interior } else { &ring };
    let bg = if bg_samples.is_empty() {
        [255, 255, 255]
    } else {
        [0, 1, 2].map(|c| median(bg_samples.iter().map(|p| p[c]).collect()))
    };
    let fg = foreground(&interior, bg)
        .filter(|fg| (luma(*fg) - luma(bg)).abs() >= MIN_LUMA_CONTRAST)
        .unwrap_or_else(|| black_or_white(bg));
    StyleHints {
        bg,
        fg,
        angle: quad.angle(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::BBox;
    use image::Rgb;

    fn sign() -> (RgbImage, Quad) {
        let mut img = RgbImage::from_pixel(200, 100, Rgb([250, 240, 200]));
        for y in 40..60 {
            for x in 50..150 {
                if (x / 5) % 2 == 0 {
                    img.put_pixel(x, y, Rgb([20, 20, 120]));
                }
            }
        }
        (
            img,
            Quad::from_bbox(BBox {
                x0: 45.0,
                y0: 35.0,
                x1: 155.0,
                y1: 65.0,
            }),
        )
    }

    fn close(a: [u8; 3], b: [u8; 3]) -> bool {
        a.iter()
            .zip(b)
            .all(|(x, y)| (*x as i32 - y as i32).abs() <= 10)
    }

    #[test]
    fn samples_background_from_ring_and_foreground_from_ink() {
        let (img, q) = sign();
        let h = style_hints(&img, &q);
        assert!(close(h.bg, [250, 240, 200]), "bg {:?}", h.bg);
        assert!(close(h.fg, [20, 20, 120]), "fg {:?}", h.fg);
        assert_eq!(h.angle, 0.0);
    }

    #[test]
    fn region_covering_whole_image_still_returns_hints() {
        let img = RgbImage::from_pixel(10, 10, Rgb([0, 0, 0]));
        let q = Quad::from_bbox(BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 9.0,
            y1: 9.0,
        });
        let h = style_hints(&img, &q);
        assert_eq!(h.bg, [0, 0, 0]);
        assert_eq!(h.fg, [255, 255, 255]);
    }

    #[test]
    fn quad_outside_image_falls_back_to_defaults() {
        let img = RgbImage::from_pixel(200, 100, Rgb([10, 10, 10]));
        let q = Quad::from_bbox(BBox {
            x0: 500.0,
            y0: 500.0,
            x1: 600.0,
            y1: 550.0,
        });
        let h = style_hints(&img, &q);
        assert_eq!(h.bg, [255, 255, 255]);
        assert_eq!(h.fg, [0, 0, 0]);
    }

    #[test]
    fn thin_antialiased_strokes_give_dark_foreground() {
        let mut img = RgbImage::from_fn(300, 80, |x, y| {
            let v = 255 - ((x * 7 + y * 13) % 12) as u8;
            Rgb([v, v, v])
        });
        for y in 25..46 {
            for x in (25..275).step_by(10) {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
                img.put_pixel(x - 1, y, Rgb([170, 170, 170]));
                img.put_pixel(x + 1, y, Rgb([170, 170, 170]));
            }
        }
        let q = Quad::from_bbox(BBox {
            x0: 20.0,
            y0: 20.0,
            x1: 280.0,
            y1: 50.0,
        });
        let h = style_hints(&img, &q);
        assert!(luma(h.fg) < 60.0, "fg {:?}", h.fg);
        assert!(luma(h.bg) > 230.0, "bg {:?}", h.bg);
    }

    #[test]
    fn low_contrast_foreground_falls_back_to_black_or_white() {
        let mut img = RgbImage::from_pixel(100, 60, Rgb([200, 200, 200]));
        for x in 20..80 {
            img.put_pixel(x, 30, Rgb([170, 170, 170]));
        }
        let q = Quad::from_bbox(BBox {
            x0: 10.0,
            y0: 20.0,
            x1: 90.0,
            y1: 40.0,
        });
        assert_eq!(style_hints(&img, &q).fg, [0, 0, 0]);
    }

    #[test]
    fn non_finite_quad_does_not_panic() {
        let img = RgbImage::from_pixel(20, 20, Rgb([0, 0, 0]));
        let q = Quad::from_bbox(BBox {
            x0: f32::NEG_INFINITY,
            y0: f32::NAN,
            x1: f32::INFINITY,
            y1: f32::INFINITY,
        });
        style_hints(&img, &q);
    }

    #[test]
    fn huge_region_sampling_is_bounded() {
        let img = RgbImage::from_pixel(4000, 3000, Rgb([30, 30, 30]));
        let q = Quad::from_bbox(BBox {
            x0: 2.0,
            y0: 2.0,
            x1: 3997.0,
            y1: 2997.0,
        });
        let (ring, interior) = sample_pixels(&img, &q);
        assert!(!ring.is_empty());
        assert!(ring.len() <= MAX_SAMPLES * 11 / 10, "{}", ring.len());
        assert!(
            interior.len() <= MAX_SAMPLES * 11 / 10,
            "{}",
            interior.len()
        );
        assert!(interior.len() >= MAX_SAMPLES / 2, "{}", interior.len());
        let h = style_hints(&img, &q);
        assert_eq!(h.bg, [30, 30, 30]);
    }

    #[test]
    fn thin_region_sampling_is_bounded() {
        let img = RgbImage::from_pixel(60_000, 2, Rgb([30, 30, 30]));
        let q = Quad::from_bbox(BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 59_999.0,
            y1: 0.0,
        });
        let (_, interior) = sample_pixels(&img, &q);
        assert!(
            interior.len() <= MAX_SAMPLES * 11 / 10,
            "{}",
            interior.len()
        );
    }
}
