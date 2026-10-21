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

/// Picks a tall crop's direction from its downward and upward readings. A lone glyph ("1", "l",
/// one CJK character) is tall without being a vertical line, so a reading of at most one glyph
/// falls back to horizontal.
fn pick_direction(down: &(String, f32), up: &(String, f32)) -> Direction {
    let (direction, (text, _)) = if up.1 > down.1 + DIRECTION_MARGIN {
        (Direction::BottomToTop, up)
    } else {
        (Direction::TopToBottom, down)
    };
    if text.chars().filter(|c| !c.is_whitespace()).count() <= 1 {
        Direction::Horizontal
    } else {
        direction
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
        let (direction, (text, confidence)) = if quad.is_tall() {
            let down = self.read(img, &quad.turned(Direction::TopToBottom))?;
            let up = self.read(img, &quad.turned(Direction::BottomToTop))?;
            match pick_direction(&down, &up) {
                Direction::Horizontal => (Direction::Horizontal, self.read(img, quad)?),
                Direction::TopToBottom => (Direction::TopToBottom, down),
                Direction::BottomToTop => (Direction::BottomToTop, up),
            }
        } else {
            (Direction::Horizontal, self.read(img, quad)?)
        };
        Ok(Line {
            quad: quad.turned(direction),
            text,
            confidence,
            direction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(text: &str, confidence: f32) -> (String, f32) {
        (text.into(), confidence)
    }

    #[test]
    fn top_to_bottom_wins_ties_and_small_margins() {
        let pick = |down, up| pick_direction(&r("本日", down), &r("PAIN", up));
        assert_eq!(pick(0.80, 0.80), Direction::TopToBottom);
        assert_eq!(
            pick(0.80, 0.80 + DIRECTION_MARGIN / 2.0),
            Direction::TopToBottom
        );
        assert_eq!(
            pick(0.60, 0.60 + DIRECTION_MARGIN * 2.0),
            Direction::BottomToTop
        );
        assert_eq!(pick(0.90, 0.40), Direction::TopToBottom);
    }

    #[test]
    fn single_glyph_readings_are_horizontal() {
        assert_eq!(
            pick_direction(&r("1", 0.99), &r("-", 0.5)),
            Direction::Horizontal
        );
        assert_eq!(
            pick_direction(&r("<", 0.54), &r(" X ", 0.61)),
            Direction::Horizontal
        );
        assert_eq!(
            pick_direction(&r("", 0.0), &r("", 0.0)),
            Direction::Horizontal
        );
        assert_eq!(
            pick_direction(&r("동", 0.98), &r("울", 0.49)),
            Direction::Horizontal
        );
    }

    #[test]
    fn the_winning_reading_decides_not_the_loser() {
        assert_eq!(
            pick_direction(&r("H", 0.26), &r("오늘", 0.9)),
            Direction::BottomToTop
        );
        assert_eq!(
            pick_direction(&r("お願い", 0.99), &r("x", 0.3)),
            Direction::TopToBottom
        );
        assert_eq!(
            pick_direction(&r("o", 0.9), &r("TE", 0.3)),
            Direction::Horizontal
        );
    }
}
