mod common;

use common::store;
use translatte_core::geometry::Direction;
use translatte_core::ocr::detector::Detector;
use translatte_core::ocr::recognizer::Recognizer;
use translatte_core::ocr::{Line, TextDetector, TextRecognizer};

fn read(image: &str, pack: &str) -> Option<Vec<Line>> {
    let store = store()?;
    let img = image::open(format!("{}/tests/data/{image}", env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .to_rgb8();
    let mut det = Detector::load(&store.model_file("det", "inference.onnx").unwrap()).unwrap();
    let mut rec = Recognizer::load(
        &store.model_file(pack, "inference.onnx").unwrap(),
        &store.model_file(pack, "dict.txt").unwrap(),
    )
    .unwrap();
    let quads = det.detect(&img).unwrap();
    Some(
        quads
            .iter()
            .map(|q| rec.recognize(&img, q).unwrap())
            .collect(),
    )
}

fn assert_vertical(image: &str, pack: &str, want: &str, direction: Direction, angle: f32) {
    let Some(lines) = read(image, pack) else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let text: String = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.to_uppercase().replace(' ', "").contains(want),
        "got {lines:?}"
    );
    for l in &lines {
        assert_eq!(l.direction, direction, "{l:?}");
        assert!((l.quad.angle() - angle).abs() < 0.2, "{l:?}");
    }
}

#[test]
fn reads_latin_rotated_clockwise_top_to_bottom() {
    assert_vertical(
        "vertical_en_down.png",
        "rec-latin",
        "PAINFRAIS",
        Direction::TopToBottom,
        std::f32::consts::FRAC_PI_2,
    );
}

#[test]
fn reads_latin_rotated_counter_clockwise_bottom_to_top() {
    assert_vertical(
        "vertical_en_up.png",
        "rec-latin",
        "PAINFRAIS",
        Direction::BottomToTop,
        -std::f32::consts::FRAC_PI_2,
    );
}

#[test]
fn reads_japanese_columns_top_to_bottom() {
    assert_vertical(
        "vertical_ja.png",
        "rec-cjk",
        "本日のおすすめ",
        Direction::TopToBottom,
        std::f32::consts::FRAC_PI_2,
    );
}
