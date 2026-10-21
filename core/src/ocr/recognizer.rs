use std::path::Path;

use image::RgbImage;
use ndarray::{Axis, Ix2};
use ort::session::Session;
use ort::value::Tensor;

use super::charset::Charset;
use super::ctc::ctc_decode;
use super::preprocess::{crop_quad, rec_preprocess};
use super::{Line, TextRecognizer};
use crate::geometry::{Direction, Quad};
use crate::{Result, TranslatteError};

pub struct Recognizer {
    session: Session,
    charset: Charset,
}

impl Recognizer {
    pub fn load(onnx: &Path, dict: &Path) -> Result<Self> {
        let session = super::session_builder(true)?.commit_from_file(onnx)?;
        let charset = Charset::from_dict_str(&std::fs::read_to_string(dict)?);
        Ok(Self { session, charset })
    }
}

/// How much more confident the upward reading must be to beat the downward one, which is
/// PaddleOCR's default for tall crops.
const DIRECTION_MARGIN: f32 = 0.05;

fn pick_direction(down: f32, up: f32) -> Direction {
    if up > down + DIRECTION_MARGIN {
        Direction::BottomToTop
    } else {
        Direction::TopToBottom
    }
}

impl Recognizer {
    fn read(&mut self, img: &RgbImage, quad: &Quad) -> Result<(String, f32)> {
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
        Ok(ctc_decode(probs, &self.charset))
    }
}

impl TextRecognizer for Recognizer {
    fn recognize(&mut self, img: &RgbImage, quad: &Quad) -> Result<Line> {
        if !quad.is_tall() {
            let (text, confidence) = self.read(img, quad)?;
            return Ok(Line {
                quad: *quad,
                text,
                confidence,
                direction: Direction::Horizontal,
            });
        }
        let down_quad = quad.turned(Direction::TopToBottom);
        let up_quad = quad.turned(Direction::BottomToTop);
        let down = self.read(img, &down_quad)?;
        let up = self.read(img, &up_quad)?;
        let direction = pick_direction(down.1, up.1);
        let ((text, confidence), quad) = match direction {
            Direction::BottomToTop => (up, up_quad),
            _ => (down, down_quad),
        };
        Ok(Line {
            quad,
            text,
            confidence,
            direction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_to_bottom_wins_ties_and_small_margins() {
        assert_eq!(pick_direction(0.80, 0.80), Direction::TopToBottom);
        assert_eq!(
            pick_direction(0.80, 0.80 + DIRECTION_MARGIN / 2.0),
            Direction::TopToBottom
        );
        assert_eq!(
            pick_direction(0.60, 0.60 + DIRECTION_MARGIN * 2.0),
            Direction::BottomToTop
        );
        assert_eq!(pick_direction(0.90, 0.40), Direction::TopToBottom);
    }
}
