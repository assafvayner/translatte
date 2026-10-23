mod common;

use common::store;
use translatte_core::manifest::Language;
use translatte_core::ocr::{load_detector, load_recognizer, Precision};
use translatte_core::pipeline::{Pipeline, TranslatedRegion};
use translatte_core::translate::Translator;
use translatte_core::Result;

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

/// The regions read from `image` (see scripts/make_diagonal_fixtures.py) at each precision.
fn read(image: &str, code: &str) -> Option<Vec<(Precision, Vec<TranslatedRegion>)>> {
    let store = store()?;
    let src = store.manifest().language(code).unwrap().clone();
    let en = store.manifest().language("en").unwrap().clone();
    let pack = src.script_pack.clone().unwrap();
    let img = image::open(format!("{}/tests/data/{image}", env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .to_rgb8();
    Some(
        Precision::ALL
            .into_iter()
            .map(|precision| {
                let pipeline: Pipeline<_, _, NoTranslator> = Pipeline::new(
                    load_detector(&store, precision).unwrap(),
                    load_recognizer(&store, &pack, precision).unwrap(),
                    None,
                );
                let out = pipeline.run(&img, &src, &en, &mut |_| {}).unwrap();
                (precision, out.regions)
            })
            .collect(),
    )
}

fn assert_angle(r: &TranslatedRegion, degrees: f32, precision: Precision) {
    let want = degrees.to_radians();
    assert!((r.quad.angle() - want).abs() < 0.05, "{precision}: {r:?}");
    assert!((r.angle - want).abs() < 0.05, "{precision}: {r:?}");
}

#[test]
fn tilted_paragraph_reads_in_order_as_one_rotated_block() {
    let Some(runs) = read("diagonal_fr_30.png", "fr") else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    for (precision, regions) in runs {
        assert_eq!(regions.len(), 1, "{precision}: {regions:?}");
        let r = &regions[0];
        assert_eq!(
            r.source_text, "Ne pas stationner devant la porte du garage",
            "{precision}"
        );
        assert_angle(r, -30.0, precision);
    }
}

#[test]
fn tilted_japanese_lines_keep_their_order() {
    let Some(runs) = read("diagonal_ja_15.png", "ja") else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    for (precision, regions) in runs {
        assert_eq!(regions.len(), 1, "{precision}: {regions:?}");
        let r = &regions[0];
        assert_eq!(
            r.source_text, "ドアの前に駐車しないでください。ご協力をお願いいたします。",
            "{precision}"
        );
        assert_angle(r, -15.0, precision);
    }
}

#[test]
fn steep_and_level_text_form_separate_blocks() {
    let Some(runs) = read("diagonal_mixed.png", "fr") else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    for (precision, regions) in runs {
        let find = |text: &str| {
            regions
                .iter()
                .find(|r| r.source_text == text)
                .unwrap_or_else(|| panic!("{precision}: no {text:?} in {regions:?}"))
        };
        assert_angle(
            find("Ne pas stationner devant la porte du garage"),
            -60.0,
            precision,
        );
        assert_angle(find("Sortie de secours"), 0.0, precision);
        assert_eq!(regions.len(), 2, "{precision}: {regions:?}");
    }
}
