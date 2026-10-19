use image::imageops::{self, FilterType};
use image::{Rgb, RgbImage};
use imageproc::geometric_transformations::{warp_into, Interpolation, Projection};
use ndarray::Array4;

use crate::geometry::Quad;

const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];
pub const REC_HEIGHT: u32 = 48;
const REC_MIN_WIDTH: u32 = 320;
const REC_MAX_WIDTH: u32 = 3200;

pub struct DetInput {
    pub tensor: Array4<f32>,
    /// Multiply detector-space coordinates by these to get original image coordinates.
    pub scale_x: f32,
    pub scale_y: f32,
}

fn round_to_32(v: f32) -> u32 {
    ((v / 32.0).round() as u32).max(1) * 32
}

/// Paddle models were trained on BGR input; channel 0 of every tensor is blue.
fn bgr(p: &Rgb<u8>) -> [f32; 3] {
    [
        p[2] as f32 / 255.0,
        p[1] as f32 / 255.0,
        p[0] as f32 / 255.0,
    ]
}

pub fn det_preprocess(img: &RgbImage, resize_long: u32) -> DetInput {
    let (w, h) = img.dimensions();
    let ratio = resize_long as f32 / w.max(h) as f32;
    let (rw, rh) = (round_to_32(w as f32 * ratio), round_to_32(h as f32 * ratio));
    let resized = imageops::resize(img, rw, rh, FilterType::Triangle);
    let mut tensor = Array4::<f32>::zeros((1, 3, rh as usize, rw as usize));
    for (x, y, p) in resized.enumerate_pixels() {
        let v = bgr(p);
        for c in 0..3 {
            tensor[[0, c, y as usize, x as usize]] = (v[c] - DET_MEAN[c]) / DET_STD[c];
        }
    }
    DetInput {
        tensor,
        scale_x: w as f32 / rw as f32,
        scale_y: h as f32 / rh as f32,
    }
}

/// Perspective-rectifies the quad. Crops at least 1.5x taller than wide are treated as vertical text
/// and rotated 90° counter-clockwise, matching PaddleOCR's get_rotate_crop_image.
pub fn crop_quad(img: &RgbImage, quad: &Quad) -> RgbImage {
    let w = quad.width().round().max(1.0) as u32;
    let h = quad.height().round().max(1.0) as u32;
    let from = quad.0.map(|p| (p.x, p.y));
    let to = [
        (0.0, 0.0),
        (w as f32, 0.0),
        (w as f32, h as f32),
        (0.0, h as f32),
    ];
    let mut out = RgbImage::new(w, h);
    match Projection::from_control_points(from, to) {
        Some(proj) => warp_into(
            img,
            &proj,
            Interpolation::Bilinear,
            Rgb([0, 0, 0]),
            &mut out,
        ),
        None => {
            let b = quad.bbox();
            out = imageops::crop_imm(img, b.x0.max(0.0) as u32, b.y0.max(0.0) as u32, w, h)
                .to_image();
        }
    }
    if h as f32 / w as f32 >= 1.5 {
        imageops::rotate270(&out)
    } else {
        out
    }
}

pub fn rec_preprocess(crop: &RgbImage) -> Array4<f32> {
    let (w, h) = crop.dimensions();
    let rw = ((REC_HEIGHT as f32 * w as f32 / h as f32).ceil() as u32).clamp(1, REC_MAX_WIDTH);
    let resized = imageops::resize(crop, rw, REC_HEIGHT, FilterType::Triangle);
    // Zero padding on the normalized tensor, as PaddleOCR does when batching.
    let mut tensor =
        Array4::<f32>::zeros((1, 3, REC_HEIGHT as usize, rw.max(REC_MIN_WIDTH) as usize));
    for (x, y, p) in resized.enumerate_pixels() {
        let v = bgr(p);
        for c in 0..3 {
            tensor[[0, c, y as usize, x as usize]] = (v[c] - 0.5) / 0.5;
        }
    }
    tensor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{BBox, Quad};
    use image::Rgb;

    #[test]
    fn det_preprocess_resizes_long_side_to_multiple_of_32() {
        let img = RgbImage::from_pixel(1000, 500, Rgb([255, 255, 255]));
        let input = det_preprocess(&img, 960);
        assert_eq!(input.tensor.dim(), (1, 3, 480, 960));
        assert!((input.scale_x - 1000.0 / 960.0).abs() < 1e-6);
        assert!((input.scale_y - 500.0 / 480.0).abs() < 1e-6);
        // White pixel, channel 0 is B: (1 - 0.485) / 0.229
        assert!((input.tensor[[0, 0, 0, 0]] - (1.0 - 0.485) / 0.229).abs() < 1e-4);
    }

    #[test]
    fn crop_quad_extracts_axis_aligned_region() {
        let mut img = RgbImage::from_pixel(100, 100, Rgb([0, 0, 0]));
        for y in 20..40 {
            for x in 10..70 {
                img.put_pixel(x, y, Rgb([200, 100, 50]));
            }
        }
        let q = Quad::from_bbox(BBox {
            x0: 10.0,
            y0: 20.0,
            x1: 70.0,
            y1: 40.0,
        });
        let crop = crop_quad(&img, &q);
        assert_eq!(crop.dimensions(), (60, 20));
        assert_eq!(*crop.get_pixel(30, 10), Rgb([200, 100, 50]));
    }

    #[test]
    fn crop_quad_rotates_tall_crops() {
        let img = RgbImage::from_pixel(100, 100, Rgb([0, 0, 0]));
        let q = Quad::from_bbox(BBox {
            x0: 10.0,
            y0: 10.0,
            x1: 20.0,
            y1: 60.0,
        });
        assert_eq!(crop_quad(&img, &q).dimensions(), (50, 10));
    }

    #[test]
    fn rec_preprocess_scales_to_height_48_and_pads_to_320() {
        let img = RgbImage::from_pixel(96, 48, Rgb([255, 255, 255]));
        let t = rec_preprocess(&img);
        assert_eq!(t.dim(), (1, 3, 48, 320));
        assert!((t[[0, 0, 0, 0]] - 1.0).abs() < 1e-6);
        assert_eq!(t[[0, 0, 0, 200]], 0.0);
    }

    #[test]
    fn rec_preprocess_keeps_wide_crops_unpadded() {
        let img = RgbImage::from_pixel(480, 24, Rgb([0, 0, 0]));
        let t = rec_preprocess(&img);
        assert_eq!(t.dim(), (1, 3, 48, 960));
        assert!((t[[0, 2, 10, 900]] + 1.0).abs() < 1e-6);
    }
}
