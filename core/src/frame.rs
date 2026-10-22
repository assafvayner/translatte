use image::{imageops, DynamicImage, RgbImage, RgbaImage};

use crate::{Result, TranslatteError};

/// Turns a camera frame (tightly packed RGBA rows, `width` x `height`) into an upright RGB image
/// by rotating it clockwise by `rotation_degrees`: 0, 90, 180 or 270.
pub fn upright_rgb(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    rotation_degrees: u32,
) -> Result<RgbImage> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .unwrap_or(usize::MAX);
    if rgba.len() != expected {
        return Err(TranslatteError::Image(format!(
            "frame is {} bytes, expected {expected} for {width}x{height} RGBA",
            rgba.len()
        )));
    }
    if ![0, 90, 180, 270].contains(&rotation_degrees) {
        return Err(TranslatteError::Image(format!(
            "unsupported frame rotation {rotation_degrees}°"
        )));
    }
    let rgba = RgbaImage::from_raw(width, height, rgba).expect("size checked above");
    let rgb = DynamicImage::ImageRgba8(rgba).to_rgb8();
    Ok(match rotation_degrees {
        90 => imageops::rotate90(&rgb),
        180 => imageops::rotate180(&rgb),
        270 => imageops::rotate270(&rgb),
        _ => rgb,
    })
}

/// Frames are scored for sharpness at most this long, which keeps scoring cheap.
const SHARPNESS_SIDE: u32 = 640;

/// How sharp `img` is: the variance of the Laplacian of its luma, downscaled by box averaging so
/// the long side is at most [`SHARPNESS_SIDE`]. Motion blur drives it toward 0.
pub fn sharpness(img: &RgbImage) -> f32 {
    sharpness_at(img, SHARPNESS_SIDE)
}

fn sharpness_at(img: &RgbImage, side: u32) -> f32 {
    let (w, h) = img.dimensions();
    let f = w.max(h).div_ceil(side.max(1)).max(1) as usize;
    let (sw, sh) = (w as usize / f, h as usize / f);
    if sw < 3 || sh < 3 {
        return 0.0;
    }
    let mut luma = vec![0f32; sw * sh];
    for (y, row) in img.rows().enumerate().take(sh * f) {
        let out = &mut luma[(y / f) * sw..][..sw];
        for (x, p) in row.take(sw * f).enumerate() {
            let [r, g, b] = p.0;
            out[x / f] += 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        }
    }
    let norm = (f * f) as f32;
    let at = |x: usize, y: usize| luma[y * sw + x] / norm;
    let (mut sum, mut sum2) = (0f64, 0f64);
    for y in 1..sh - 1 {
        for x in 1..sw - 1 {
            let l =
                (at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1) - 4.0 * at(x, y)) as f64;
            sum += l;
            sum2 += l * l;
        }
    }
    let n = ((sw - 2) * (sh - 2)) as f64;
    let mean = sum / n;
    (sum2 / n - mean * mean) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 7];

    /// A 2x1 frame: red on the left, green on the right.
    fn frame() -> Vec<u8> {
        [RED, GREEN].concat()
    }

    fn pixels(img: &RgbImage) -> Vec<[u8; 3]> {
        img.pixels().map(|p| p.0).collect()
    }

    #[test]
    fn unrotated_frames_drop_alpha() {
        let img = upright_rgb(frame(), 2, 1, 0).unwrap();
        assert_eq!(img.dimensions(), (2, 1));
        assert_eq!(pixels(&img), vec![[255, 0, 0], [0, 255, 0]]);
    }

    #[test]
    fn rotates_clockwise() {
        let img = upright_rgb(frame(), 2, 1, 90).unwrap();
        assert_eq!(img.dimensions(), (1, 2));
        assert_eq!(pixels(&img), vec![[255, 0, 0], [0, 255, 0]]);
        let img = upright_rgb(frame(), 2, 1, 180).unwrap();
        assert_eq!(pixels(&img), vec![[0, 255, 0], [255, 0, 0]]);
        let img = upright_rgb(frame(), 2, 1, 270).unwrap();
        assert_eq!(img.dimensions(), (1, 2));
        assert_eq!(pixels(&img), vec![[0, 255, 0], [255, 0, 0]]);
    }

    #[test]
    fn blur_lowers_sharpness() {
        let page = RgbImage::from_fn(1280, 720, |x, y| {
            if (x / 3 + y / 7) % 2 == 0 {
                Rgb([20, 20, 20])
            } else {
                Rgb([240, 240, 240])
            }
        });
        let sharp = sharpness(&page);
        let blurred = sharpness(&imageops::blur(&page, 6.0));
        assert!(sharp > 1000.0, "{sharp}");
        assert!(blurred < sharp / 10.0, "{blurred} vs {sharp}");
        assert_eq!(
            sharpness(&RgbImage::from_pixel(64, 64, Rgb([9, 9, 9]))),
            0.0
        );
        assert_eq!(sharpness(&RgbImage::new(2, 2)), 0.0);
    }

    #[test]
    fn rejects_a_wrong_size_or_rotation() {
        let err = upright_rgb(vec![0; 7], 2, 1, 0).unwrap_err();
        assert!(err.to_string().contains("expected 8"), "{err}");
        let err = upright_rgb(frame(), 2, 1, 45).unwrap_err();
        assert!(err.to_string().contains("rotation"), "{err}");
    }
}
