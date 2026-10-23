//! Global camera motion between two small grayscale frames, by phase correlation: how far the
//! image moved from a live result's source frame to the frame on screen, so the overlay can follow.

use std::f32::consts::PI;

/// The share of correlation energy the peak must hold for a shift to count.
pub const MIN_PEAK: f32 = 0.3;
/// How much stronger the peak must be than the strongest response away from it.
pub const MIN_PEAK_RATIO: f32 = 2.0;

/// How far `current` moved relative to `reference`, in thumbnail pixels: a feature at (x, y) in
/// the reference is at (x + dx, y + dy) in the current frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shift {
    pub dx: f32,
    pub dy: f32,
    /// The peak's share of the correlation energy, 0 to 1.
    pub peak: f32,
}

/// Estimates the translation from `reference` to `current`, two row-major 8-bit grayscale images
/// of `width` x `height` (powers of two). Returns `None` when the sizes don't match or the
/// correlation peak is too weak or ambiguous to trust (flat frames, blur, scene changes).
pub fn estimate_shift(
    reference: &[u8],
    current: &[u8],
    width: usize,
    height: usize,
) -> Option<Shift> {
    Reference::new(reference, width, height)?.shift_to(current)
}

/// A frame to measure later frames' motion against, with its spectrum computed once.
pub struct Reference {
    spectrum: Vec<(f32, f32)>,
    width: usize,
    height: usize,
}

impl Reference {
    /// `None` when `img` isn't `width` x `height` (powers of two) or is too flat to match.
    pub fn new(img: &[u8], width: usize, height: usize) -> Option<Self> {
        if !width.is_power_of_two() || !height.is_power_of_two() || img.len() != width * height {
            return None;
        }
        let mut spectrum = windowed(img, width, height)?;
        fft2(&mut spectrum, width, height, false);
        Some(Self {
            spectrum,
            width,
            height,
        })
    }

    /// See [`estimate_shift`].
    pub fn shift_to(&self, current: &[u8]) -> Option<Shift> {
        let (width, height) = (self.width, self.height);
        let n = width * height;
        if current.len() != n {
            return None;
        }
        let mut a = windowed(current, width, height)?;
        fft2(&mut a, width, height, false);
        for (q, p) in a.iter_mut().zip(&self.spectrum) {
            let re = q.0 * p.0 + q.1 * p.1;
            let im = q.1 * p.0 - q.0 * p.1;
            let mag = (re * re + im * im).sqrt();
            *q = if mag > 1e-9 {
                (re / mag, im / mag)
            } else {
                (0.0, 0.0)
            };
        }
        fft2(&mut a, width, height, true);
        let r: Vec<f32> = a.iter().map(|c| c.0 / n as f32).collect();
        peak(&r, width, height)
    }
}

/// The correlation surface's peak with subpixel refinement, if it is strong and unambiguous.
fn peak(r: &[f32], width: usize, height: usize) -> Option<Shift> {
    let (best, _) = r.iter().enumerate().max_by(|x, y| x.1.total_cmp(y.1))?;
    let (bx, by) = ((best % width) as isize, (best / width) as isize);
    let at = |x: isize, y: isize| {
        r[(y.rem_euclid(height as isize) as usize) * width + x.rem_euclid(width as isize) as usize]
    };
    let (mut sum, mut sx, mut sy) = (0.0, 0.0, 0.0);
    for oy in -1..=1 {
        for ox in -1..=1 {
            let v = at(bx + ox, by + oy).max(0.0);
            sum += v;
            sx += v * ox as f32;
            sy += v * oy as f32;
        }
    }
    let mut second = 0f32;
    for y in 0..height as isize {
        for x in 0..width as isize {
            let ddx = (x - bx)
                .rem_euclid(width as isize)
                .min((bx - x).rem_euclid(width as isize));
            let ddy = (y - by)
                .rem_euclid(height as isize)
                .min((by - y).rem_euclid(height as isize));
            if ddx > 2 || ddy > 2 {
                second = second.max(at(x, y));
            }
        }
    }
    if sum < MIN_PEAK || at(bx, by) < MIN_PEAK_RATIO * second {
        return None;
    }
    let wrap = |v: isize, size: usize| {
        if v > size as isize / 2 {
            v - size as isize
        } else {
            v
        }
    };
    Some(Shift {
        dx: wrap(bx, width) as f32 + sx / sum,
        dy: wrap(by, height) as f32 + sy / sum,
        peak: sum.min(1.0),
    })
}

