use std::path::Path;

use image::RgbImage;
use ndarray::{Axis, Ix3};

use super::backend::{load_model, Infer, OcrEngine};
use super::charset::Charset;
use super::ctc::ctc_decode;
use super::preprocess::{crop_quad, rec_input_width, rec_preprocess_batch};
use super::{Line, TextRecognizer};
use crate::geometry::Quad;
use crate::{Result, TranslatteError};

/// At most this many lines per inference call, as PaddleOCR's rec_batch_num. Overridable with
/// `TRANSLATTE_OCR_REC_BATCH`.
const BATCH: usize = 6;
/// A line joins a batch only if it is at most this much wider than the batch's narrowest line,
/// so little of the batch is padding.
const BATCH_SLACK: f32 = 1.25;

pub struct Recognizer {
    model: Box<dyn Infer>,
    charset: Charset,
}

impl Recognizer {
    /// Loads the recognizer whose ONNX file is `onnx` (the NCNN files sit next to it).
    pub fn load(onnx: &Path, dict: &Path, engine: OcrEngine) -> Result<Self> {
        let _pin = super::affinity::pin();
        let model = load_model(onnx, engine, true, 3)?;
        let charset = Charset::from_dict_str(&std::fs::read_to_string(dict)?);
        Ok(Self { model, charset })
    }

    fn run_batch(&mut self, crops: &[&RgbImage]) -> Result<Vec<(String, f32)>> {
        let tensor = rec_preprocess_batch(crops);
        let _pin = super::affinity::pin();
        let out = self
            .model
            .run(tensor.view())?
            .into_dimensionality::<Ix3>()
            .map_err(|e| {
                TranslatteError::Inference(format!("unexpected recognizer output shape: {e}"))
            })?;
        if out.dim().2 != self.charset.len() {
            return Err(TranslatteError::Inference(format!(
                "charset has {} classes but model outputs {}",
                self.charset.len(),
                out.dim().2
            )));
        }
        Ok(out
            .axis_iter(Axis(0))
            .map(|probs| ctc_decode(probs, &self.charset))
            .collect())
    }
}

impl TextRecognizer for Recognizer {
    fn recognize(&mut self, img: &RgbImage, quad: &Quad) -> Result<Line> {
        let mut lines = self.recognize_all(img, std::slice::from_ref(quad), &mut |_| {})?;
        Ok(lines.remove(0))
    }

    fn recognize_all(
        &mut self,
        img: &RgbImage,
        quads: &[Quad],
        on_progress: &mut dyn FnMut(usize),
    ) -> Result<Vec<Line>> {
        let crops: Vec<RgbImage> = quads.iter().map(|q| crop_quad(img, q)).collect();
        let mut order: Vec<usize> = (0..crops.len()).collect();
        order.sort_by_key(|&i| rec_input_width(&crops[i]));
        let mut lines: Vec<Option<Line>> = vec![None; quads.len()];
        let max_batch = if self.model.batches() {
            super::env_or("TRANSLATTE_OCR_REC_BATCH", BATCH)
        } else {
            1
        };
        let mut batches: Vec<Vec<usize>> = Vec::new();
        for &i in &order {
            let w = rec_input_width(&crops[i]) as f32;
            match batches.last_mut() {
                Some(b)
                    if b.len() < max_batch
                        && w <= rec_input_width(&crops[b[0]]) as f32 * BATCH_SLACK =>
                {
                    b.push(i)
                }
                _ => batches.push(vec![i]),
            }
        }
        let mut done = 0;
        for chunk in batches {
            on_progress(done);
            done += chunk.len();
            let batch: Vec<&RgbImage> = chunk.iter().map(|&i| &crops[i]).collect();
            for (&i, (text, confidence)) in chunk.iter().zip(self.run_batch(&batch)?) {
                lines[i] = Some(Line {
                    quad: quads[i],
                    text,
                    confidence,
                });
            }
        }
        Ok(lines.into_iter().flatten().collect())
    }
}
