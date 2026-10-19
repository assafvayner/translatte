mod common;

use common::store;
use translatte_core::ocr::detector::Detector;
use translatte_core::ocr::recognizer::Recognizer;
use translatte_core::ocr::{TextDetector, TextRecognizer};

#[test]
fn detects_and_reads_hello_world() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let img = image::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/hello_world.png"
    ))
    .unwrap()
    .to_rgb8();
    let mut det = Detector::load(&store.model_file("det", "inference.onnx").unwrap()).unwrap();
    let mut rec = Recognizer::load(
        &store.model_file("rec-en", "inference.onnx").unwrap(),
        &store.model_file("rec-en", "dict.txt").unwrap(),
    )
    .unwrap();

    let quads = det.detect(&img).unwrap();
    assert!(!quads.is_empty());
    let text: String = quads
        .iter()
        .map(|q| rec.recognize(&img, q).unwrap().text)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.to_uppercase().contains("HELLO"), "got {text:?}");
    assert!(text.to_uppercase().contains("WORLD"), "got {text:?}");
}

#[test]
fn empty_image_yields_no_quads() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let mut det = Detector::load(&store.model_file("det", "inference.onnx").unwrap()).unwrap();
    assert!(det.detect(&image::RgbImage::new(0, 10)).unwrap().is_empty());
    assert!(det.detect(&image::RgbImage::new(10, 0)).unwrap().is_empty());
}
