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
            model: NcnnModel::load(param, precision, 4)?,
            params: DbParams::default(),
        })
    }
}

impl TextDetector for Detector {
    fn detect(&mut self, img: &RgbImage) -> Result<Vec<Quad>> {
        self.detect_at(img, RESIZE_LONG)
    }

    fn detect_at(&mut self, img: &RgbImage, long_side: u32) -> Result<Vec<Quad>> {
        if img.width() == 0 || img.height() == 0 {
            return Ok(Vec::new());
        }
        let input = det_preprocess(img, long_side.min(RESIZE_LONG));
        let _pin = super::affinity::pin();
        let out = self.model.run(input.tensor.view())?;
        let prob = out
            .index_axis(Axis(0), 0)
            .index_axis_move(Axis(0), 0)
            .into_dimensionality::<Ix2>()
            .map_err(|e| {
                TranslatteError::Inference(format!("unexpected detector output shape: {e}"))
            })?;
        let (w, h) = (img.width() as f32, img.height() as f32);
        Ok(db_postprocess(prob, &self.params)
            .into_iter()
            .map(|(q, _)| {
                q.scale(input.scale_x, input.scale_y)
                    .clamp(w - 1.0, h - 1.0)
            })
            .filter(|q| q.width() > MIN_SIDE && q.height() > MIN_SIDE)
            .collect())
    }
}
