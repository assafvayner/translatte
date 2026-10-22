use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use translatte_core::loader::LoadedPipeline;
use translatte_core::ocr::Precision;

use crate::setup;

fn normalize(s: &str) -> Vec<char> {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Character error rate of `got` against `expected`, ignoring whitespace and case. Capped at 1.0.
pub fn cer(expected: &str, got: &str) -> f64 {
    let (e, g) = (normalize(expected), normalize(got));
    if e.is_empty() {
        return if g.is_empty() { 0.0 } else { 1.0 };
    }
    let dist = strsim::generic_levenshtein::<Vec<char>, Vec<char>, &char, &char>(&e, &g);
    (dist as f64 / e.len() as f64).min(1.0)
}

/// Mean over expected lines of the CER against the best-matching recognized line (order-independent).
pub fn fixture_cer(expected: &[String], got: &[String]) -> f64 {
    if expected.is_empty() {
        return 0.0;
    }
    let total: f64 = expected
        .iter()
        .map(|e| got.iter().map(|g| cer(e, g)).fold(1.0, f64::min))
        .sum();
    total / expected.len() as f64
}

pub fn run_eval(
    dir: &Path,
    manifest: &Path,
    models: &Path,
    max_cer: Option<f64>,
    precision: Precision,
) -> Result<bool> {
    let store = setup::load_store(manifest, models)?;
    let mut fixtures: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    fixtures.sort();
    let mut pipelines: HashMap<String, LoadedPipeline> = HashMap::new();
    let mut scores = Vec::new();
    let mut by_lang: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for json_path in fixtures {
        let text = std::fs::read_to_string(&json_path)
            .with_context(|| format!("reading {}", json_path.display()))?;
        let spec: Value = serde_json::from_str(&text)
            .with_context(|| format!("parsing {}", json_path.display()))?;
        let lang_code = spec["lang"].as_str().context("fixture missing \"lang\"")?;
        let image_name = spec["image"]
            .as_str()
            .context("fixture missing \"image\"")?;
        let expected = spec["lines"]
            .as_array()
            .context("fixture missing \"lines\"")?
            .iter()
            .map(|v| {
                v.as_str().map(str::to_string).with_context(|| {
                    format!("non-string entry in \"lines\" of {}", json_path.display())
                })
            })
            .collect::<Result<Vec<String>>>()?;
        let src = store.manifest().language(lang_code)?;
        let Some(key) = src.script_pack.clone() else {
            println!(
                "{:<40} {lang_code:<6} skipped: no OCR script pack",
                image_name
            );
            continue;
        };
        if !pipelines.contains_key(&key) {
            pipelines.insert(
                key.clone(),
                setup::build_pipeline(&store, src, false, precision)?,
            );
        }
        let pipeline = pipelines.get_mut(&key).expect("inserted above");
        let image_path = dir.join(image_name);
        let img = image::open(&image_path)
            .with_context(|| format!("opening {}", image_path.display()))?
            .to_rgb8();
        let got: Vec<String> = pipeline
            .ocr(&img, src, &mut |_| {})?
            .into_iter()
            .map(|l| l.text)
            .collect();
        let score = fixture_cer(&expected, &got);
        println!("{:<40} {lang_code:<6} CER {:.3}", image_name, score);
        scores.push(score);
        by_lang
            .entry(lang_code.to_string())
            .or_default()
            .push(score);
    }
    if scores.is_empty() {
        println!("0 fixtures found in {}", dir.display());
        return Ok(max_cer.is_none());
    }
    let mean = scores.iter().sum::<f64>() / scores.len() as f64;
    if by_lang.len() > 1 {
        for (lang, s) in &by_lang {
            let m = s.iter().sum::<f64>() / s.len() as f64;
            println!("  {lang:<6} {:>3} fixtures, mean CER {m:.3}", s.len());
        }
    }
    println!(
        "{} fixtures, mean CER {:.3} ({precision})",
        scores.len(),
        mean
    );
    Ok(max_cer.is_none_or(|m| mean <= m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cer_ignores_whitespace_and_case() {
        assert_eq!(cer("Hello World", "helloworld"), 0.0);
        assert!((cer("abcd", "abxd") - 0.25).abs() < 1e-6);
        assert_eq!(cer("", ""), 0.0);
    }

    #[test]
    fn line_cer_uses_best_matching_recognized_line() {
        let expected = vec!["EXIT".to_string(), "NO SMOKING".to_string()];
        let got = vec![
            "NO SMOKlNG".to_string(),
            "EXIT".to_string(),
            "noise".to_string(),
        ];
        let score = fixture_cer(&expected, &got);
        assert!((score - (0.0 + 1.0 / 9.0) / 2.0).abs() < 1e-6, "{score}");
    }

    #[test]
    fn missing_lines_score_one() {
        assert_eq!(fixture_cer(&["EXIT".to_string()], &[]), 1.0);
    }
}
