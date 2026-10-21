use image::imageops::{self, FilterType};
use image::{Rgb, RgbImage};
use imageproc::geometric_transformations::{warp_into, Interpolation, Projection};
use ndarray::{Array4, ArrayViewMut3, Axis};

use crate::geometry::Quad;

const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];
pub const REC_HEIGHT: u32 = 48;
/// PaddleOCR pads lines to at least 320 wide; 160 recognizes as well on the fixtures with less
/// compute for short lines.
const REC_MIN_WIDTH: u32 = 160;
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

/// Writes `img` into the top-left of `out` ([channel, y, x]) as `(v / 255 - mean) / std` per
/// channel. Paddle models were trained on BGR input, so channel 0 is blue.
fn write_normalized(img: &RgbImage, mut out: ArrayViewMut3<f32>, mean: [f32; 3], std: [f32; 3]) {
    let row_len = img.width() as usize * 3;
    if row_len == 0 {
        return;
    }
    for (c, mut plane) in out.axis_iter_mut(Axis(0)).enumerate() {
        let lut: [f32; 256] = std::array::from_fn(|v| (v as f32 / 255.0 - mean[c]) / std[c]);
        for (mut dst, src) in plane.rows_mut().into_iter().zip(img.chunks_exact(row_len)) {
            for (d, px) in dst.iter_mut().zip(src.chunks_exact(3)) {
                *d = lut[px[2 - c] as usize];
            }
        }
    }
}

pub fn det_preprocess(img: &RgbImage, resize_long: u32) -> DetInput {
    let (w, h) = img.dimensions();
    let ratio = resize_long as f32 / w.max(h) as f32;
    let (rw, rh) = (round_to_32(w as f32 * ratio), round_to_32(h as f32 * ratio));
    let resized = imageops::resize(img, rw, rh, FilterType::Triangle);
    let mut tensor = Array4::<f32>::zeros((1, 3, rh as usize, rw as usize));
    write_normalized(
        &resized,
        tensor.index_axis_mut(Axis(0), 0),
        DET_MEAN,
        DET_STD,
    );
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

/// Width of `crop` scaled to the recognizer's input height.
fn rec_width(crop: &RgbImage) -> u32 {
    let (w, h) = crop.dimensions();
    ((REC_HEIGHT as f32 * w as f32 / h as f32).ceil() as u32).clamp(1, REC_MAX_WIDTH)
}

/// Width of the recognizer input for `crop` alone, padding included.
pub fn rec_input_width(crop: &RgbImage) -> u32 {
    rec_width(crop).max(REC_MIN_WIDTH)
}

pub fn rec_preprocess(crop: &RgbImage) -> Array4<f32> {
    rec_preprocess_batch(&[crop])
}

/// Scales each crop to the recognizer's input height and stacks them into one batch as wide as the
/// widest (at least `REC_MIN_WIDTH`), zero-padding the normalized tensor on the right, as
/// PaddleOCR does when batching.
pub fn rec_preprocess_batch(crops: &[&RgbImage]) -> Array4<f32> {
    let width = crops
        .iter()
        .map(|c| rec_input_width(c))
        .max()
        .unwrap_or(REC_MIN_WIDTH);
    let mut tensor = Array4::<f32>::zeros((crops.len(), 3, REC_HEIGHT as usize, width as usize));
    for (i, crop) in crops.iter().enumerate() {
        let resized = imageops::resize(*crop, rec_width(crop), REC_HEIGHT, FilterType::Triangle);
        write_normalized(
            &resized,
            tensor.index_axis_mut(Axis(0), i),
            [0.5; 3],
            [0.5; 3],
        );
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
    fn det_preprocess_puts_blue_in_channel_0() {
        let img = RgbImage::from_pixel(64, 32, Rgb([200, 100, 50]));
        let t = det_preprocess(&img, 64).tensor;
        for (c, v) in [50.0, 100.0, 200.0].into_iter().enumerate() {
            let want = (v / 255.0 - DET_MEAN[c]) / DET_STD[c];
            assert!((t[[0, c, 31, 63]] - want).abs() < 1e-6, "channel {c}");
        }
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
    fn rec_preprocess_scales_to_height_48_and_pads_to_min_width() {
        let img = RgbImage::from_pixel(96, 48, Rgb([255, 255, 255]));
        let t = rec_preprocess(&img);
        assert_eq!(t.dim(), (1, 3, 48, 160));
        assert!((t[[0, 0, 0, 0]] - 1.0).abs() < 1e-6);
        assert_eq!(t[[0, 0, 0, 120]], 0.0);
    }

    #[test]
    fn rec_preprocess_batch_pads_to_the_widest_crop() {
        let narrow = RgbImage::from_pixel(96, 48, Rgb([255, 255, 255]));
        let wide = RgbImage::from_pixel(400, 40, Rgb([0, 0, 0]));
        let t = rec_preprocess_batch(&[&narrow, &wide]);
        assert_eq!(t.dim(), (2, 3, 48, 480));
        assert!((t[[0, 0, 0, 95]] - 1.0).abs() < 1e-6);
        assert_eq!(t[[0, 0, 0, 96]], 0.0);
        assert!((t[[1, 1, 47, 479]] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn rec_preprocess_keeps_wide_crops_unpadded() {
        let img = RgbImage::from_pixel(480, 24, Rgb([0, 0, 0]));
        let t = rec_preprocess(&img);
        assert_eq!(t.dim(), (1, 3, 48, 960));
        assert!((t[[0, 2, 10, 900]] + 1.0).abs() < 1e-6);
    }
}
