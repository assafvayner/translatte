use std::path::Path;

use image::RgbImage;
use ndarray::{Axis, Ix2};
use ort::session::Session;
use ort::value::Tensor;

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
    session: Session,
    params: DbParams,
}

impl Detector {
    pub fn load(path: &Path) -> Result<Self> {
        let _pin = super::affinity::pin();
        let session = super::session_builder(false)?.commit_from_file(path)?;
        Ok(Self {
            session,
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
        let outputs = self
            .session
            .run(ort::inputs![Tensor::from_array(input.tensor)?])?;
        let out = outputs[0].try_extract_array::<f32>()?;
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
