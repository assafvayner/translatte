mod common;

use common::store;
use translatte_core::geometry::Direction;
use translatte_core::manifest::Language;
use translatte_core::ocr::detector::Detector;
use translatte_core::ocr::recognizer::Recognizer;
use translatte_core::ocr::{Line, OcrEngine, TextDetector, TextRecognizer};
use translatte_core::pipeline::Pipeline;
use translatte_core::translate::Translator;
use translatte_core::Result;

/// The engines this build can run: ONNX Runtime, and NCNN with the `ncnn` feature.
fn engines() -> Vec<OcrEngine> {
    if cfg!(feature = "ncnn") {
        OcrEngine::ALL.to_vec()
    } else {
        vec![OcrEngine::Ort]
    }
}

fn read(image: &str, pack: &str, engine: OcrEngine) -> Option<Vec<Line>> {
    let store = store()?;
    let img = image::open(format!("{}/tests/data/{image}", env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .to_rgb8();
    let mut det =
        Detector::load(&store.model_file("det", "inference.onnx").unwrap(), engine).unwrap();
    let mut rec = Recognizer::load(
        &store.model_file(pack, "inference.onnx").unwrap(),
        &store.model_file(pack, "dict.txt").unwrap(),
        engine,
    )
    .unwrap();
    let quads = det.detect(&img).unwrap();
    Some(rec.recognize_all(&img, &quads, &mut |_| {}).unwrap())
}

fn assert_vertical(image: &str, pack: &str, want: &str, direction: Direction, angle: f32) {
    for engine in engines() {
        assert_vertical_on(image, pack, want, direction, angle, engine);
    }
}

fn assert_vertical_on(
    image: &str,
    pack: &str,
    want: &str,
    direction: Direction,
    angle: f32,
    engine: OcrEngine,
) {
    let Some(lines) = read(image, pack, engine) else {
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
        "{engine}: got {lines:?}"
    );
    for l in &lines {
        assert_eq!(l.direction, direction, "{engine}: {l:?}");
        assert!((l.quad.angle() - angle).abs() < 0.2, "{engine}: {l:?}");
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

#[test]
fn lone_tall_glyph_stays_horizontal() {
    for engine in engines() {
        let Some(lines) = read("lone_glyph.png", "rec-latin", engine) else {
            eprintln!("TRANSLATTE_MODELS not set; skipping");
            return;
        };
        assert_eq!(lines.len(), 1, "{engine}: {lines:?}");
        let l = &lines[0];
        assert!(l.quad.is_tall(), "{engine}: {l:?}");
        assert_eq!(l.text, "1", "{engine}: {l:?}");
        assert_eq!(l.direction, Direction::Horizontal, "{engine}: {l:?}");
        assert!(l.quad.angle().abs() < 0.2, "{engine}: {l:?}");
    }
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
    let img = image::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/vertical_ja.png"
    ))
    .unwrap()
    .to_rgb8();
    for engine in engines() {
        let mut pipeline: Pipeline<_, _, NoTranslator> = Pipeline::new(
            Detector::load(&store.model_file("det", "inference.onnx").unwrap(), engine).unwrap(),
            Recognizer::load(
                &store.model_file(&pack, "inference.onnx").unwrap(),
                &store.model_file(&pack, "dict.txt").unwrap(),
                engine,
            )
            .unwrap(),
            None,
        );
        let regions = pipeline.run(&img, &ja, &en, &mut |_| {}).unwrap().regions;
        assert_eq!(regions.len(), 1, "{engine}: {regions:?}");
        let r = &regions[0];
        // The recognizer drops the vertical long-vowel mark (ー) in rotated column crops.
        assert_eq!(
            r.source_text.replace('ー', ""),
            "本日のおすすめ醤油ラメン味噌ラメン",
            "{engine}: {r:?}"
        );
        assert!(
            (r.quad.angle() - std::f32::consts::FRAC_PI_2).abs() < 0.2,
            "{engine}: {r:?}"
        );
        assert!(
            (r.angle - std::f32::consts::FRAC_PI_2).abs() < 0.2,
            "{engine}: {r:?}"
        );
    }
}
