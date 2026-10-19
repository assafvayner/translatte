pub mod charset;
pub mod ctc;

use image::RgbImage;
use serde::Serialize;

use crate::geometry::Quad;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    pub quad: Quad,
    pub text: String,
    pub confidence: f32,
}

pub trait TextDetector {
    fn detect(&mut self, img: &RgbImage) -> Result<Vec<Quad>>;
}

pub trait TextRecognizer {
    fn recognize(&mut self, img: &RgbImage, quad: &Quad) -> Result<Line>;
}
