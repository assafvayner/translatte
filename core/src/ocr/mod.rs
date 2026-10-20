pub mod charset;
pub mod ctc;
pub mod det_post;
pub mod detector;
pub mod preprocess;
pub mod recognizer;

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

/// ONNX Runtime session settings shared by the OCR models. The detector's activations (for a
/// 960px image) are large, and ORT's arena would keep them allocated while the translator runs,
/// so it gets plain allocations instead; the recognizer's small arena is kept.
fn session_builder(arena: bool) -> ort::Result<ort::session::builder::SessionBuilder> {
    let builder = ort::session::Session::builder()?.with_intra_threads(4)?;
    if arena {
        return Ok(builder);
    }
    builder
        .with_execution_providers([ort::execution_providers::CPUExecutionProvider::default()
            .with_arena_allocator(false)
            .build()])?
        .with_memory_pattern(false)
}
