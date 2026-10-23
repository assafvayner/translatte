pub mod affinity;
pub mod backend;
pub mod charset;
pub mod ctc;
pub mod det_post;
pub mod detector;
pub mod preprocess;
pub mod recognizer;

use image::RgbImage;
use serde::Serialize;

pub use backend::Precision;

use crate::geometry::{Direction, Quad};
use crate::manifest::DETECTOR_ID;
use crate::model_store::ModelStore;
use crate::Result;
use backend::PARAM_FILE;
use detector::Detector;
use recognizer::Recognizer;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    pub quad: Quad,
    pub text: String,
    pub confidence: f32,
    pub direction: Direction,
}

pub trait TextDetector {
    fn detect(&mut self, img: &RgbImage) -> Result<Vec<Quad>>;
}

pub trait TextRecognizer {
    fn recognize(&mut self, img: &RgbImage, quad: &Quad) -> Result<Line>;

    /// Recognizes every quad, returning lines in the same order. `on_progress(done)` is called
    /// before each unit of work with the number of quads already recognized.
    fn recognize_all(
        &mut self,
        img: &RgbImage,
        quads: &[Quad],
        on_progress: &mut dyn FnMut(usize),
    ) -> Result<Vec<Line>> {
        quads
            .iter()
            .enumerate()
            .map(|(i, q)| {
                on_progress(i);
                self.recognize(img, q)
            })
            .collect()
    }
}

/// A positive integer from the environment variable `key`, else `default`; lets OCR settings be
/// tuned on a device without rebuilding.
pub(crate) fn env_or(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

/// The installed text detector, at `precision`.
pub fn load_detector(store: &ModelStore, precision: Precision) -> Result<Detector> {
    Detector::load(&store.model_file(DETECTOR_ID, PARAM_FILE)?, precision)
}

/// The installed recognizer of script pack `pack`, at `precision`.
pub fn load_recognizer(store: &ModelStore, pack: &str, precision: Precision) -> Result<Recognizer> {
    Recognizer::load(
        &store.model_file(pack, PARAM_FILE)?,
        &store.model_file(pack, "dict.txt")?,
        precision,
    )
}