/// The image minus its mean, under a Hann window so the borders don't correlate; `None` when it's flat.
fn windowed(img: &[u8], width: usize, height: usize) -> Option<Vec<(f32, f32)>> {
    let mean = img.iter().map(|&v| v as f32).sum::<f32>() / img.len() as f32;
    let var = img.iter().map(|&v| (v as f32 - mean).powi(2)).sum::<f32>() / img.len() as f32;
    if var < 4.0 {
        return None;
    }
    let hann = |i: usize, size: usize| 0.5 - 0.5 * (2.0 * PI * i as f32 / size as f32).cos();
    let wx: Vec<f32> = (0..width).map(|x| hann(x, width)).collect();
    Some(
        img.iter()
            .enumerate()
            .map(|(i, &v)| {
                (
                    (v as f32 - mean) * wx[i % width] * hann(i / width, height),
                    0.0,
                )
            })
            .collect(),
    )
}

fn fft2(data: &mut [(f32, f32)], width: usize, height: usize, inverse: bool) {
    let mut col = vec![(0.0, 0.0); height];
    for row in data.chunks_exact_mut(width) {
        fft(row, inverse);
    }
    for x in 0..width {
        for y in 0..height {
            col[y] = data[y * width + x];
        }
        fft(&mut col, inverse);
        for y in 0..height {
            data[y * width + x] = col[y];
        }
    }
}

/// In-place radix-2 FFT (unscaled both ways).
fn fft(data: &mut [(f32, f32)], inverse: bool) {
    let n = data.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            data.swap(i, j);
        }
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let ang = sign * 2.0 * PI / len as f32;
        let (ws, wc) = ang.sin_cos();
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (ar, ai) = data[start + k];
                let (br, bi) = data[start + k + len / 2];
                let (tr, ti) = (br * cr - bi * ci, br * ci + bi * cr);
                data[start + k] = (ar + tr, ai + ti);
                data[start + k + len / 2] = (ar - tr, ai - ti);
                (cr, ci) = (cr * wc - ci * ws, cr * ws + ci * wc);
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 128;
    const H: usize = 128;

    /// A smooth random texture (the same for a seed), sampled at an offset with bilinear filtering.
    fn scene(seed: u32, dx: f32, dy: f32) -> Vec<u8> {
        let size = 64usize;
        let mut s = seed.wrapping_mul(2654435761).max(1);
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s % 256) as f32
        };
        let grid: Vec<f32> = (0..size * size).map(|_| rnd()).collect();
        let g = |x: i32, y: i32| {
            grid[(y.rem_euclid(size as i32) * size as i32 + x.rem_euclid(size as i32)) as usize]
        };
        let mut out = Vec::with_capacity(W * H);
        for y in 0..H {
            for x in 0..W {
                let fx = (x as f32 - dx) / 3.0;
                let fy = (y as f32 - dy) / 3.0;
                let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let top = g(x0, y0) * (1.0 - tx) + g(x0 + 1, y0) * tx;
                let bottom = g(x0, y0 + 1) * (1.0 - tx) + g(x0 + 1, y0 + 1) * tx;
                out.push((top * (1.0 - ty) + bottom * ty) as u8);
            }
        }
        out
    }

    #[test]
    fn finds_whole_and_fractional_shifts_both_ways() {
        for (dx, dy) in [
            (0.0, 0.0),
            (5.0, -3.0),
            (-12.0, 20.0),
            (2.5, 7.25),
            (-30.0, -9.5),
        ] {
            let s = estimate_shift(&scene(1, 0.0, 0.0), &scene(1, dx, dy), W, H).expect("a shift");
            assert!(
                (s.dx - dx).abs() < 0.35 && (s.dy - dy).abs() < 0.35,
                "{dx},{dy} -> {s:?}"
            );
        }
    }

    #[test]
    fn rejects_flat_and_unrelated_frames() {
        assert_eq!(
            estimate_shift(&[90; W * H], &scene(1, 0.0, 0.0), W, H),
            None
        );
        assert_eq!(
            estimate_shift(&scene(1, 0.0, 0.0), &scene(2, 0.0, 0.0), W, H),
            None
        );
    }

    #[test]
    fn rejects_mismatched_sizes() {
        assert_eq!(estimate_shift(&scene(1, 0.0, 0.0), &[0; 10], W, H), None);
        assert_eq!(estimate_shift(&[0; 12], &[0; 12], 4, 3), None);
    }
}
