use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use image::imageops::FilterType;
use serde_json::{json, Value};

use translatte_core::loader::LoadedPipeline;
use translatte_core::ocr::Precision;
use translatte_core::pipeline::StageTimings;

use crate::{eval, setup};

pub struct BenchArgs {
    pub dir: PathBuf,
    pub iters: usize,
    pub max_side: Option<u32>,
    pub translate: bool,
    pub only: Vec<String>,
    pub text: Option<PathBuf>,
    /// Treat the fixtures, in name order, as consecutive camera frames: one live OCR run each, after
    /// one warm-up run per pipeline.
    pub live: bool,
    pub precision: Precision,
}

fn stats(name: &str, mut v: Vec<u32>) {
    if v.is_empty() {
        return;
    }
    v.sort_unstable();
    let pct = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    let mean = v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64;
    println!(
        "  {name:<10} mean {mean:>7.1}ms  p50 {:>5}ms  p90 {:>5}ms  max {:>5}ms",
        pct(0.5),
        pct(0.9),
        v[v.len() - 1]
    );
}

/// Times the pipeline stages over the fixtures in `dir` (see fixtures/README.md), `iters` times
/// each after one warm-up run (once each with `live`), and reports the OCR CER and recognized
/// lines.
pub fn run_bench(args: &BenchArgs, manifest: &Path, models: &Path) -> Result<()> {
    let store = setup::load_store(manifest, models)?;
    let tgt = store.target_language()?;
    let mut fixtures: Vec<_> = std::fs::read_dir(&args.dir)
        .with_context(|| format!("reading {}", args.dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    fixtures.sort();
    let mut pipelines: HashMap<String, LoadedPipeline> = HashMap::new();
    let mut samples: Vec<StageTimings> = Vec::new();
    let mut best: Vec<StageTimings> = Vec::new();
    let mut line_counts = Vec::new();
    let mut cers = Vec::new();
    let mut texts = BTreeMap::new();
    for json_path in fixtures {
        let spec: Value = serde_json::from_str(
            &std::fs::read_to_string(&json_path)
                .with_context(|| format!("reading {}", json_path.display()))?,
        )
        .with_context(|| format!("parsing {}", json_path.display()))?;
        let lang = spec["lang"].as_str().context("fixture missing \"lang\"")?;
        if !args.only.is_empty() && !args.only.iter().any(|l| l == lang) {
            continue;
        }
        let image_name = spec["image"]
            .as_str()
            .context("fixture missing \"image\"")?;
        let expected: Vec<String> = spec["lines"]
            .as_array()
            .context("fixture missing \"lines\"")?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        let src = store.manifest().language(lang)?;
        let Some(key) = src.script_pack.clone() else {
            continue;
        };
        let fresh = !pipelines.contains_key(&key);
        if fresh {
            pipelines.insert(
                key.clone(),
                setup::build_pipeline(&store, src, args.translate, args.precision)?,
            );
        }
        let pipeline = &pipelines[&key];
        if args.translate {
            pipeline.warm(src, tgt)?;
        }
        let image_path = args.dir.join(image_name);
        let mut img = image::open(&image_path)
            .with_context(|| format!("opening {}", image_path.display()))?
            .to_rgb8();
        if let Some(side) = args.max_side {
            let (w, h) = img.dimensions();
            if w.max(h) > side {
                let s = side as f32 / w.max(h) as f32;
                let (nw, nh) = ((w as f32 * s).round() as u32, (h as f32 * s).round() as u32);
                img = image::imageops::resize(&img, nw.max(1), nh.max(1), FilterType::Triangle);
            }
        }
        let mut rec = Vec::new();
        let mut min = StageTimings {
            detect_ms: u32::MAX,
            recognize_ms: u32::MAX,
            translate_ms: u32::MAX,
            precision: args.precision,
            reused: 0,
        };
        let lines = if args.live {
            if fresh {
                pipeline.ocr(&img, src, true, &mut |_| {})?;
            }
            let (lines, t) = pipeline.ocr(&img, src, true, &mut |_| {})?;
            rec.push(t.recognize_ms);
            min = t;
            samples.push(t);
            lines
        } else {
            pipeline.ocr(&img, src, false, &mut |_| {})?.0
        };
        for _ in 0..if args.live { 0 } else { args.iters } {
            let t = pipeline.run(&img, src, tgt, &mut |_| {})?.timings;
            rec.push(t.recognize_ms);
            min.detect_ms = min.detect_ms.min(t.detect_ms);
            min.recognize_ms = min.recognize_ms.min(t.recognize_ms);
            min.translate_ms = min.translate_ms.min(t.translate_ms);
            samples.push(t);
        }
        best.push(min);
        rec.sort_unstable();
        let got: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
        let cer = eval::fixture_cer(&expected, &got);
        println!(
            "{image_name:<44} {lang:<3} {:>4}x{:<4} {:>2} lines  rec p50 {:>4}ms  CER {cer:.3}",
            img.width(),
            img.height(),
            lines.len(),
            rec.get(rec.len() / 2).copied().unwrap_or(0),
        );
        line_counts.push(lines.len());
        cers.push(cer);
        texts.insert(
            image_name.to_string(),
            lines
                .iter()
                .map(|l| json!([l.text, (l.confidence * 1000.0).round() / 1000.0]))
                .collect::<Vec<_>>(),
        );
    }
    if samples.is_empty() {
        println!("0 fixtures found in {}", args.dir.display());
        return Ok(());
    }
    let iters = if args.live { 1 } else { args.iters };
    println!(
        "{} fixtures x {iters} iters ({}), {:.1} lines/image, mean CER {:.4}",
        cers.len(),
        args.precision,
        line_counts.iter().sum::<usize>() as f64 / line_counts.len() as f64,
        cers.iter().sum::<f64>() / cers.len() as f64
    );
    stats("detect", samples.iter().map(|t| t.detect_ms).collect());
    stats(
        "recognize",
        samples.iter().map(|t| t.recognize_ms).collect(),
    );
    if args.translate {
        stats(
            "translate",
            samples.iter().map(|t| t.translate_ms).collect(),
        );
    }
    stats(
        "total",
        samples
            .iter()
            .map(|t| t.detect_ms + t.recognize_ms + t.translate_ms)
            .collect(),
    );
    let mean = |f: fn(&StageTimings) -> u32| {
        best.iter().map(|t| f(t) as f64).sum::<f64>() / best.len() as f64
    };
    let per_image = if args.live {
        "per-frame".to_string()
    } else {
        format!("per-image best of {iters}")
    };
    println!(
        "  {per_image}: detect {:.1}ms  recognize {:.1}ms  translate {:.1}ms",
        mean(|t| t.detect_ms),
        mean(|t| t.recognize_ms),
        if args.translate {
            mean(|t| t.translate_ms)
        } else {
            0.0
        },
    );
    if let Some(p) = &args.text {
        std::fs::write(p, serde_json::to_string_pretty(&texts)?)
            .with_context(|| format!("writing {}", p.display()))?;
    }
    Ok(())
}
