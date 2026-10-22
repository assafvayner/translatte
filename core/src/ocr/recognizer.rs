use std::path::Path;

use image::RgbImage;
use ndarray::Ix2;

use super::backend::{NcnnModel, Precision};
use super::charset::Charset;
use super::ctc::ctc_decode;
use super::preprocess::{crop_quad, rec_preprocess};
use super::{Line, TextRecognizer};
use crate::geometry::{Direction, Quad};
use crate::{Result, TranslatteError};

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

pub struct Recognizer {
    model: NcnnModel,
    charset: Charset,
}

impl Recognizer {
    /// Loads the recognizer whose NCNN parameter file is `param`, reading characters from `dict`.
    pub fn load(param: &Path, dict: &Path, precision: Precision) -> Result<Self> {
        let _pin = super::affinity::pin();
        let model = NcnnModel::load(param, precision, 2)?;
        let charset = Charset::from_dict_str(&std::fs::read_to_string(dict)?);
        Ok(Self { model, charset })
    }

    fn read(&mut self, crop: &RgbImage) -> Result<(String, f32)> {
        let tensor = rec_preprocess(crop);
        let _pin = super::affinity::pin();
        let out = self
            .model
            .run(tensor.view())?
            .into_dimensionality::<Ix2>()
            .map_err(|e| {
                TranslatteError::Inference(format!("unexpected recognizer output shape: {e}"))
            })?;
        if out.dim().1 != self.charset.len() {
            return Err(TranslatteError::Inference(format!(
                "charset has {} classes but model outputs {}",
                self.charset.len(),
                out.dim().1
            )));
        }
        Ok(ctc_decode(out.view(), &self.charset))
    }

    /// Reads each crop; `on_progress(done)` gets the number of crops read before each one.
    fn read_all(
        &mut self,
        crops: &[RgbImage],
        on_progress: &mut dyn FnMut(usize),
    ) -> Result<Vec<(String, f32)>> {
        crops
            .iter()
            .enumerate()
            .map(|(i, crop)| {
                on_progress(i);
                self.read(crop)
            })
            .collect()
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
        let mut crops = Vec::with_capacity(quads.len());
        for q in quads {
            if q.is_tall() {
                crops.push(crop_quad(img, &q.turned(Direction::TopToBottom)));
                crops.push(crop_quad(img, &q.turned(Direction::BottomToTop)));
            } else {
                crops.push(crop_quad(img, q));
            }
        }
        let total = crops.len().max(1);
        let mut reads = self
            .read_all(&crops, &mut |done| on_progress(done * quads.len() / total))?
            .into_iter();
        let mut lines = Vec::with_capacity(quads.len());
        let mut horizontal = Vec::new();
        for (i, q) in quads.iter().enumerate() {
            let first = reads.next().expect("one reading per crop");
            let (direction, (text, confidence)) = if q.is_tall() {
                let up = reads.next().expect("two readings per tall crop");
                match pick_direction(&first, &up) {
                    Direction::Horizontal => {
                        horizontal.push(i);
                        (Direction::Horizontal, first)
                    }
                    Direction::TopToBottom => (Direction::TopToBottom, first),
                    Direction::BottomToTop => (Direction::BottomToTop, up),
                }
            } else {
                (Direction::Horizontal, first)
            };
            lines.push(Line {
                quad: q.turned(direction),
                text,
                confidence,
                direction,
            });
        }
        if !horizontal.is_empty() {
            let crops: Vec<RgbImage> = horizontal
                .iter()
                .map(|&i| crop_quad(img, &quads[i]))
                .collect();
            for (&i, (text, confidence)) in
                horizontal.iter().zip(self.read_all(&crops, &mut |_| {})?)
            {
                lines[i].text = text;
                lines[i].confidence = confidence;
            }
        }
        Ok(lines)
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
