use std::path::Path;

use image::RgbImage;
use ndarray::{Axis, Ix2};
use ort::session::Session;
use ort::value::Tensor;

use super::charset::Charset;
use super::ctc::ctc_decode;
use super::preprocess::{crop_quad, rec_preprocess};
use super::{Line, TextRecognizer};
use crate::geometry::Quad;
use crate::{Result, TranslatteError};

pub struct Recognizer {
    session: Session,
    charset: Charset,
}

impl Recognizer {
    pub fn load(onnx: &Path, dict: &Path) -> Result<Self> {
        let session = Session::builder()?
            .with_intra_threads(4)?
            .commit_from_file(onnx)?;
        let charset = Charset::from_dict_str(&std::fs::read_to_string(dict)?);
        Ok(Self { session, charset })
    }
}

impl TextRecognizer for Recognizer {
    fn recognize(&mut self, img: &RgbImage, quad: &Quad) -> Result<Line> {
        let tensor = rec_preprocess(&crop_quad(img, quad));
        let outputs = self
            .session
            .run(ort::inputs![Tensor::from_array(tensor)?])?;
        let out = outputs[0].try_extract_array::<f32>()?;
        let probs = out
            .index_axis(Axis(0), 0)
            .into_dimensionality::<Ix2>()
            .map_err(|e| {
                TranslatteError::Inference(format!("unexpected recognizer output shape: {e}"))
            })?;
        if probs.ncols() != self.charset.len() {
            return Err(TranslatteError::Inference(format!(
                "charset has {} classes but model outputs {}",
                self.charset.len(),
                probs.ncols()
            )));
        }
        let (text, confidence) = ctc_decode(probs, &self.charset);
        Ok(Line {
            quad: *quad,
            text,
            confidence,
        })
    }
}
