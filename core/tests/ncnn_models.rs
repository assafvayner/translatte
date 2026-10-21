#![cfg(feature = "ncnn")]

mod common;

use common::store;
use translatte_core::ocr::backend::{ncnn_files, Infer, NcnnModel, OrtModel};
use translatte_core::ocr::detector::Detector;
use translatte_core::ocr::preprocess::{crop_quad, det_preprocess, rec_preprocess};
use translatte_core::ocr::recognizer::Recognizer;
use translatte_core::ocr::{OcrEngine, TextDetector, TextRecognizer};

fn image(name: &str) -> image::RgbImage {
    image::open(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .to_rgb8()
}

fn max_abs_diff(a: &ndarray::ArrayD<f32>, b: &ndarray::ArrayD<f32>) -> f32 {
    assert_eq!(a.shape(), b.shape());
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

fn read_text(
    store: &translatte_core::model_store::ModelStore,
    img: &image::RgbImage,
    engine: OcrEngine,
) -> String {
    let mut det =
        Detector::load(&store.model_file("det", "inference.onnx").unwrap(), engine).unwrap();
    let mut rec = Recognizer::load(
        &store.model_file("rec-latin", "inference.onnx").unwrap(),
        &store.model_file("rec-latin", "dict.txt").unwrap(),
        engine,
    )
    .unwrap();
    let quads = det.detect(img).unwrap();
    rec.recognize_all(img, &quads, &mut |_| {})
        .unwrap()
        .into_iter()
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join(" | ")
}

/// NCNN fp32 reproduces ONNX Runtime's detector map and recognizer logits, and reads the same
/// text; fp16 text is printed for comparison. Needs the NCNN files from
/// scripts/convert_ocr_ncnn.py.
#[test]
fn ncnn_fp32_matches_ort() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let det_onnx = store.model_file("det", "inference.onnx").unwrap();
    let rec_onnx = store.model_file("rec-latin", "inference.onnx").unwrap();
    let (det_param, det_bin) = ncnn_files(&det_onnx);
    let (rec_param, rec_bin) = ncnn_files(&rec_onnx);
    if !det_param.exists() || !rec_param.exists() {
        eprintln!("no NCNN models (run scripts/convert_ocr_ncnn.py); skipping");
        return;
    }
    let mut det_ort = OrtModel::load(&det_onnx, false).unwrap();
    let mut det_ncnn = NcnnModel::load(&det_param, &det_bin, false, 4).unwrap();
    let mut rec_ort = OrtModel::load(&rec_onnx, true).unwrap();
    let mut rec_ncnn = NcnnModel::load(&rec_param, &rec_bin, false, 3).unwrap();
    let mut det = Detector::load(&det_onnx, OcrEngine::Ort).unwrap();
    for name in ["hello_world.png", "bonjour.png"] {
        let img = image(name);
        let input = det_preprocess(&img, 960).tensor;
        let diff = max_abs_diff(
            &det_ort.run(input.view()).unwrap(),
            &det_ncnn.run(input.view()).unwrap(),
        );
        assert!(diff <= 1e-3, "{name}: detector map differs by {diff}");
        let quads = det.detect(&img).unwrap();
        assert!(!quads.is_empty(), "{name}: no text found");
        for q in &quads {
            let input = rec_preprocess(&crop_quad(&img, q));
            let diff = max_abs_diff(
                &rec_ort.run(input.view()).unwrap(),
                &rec_ncnn.run(input.view()).unwrap(),
            );
            assert!(diff <= 1e-3, "{name}: recognizer logits differ by {diff}");
        }
        let ort = read_text(&store, &img, OcrEngine::Ort);
        assert_eq!(read_text(&store, &img, OcrEngine::NcnnFp32), ort, "{name}");
        eprintln!(
            "{name}: ort {ort:?}, ncnn16 {:?}",
            read_text(&store, &img, OcrEngine::NcnnFp16)
        );
    }
}

#[cfg(feature = "pair")]
#[test]
fn switching_the_pipeline_engine_keeps_reading_the_same_text() {
    use translatte_core::loader::load_pipeline;
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let det_onnx = store.model_file("det", "inference.onnx").unwrap();
    if !ncnn_files(&det_onnx).0.exists() {
        eprintln!("no NCNN models (run scripts/convert_ocr_ncnn.py); skipping");
        return;
    }
    let fr = store.manifest().language("fr").unwrap();
    let img = image("bonjour.png");
    let mut p = load_pipeline(&store, fr, false, OcrEngine::Ort).unwrap();
    let text = |p: &mut translatte_core::loader::LoadedPipeline| -> Vec<String> {
        p.ocr(&img, fr, &mut |_| {})
            .unwrap()
            .into_iter()
            .map(|l| l.text)
            .collect()
    };
    let ort = text(&mut p);
    p.set_ocr_engine(&store, OcrEngine::NcnnFp32).unwrap();
    assert_eq!(p.ocr_engine(), OcrEngine::NcnnFp32);
    assert_eq!(text(&mut p), ort);
    p.set_ocr_engine(&store, OcrEngine::Ort).unwrap();
    assert_eq!(text(&mut p), ort);
}
