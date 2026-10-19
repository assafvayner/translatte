use image::RgbImage;

use crate::geometry::Quad;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleHints {
    pub bg: [u8; 3],
    pub fg: [u8; 3],
    pub angle: f32,
}

const RING: i64 = 3;
const INK_FRACTION: f32 = 0.2;

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

pub fn style_hints(img: &RgbImage, quad: &Quad) -> StyleHints {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let b = quad.bbox();
    let (x0, y0) = (b.x0.floor() as i64, b.y0.floor() as i64);
    let (x1, y1) = (b.x1.ceil() as i64, b.y1.ceil() as i64);
    let px = |x: i64, y: i64| img.get_pixel(x as u32, y as u32).0;
    let in_img = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h;
    let inside = |x: i64, y: i64| x >= x0 && x <= x1 && y >= y0 && y <= y1;

    let mut ring = Vec::new();
    for y in y0 - RING..=y1 + RING {
        for x in x0 - RING..=x1 + RING {
            if in_img(x, y) && !inside(x, y) {
                ring.push(px(x, y));
            }
        }
    }
    let mut interior = Vec::new();
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            interior.push(px(x, y));
        }
    }
    let bg_samples = if ring.is_empty() { &interior } else { &ring };
    let bg = [0, 1, 2].map(|c| median(bg_samples.iter().map(|p| p[c]).collect()));

    interior.sort_by_key(|p| std::cmp::Reverse(dist2(*p, bg)));
    let take = ((interior.len() as f32 * INK_FRACTION) as usize).max(1);
    let ink: Vec<[u8; 3]> = interior
        .into_iter()
        .take(take)
        .filter(|p| dist2(*p, bg) > 0)
        .collect();
    let fg = if ink.is_empty() {
        let luma = 0.299 * bg[0] as f32 + 0.587 * bg[1] as f32 + 0.114 * bg[2] as f32;
        if luma > 127.0 {
            [0, 0, 0]
        } else {
            [255, 255, 255]
        }
    } else {
        [0, 1, 2].map(|c| (ink.iter().map(|p| p[c] as u32).sum::<u32>() / ink.len() as u32) as u8)
    };
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
}
