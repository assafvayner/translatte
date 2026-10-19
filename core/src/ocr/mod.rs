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

/// Releases ONNX Runtime's process-global environment so its static destructors don't run
/// against a live env at exit (bionic aborts with "pthread_mutex_lock called on a destroyed mutex").
///
/// # Safety
/// Every ort session must already be dropped, and none may be created afterwards.
pub unsafe fn release_ort_environment() {
    use ort::AsPointer;
    if let Ok(env) = ort::environment::get_environment() {
        (ort::api().ReleaseEnv)(env.ptr().cast_mut());
    }
}
