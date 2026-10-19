use image::{GrayImage, Luma};
use imageproc::contours::{find_contours, BorderType};
use imageproc::geometry::min_area_rect;
use ndarray::ArrayView2;

use crate::geometry::{Pt, Quad};

/// Defaults from PP-OCRv5_mobile_det inference.yml (DBPostProcess).
#[derive(Debug, Clone)]
pub struct DbParams {
    pub thresh: f32,
    pub box_thresh: f32,
    pub unclip_ratio: f32,
    pub min_size: f32,
    pub max_candidates: usize,
}

impl Default for DbParams {
    fn default() -> Self {
        Self {
            thresh: 0.3,
            box_thresh: 0.6,
            unclip_ratio: 1.5,
            min_size: 3.0,
            max_candidates: 1000,
        }
    }
}

pub fn db_postprocess(prob: ArrayView2<f32>, params: &DbParams) -> Vec<(Quad, f32)> {
    let (h, w) = prob.dim();
    let bitmap = GrayImage::from_fn(w as u32, h as u32, |x, y| {
        Luma([if prob[[y as usize, x as usize]] > params.thresh {
            255
        } else {
            0
        }])
    });
    let mut out = Vec::new();
    for contour in find_contours::<i32>(&bitmap)
        .into_iter()
        .filter(|c| c.border_type == BorderType::Outer)
        .take(params.max_candidates)
    {
        if contour.points.len() < 4 {
            continue;
        }
        let rect = min_area_rect(&contour.points);
        let quad = Quad::from_unordered(rect.map(|p| Pt::new(p.x as f32, p.y as f32)));
        if quad.width().min(quad.height()) < params.min_size {
            continue;
        }
        let score = box_score(prob, &quad);
        if score < params.box_thresh {
            continue;
        }
        let expanded = unclip(&quad, params.unclip_ratio);
        if expanded.width().min(expanded.height()) < params.min_size + 2.0 {
            continue;
        }
        out.push((expanded, score));
    }
    out
}

fn box_score(prob: ArrayView2<f32>, quad: &Quad) -> f32 {
    let (h, w) = prob.dim();
    let b = quad.bbox();
    let (x0, y0) = (
        b.x0.floor().max(0.0) as usize,
        b.y0.floor().max(0.0) as usize,
    );
    let (x1, y1) = (
        (b.x1.ceil() as usize).min(w - 1),
        (b.y1.ceil() as usize).min(h - 1),
    );
    let (mut sum, mut n) = (0.0f32, 0usize);
    for y in y0..=y1 {
        for x in x0..=x1 {
            if quad.contains(Pt::new(x as f32, y as f32)) {
                sum += prob[[y, x]];
                n += 1;
            }
        }
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f32
    }
}

/// For a rectangle, pyclipper's polygon offset by d = area * ratio / perimeter equals growing each side by d.
pub fn unclip(quad: &Quad, ratio: f32) -> Quad {
    let (w, h) = (quad.width(), quad.height());
    if w == 0.0 || h == 0.0 {
        return *quad;
    }
    let d = w * h * ratio / (2.0 * (w + h));
    let [tl, tr, br, bl] = quad.0;
    let ux = Pt::new((tr.x - tl.x) / tl.dist(tr), (tr.y - tl.y) / tl.dist(tr));
    let uy = Pt::new((bl.x - tl.x) / tl.dist(bl), (bl.y - tl.y) / tl.dist(bl));
    let shift = |p: Pt, sx: f32, sy: f32| {
        Pt::new(
            p.x + (ux.x * sx + uy.x * sy) * d,
            p.y + (ux.y * sx + uy.y * sy) * d,
        )
    };
    Quad([
        shift(tl, -1.0, -1.0),
        shift(tr, 1.0, -1.0),
        shift(br, 1.0, 1.0),
        shift(bl, -1.0, 1.0),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array2;

    fn map_with(rect: (usize, usize, usize, usize), v: f32) -> Array2<f32> {
        let mut m = Array2::<f32>::zeros((100, 200));
        let (y0, y1, x0, x1) = rect;
        m.slice_mut(ndarray::s![y0..y1, x0..x1]).fill(v);
        m
    }

    #[test]
    fn finds_and_unclips_a_confident_region() {
        let m = map_with((20, 40, 30, 150), 0.9);
        let out = db_postprocess(m.view(), &DbParams::default());
        assert_eq!(out.len(), 1);
        let (q, score) = out[0];
        assert!((score - 0.9).abs() < 0.01);
        let b = q.bbox();
        // Contour spans x 30..=149, y 20..=39; d = 119*19*1.5 / (2*(119+19)) ≈ 12.3
        assert!((b.x0 - 17.7).abs() < 1.5, "x0 {}", b.x0);
        assert!((b.y0 - 7.7).abs() < 1.5, "y0 {}", b.y0);
        assert!((b.x1 - 161.3).abs() < 1.5, "x1 {}", b.x1);
        assert!((b.y1 - 51.3).abs() < 1.5, "y1 {}", b.y1);
    }

    #[test]
    fn drops_regions_below_box_threshold() {
        let m = map_with((20, 40, 30, 150), 0.4);
        assert!(db_postprocess(m.view(), &DbParams::default()).is_empty());
    }

    #[test]
    fn drops_tiny_regions() {
        let m = map_with((20, 22, 30, 32), 0.9);
        assert!(db_postprocess(m.view(), &DbParams::default()).is_empty());
    }

    #[test]
    fn finds_two_separate_regions() {
        let mut m = map_with((10, 25, 10, 90), 0.9);
        m.slice_mut(ndarray::s![60..80, 100..190]).fill(0.8);
        assert_eq!(db_postprocess(m.view(), &DbParams::default()).len(), 2);
    }
}
