mod common;

use common::store;
use translatte_core::ocr::{load_detector, load_recognizer};
use translatte_core::ocr::{Precision, TextDetector, TextRecognizer};

fn image(name: &str) -> image::RgbImage {
    image::open(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .to_rgb8()
}

fn read_text(name: &str, pack: &str, precision: Precision) -> Option<Vec<String>> {
    let store = store()?;
    let img = image(name);
    let mut det = load_detector(&store, precision).unwrap();
    let mut rec = load_recognizer(&store, pack, precision).unwrap();
    let quads = det.detect(&img).unwrap();
    let lines = rec.recognize_all(&img, &quads, &mut |_| {}).unwrap();
    Some(lines.into_iter().map(|l| l.text).collect())
}

#[test]
fn reads_the_test_images_at_both_precisions() {
    for precision in Precision::ALL {
        let Some(hello) = read_text("hello_world.png", "rec-latin", precision) else {
            eprintln!("TRANSLATTE_MODELS not set; skipping");
            return;
        };
        assert_eq!(hello, ["HELLO WORLD"], "{precision}");
        assert_eq!(
            read_text("bonjour.png", "rec-latin", precision).unwrap(),
            ["Bonjour le monde"],
            "{precision}"
        );
    }
}

#[test]
fn empty_image_yields_no_quads() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let mut det = load_detector(&store, Precision::Fp32).unwrap();
    assert!(det.detect(&image::RgbImage::new(0, 10)).unwrap().is_empty());
    assert!(det.detect(&image::RgbImage::new(10, 0)).unwrap().is_empty());
}
