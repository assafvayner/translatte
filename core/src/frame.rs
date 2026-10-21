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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn rejects_a_wrong_size_or_rotation() {
        let err = upright_rgb(vec![0; 7], 2, 1, 0).unwrap_err();
        assert!(err.to_string().contains("expected 8"), "{err}");
        let err = upright_rgb(frame(), 2, 1, 45).unwrap_err();
        assert!(err.to_string().contains("rotation"), "{err}");
    }
}
