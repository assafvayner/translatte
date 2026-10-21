mod common;

use common::store;
use translatte_core::geometry::Direction;
use translatte_core::manifest::Language;
use translatte_core::ocr::detector::Detector;
use translatte_core::ocr::recognizer::Recognizer;
use translatte_core::ocr::{Line, TextDetector, TextRecognizer};
use translatte_core::pipeline::Pipeline;
use translatte_core::translate::Translator;
use translatte_core::Result;

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

struct NoTranslator;

impl Translator for NoTranslator {
    fn translate(
        &mut self,
        blocks: &[String],
        _src: &Language,
        _tgt: &Language,
    ) -> Result<Vec<Option<String>>> {
        Ok(vec![None; blocks.len()])
    }
}

#[test]
fn japanese_columns_become_one_block_in_reading_order() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let ja = store.manifest().language("ja").unwrap().clone();
    let en = store.manifest().language("en").unwrap().clone();
    let pack = ja.script_pack.clone().unwrap();
    let mut pipeline: Pipeline<_, _, NoTranslator> = Pipeline::new(
        Detector::load(&store.model_file("det", "inference.onnx").unwrap()).unwrap(),
        Recognizer::load(
            &store.model_file(&pack, "inference.onnx").unwrap(),
            &store.model_file(&pack, "dict.txt").unwrap(),
        )
        .unwrap(),
        None,
    );
    let img = image::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/vertical_ja.png"
    ))
    .unwrap()
    .to_rgb8();
    let regions = pipeline.run(&img, &ja, &en, &mut |_| {}).unwrap().regions;
    assert_eq!(regions.len(), 1, "{regions:?}");
    let r = &regions[0];
    // The recognizer drops the vertical long-vowel mark (ー) in rotated column crops.
    assert_eq!(
        r.source_text.replace('ー', ""),
        "本日のおすすめ醤油ラメン味噌ラメン",
        "{r:?}"
    );
    assert!(
        (r.quad.angle() - std::f32::consts::FRAC_PI_2).abs() < 0.2,
        "{r:?}"
    );
    assert!((r.angle - std::f32::consts::FRAC_PI_2).abs() < 0.2, "{r:?}");
}
