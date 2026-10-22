use std::path::Path;

use image::RgbImage;
use ndarray::{Axis, Ix2};

use super::backend::{NcnnModel, Precision};
use super::det_post::{db_postprocess, DbParams};
use super::preprocess::det_preprocess;
use super::TextDetector;
use crate::geometry::Quad;
use crate::{Result, TranslatteError};

/// PP-OCRv5_mobile_det inference.yml uses resize_long: 960.
const RESIZE_LONG: u32 = 960;
/// Live frames detect at their own size up to this. On camera frames 1120 finds most of the extra
/// text 1280 does for half its added detection time, with fewer overlapping blocks.
const LIVE_MAX_LONG: u32 = 1120;
/// Matches PaddleOCR filter_tag_det_res.
const MIN_SIDE: f32 = 3.0;

pub struct Detector {
    model: NcnnModel,
    params: DbParams,
}

impl Detector {
    /// Loads the detector whose NCNN parameter file is `param`.
    pub fn load(param: &Path, precision: Precision) -> Result<Self> {
        let _pin = super::affinity::pin();
        Ok(Self {
            model: NcnnModel::load(param, precision, 3)?,
            params: DbParams::default(),
        })
    }
}

/// The long side a live frame of `img`'s size is detected at.
fn live_resize_long(img: &RgbImage) -> u32 {
    img.width()
        .max(img.height())
        .clamp(RESIZE_LONG, LIVE_MAX_LONG)
}

impl Detector {
    fn detect_at(&mut self, img: &RgbImage, resize_long: u32) -> Result<Vec<Quad>> {
        if img.width() == 0 || img.height() == 0 {
            return Ok(Vec::new());
        }
        let input = det_preprocess(img, resize_long);
        let _pin = super::affinity::pin();
        let out = self.model.run(input.tensor.view())?;
        let prob = out
            .index_axis_move(Axis(0), 0)
            .into_dimensionality::<Ix2>()
            .map_err(|e| {
                TranslatteError::Inference(format!("unexpected detector output shape: {e}"))
            })?;
        let (w, h) = (img.width() as f32, img.height() as f32);
        Ok(db_postprocess(prob.view(), &self.params)
            .into_iter()
            .map(|(q, _)| {
                q.scale(input.scale_x, input.scale_y)
                    .clamp(w - 1.0, h - 1.0)
            })
            .filter(|q| q.width() > MIN_SIDE && q.height() > MIN_SIDE)
            .collect())
    }
}

impl TextDetector for Detector {
    fn detect(&mut self, img: &RgbImage) -> Result<Vec<Quad>> {
        self.detect_at(img, RESIZE_LONG)
    }

    fn detect_frame(&mut self, img: &RgbImage) -> Result<Vec<Quad>> {
        self.detect_at(img, live_resize_long(img))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_frames_detect_at_their_own_size_within_bounds() {
        let size = |w, h| live_resize_long(&RgbImage::new(w, h));
        assert_eq!(size(720, 1280), 1120);
        assert_eq!(size(1080, 1920), 1120);
        assert_eq!(size(1088, 816), 1088);
        assert_eq!(size(1024, 768), 1024);
        assert_eq!(size(480, 640), RESIZE_LONG);
    }
}
