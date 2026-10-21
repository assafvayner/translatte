use std::time::{Duration, Instant};

use image::RgbImage;
use serde::Serialize;

use crate::geometry::Quad;
use crate::layout::group_lines;
use crate::manifest::Language;
use crate::ocr::{Line, TextDetector, TextRecognizer};
use crate::render::style_hints;
use crate::translate::Translator;
use crate::{Result, TranslatteError};

const MIN_LINE_CONFIDENCE: f32 = 0.5;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TranslatedRegion {
    pub quad: Quad,
    pub source_text: String,
    pub translated_text: Option<String>,
    pub bg_rgb: [u8; 3],
    pub fg_rgb: [u8; 3],
    pub angle: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    Detecting,
    Recognizing { done: usize, total: usize },
    Translating { done: usize, total: usize },
}

impl Stage {
    pub fn name(&self) -> &'static str {
        match self {
            Stage::Detecting => "Detecting",
            Stage::Recognizing { .. } => "Recognizing",
            Stage::Translating { .. } => "Translating",
        }
    }
}

/// Wall time of each pipeline stage in one run, for the live camera's debug readout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageTimings {
    pub detect_ms: u32,
    pub recognize_ms: u32,
    pub translate_ms: u32,
}

fn millis(d: Duration) -> u32 {
    d.as_millis().min(u32::MAX as u128) as u32
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PipelineOutput {
    pub regions: Vec<TranslatedRegion>,
    pub translation_error: Option<String>,
    #[serde(skip)]
    pub timings: StageTimings,
}

pub struct Pipeline<D, R, T> {
    detector: D,
    recognizer: R,
    translator: Option<T>,
    script_pack: Option<String>,
}

impl<D: TextDetector, R: TextRecognizer, T: Translator> Pipeline<D, R, T> {
    pub fn new(detector: D, recognizer: R, translator: Option<T>) -> Self {
        Self {
            detector,
            recognizer,
            translator,
            script_pack: None,
        }
    }

    /// Restricts the pipeline to source languages using this script pack.
    pub fn with_script_pack(mut self, id: impl Into<String>) -> Self {
        self.script_pack = Some(id.into());
        self
    }

    fn check_source(&self, src: &Language) -> Result<()> {
        match (&self.script_pack, &src.script_pack) {
            (Some(own), got) if got.as_ref() != Some(own) => {
                Err(TranslatteError::ScriptPackMismatch {
                    pipeline: own.clone(),
                    language: src.code.clone(),
                })
            }
            _ => Ok(()),
        }
    }

    pub fn ocr(
        &mut self,
        img: &RgbImage,
        src: &Language,
        on_stage: &mut dyn FnMut(Stage),
    ) -> Result<Vec<Line>> {
        Ok(self.timed_ocr(img, src, on_stage)?.0)
    }

    fn timed_ocr(
        &mut self,
        img: &RgbImage,
        src: &Language,
        on_stage: &mut dyn FnMut(Stage),
    ) -> Result<(Vec<Line>, StageTimings)> {
        self.check_source(src)?;
        on_stage(Stage::Detecting);
        let start = Instant::now();
        let quads = self.detector.detect(img)?;
        let detected = Instant::now();
        let mut lines = Vec::new();
        for (i, q) in quads.iter().enumerate() {
            on_stage(Stage::Recognizing {
                done: i,
                total: quads.len(),
            });
            let line = self.recognizer.recognize(img, q)?;
            if line.confidence >= MIN_LINE_CONFIDENCE && !line.text.trim().is_empty() {
                lines.push(line);
            }
        }
        let timings = StageTimings {
            detect_ms: millis(detected - start),
            recognize_ms: millis(detected.elapsed()),
            translate_ms: 0,
        };
        Ok((lines, timings))
    }

    /// Prepares the translator for `src` to `tgt` (e.g. when the user picks the languages), so the
    /// next `run` spends less time translating. A no-op without a translator.
    pub fn warm(&mut self, src: &Language, tgt: &Language) -> Result<()> {
        match &mut self.translator {
            Some(t) => t.warm(src, tgt),
            None => Ok(()),
        }
    }

    pub fn run(
        &mut self,
        img: &RgbImage,
        src: &Language,
        tgt: &Language,
        on_stage: &mut dyn FnMut(Stage),
    ) -> Result<PipelineOutput> {
        let (lines, mut timings) = self.timed_ocr(img, src, on_stage)?;
        let mut blocks = group_lines(lines, src);
        blocks.retain(|b| !b.text.trim().is_empty());
        let texts: Vec<String> = blocks.iter().map(|b| b.text.clone()).collect();
        let mut translation_error = None;
        let mut translations = Vec::with_capacity(texts.len());
        if let Some(t) = &mut self.translator {
            if !texts.is_empty() {
                on_stage(Stage::Translating { done: 0, total: 1 });
                let start = Instant::now();
                match t.translate(&texts, src, tgt) {
                    Ok(out) => translations = out,
                    Err(e) => translation_error = Some(e.to_string()),
                }
                timings.translate_ms = millis(start.elapsed());
            }
        }
        translations.resize(texts.len(), None);
        let regions = blocks
            .into_iter()
            .zip(translations)
            .map(|(b, translated_text)| {
                let hints = style_hints(img, &b.quad);
                TranslatedRegion {
                    quad: b.quad,
                    source_text: b.text,
                    translated_text,
                    bg_rgb: hints.bg,
                    fg_rgb: hints.fg,
                    angle: hints.angle,
                }
            })
            .collect();
        Ok(PipelineOutput {
            regions,
            translation_error,
            timings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::BBox;
    use image::Rgb;

    struct FakeDet(Vec<Quad>);
    impl TextDetector for FakeDet {
        fn detect(&mut self, _img: &RgbImage) -> Result<Vec<Quad>> {
            Ok(self.0.clone())
        }
    }

    struct FakeRec(Vec<(&'static str, f32)>, usize);
    impl TextRecognizer for FakeRec {
        fn recognize(&mut self, _img: &RgbImage, quad: &Quad) -> Result<Line> {
            let (text, confidence) = self.0[self.1];
            self.1 += 1;
            Ok(Line {
                quad: *quad,
                text: text.into(),
                confidence,
            })
        }
    }

    struct Upper;
    impl Translator for Upper {
        fn translate(
            &mut self,
            blocks: &[String],
            _s: &Language,
            _t: &Language,
        ) -> Result<Vec<Option<String>>> {
            Ok(blocks.iter().map(|b| Some(b.to_uppercase())).collect())
        }
    }

    fn lang() -> Language {
        Language {
            code: "fr".into(),
            name: "French".into(),
            script_pack: None,
            pair_model: None,
            rtl: false,
            spaced: true,
        }
    }

    fn quad(y: f32) -> Quad {
        Quad::from_bbox(BBox {
            x0: 10.0,
            y0: y,
            x1: 100.0,
            y1: y + 20.0,
        })
    }

    #[test]
    fn runs_all_stages_and_drops_low_confidence_lines() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let mut p = Pipeline::new(
            FakeDet(vec![quad(10.0), quad(200.0), quad(300.0)]),
            FakeRec(vec![("sortie", 0.9), ("bruit", 0.2), ("entrée", 0.95)], 0),
            Some(Upper),
        );
        let mut stages = Vec::new();
        let out = p
            .run(&img, &lang(), &lang(), &mut |s| stages.push(s))
            .unwrap()
            .regions;
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source_text, "sortie");
        assert_eq!(out[0].translated_text.as_deref(), Some("SORTIE"));
        assert_eq!(out[1].translated_text.as_deref(), Some("ENTRÉE"));
        assert_eq!(out[0].bg_rgb, [255, 255, 255]);
        assert_eq!(
            stages,
            vec![
                Stage::Detecting,
                Stage::Recognizing { done: 0, total: 3 },
                Stage::Recognizing { done: 1, total: 3 },
                Stage::Recognizing { done: 2, total: 3 },
                Stage::Translating { done: 0, total: 1 },
            ]
        );
    }

    #[test]
    fn blank_blocks_are_dropped_before_translation() {
        let img = RgbImage::from_pixel(200, 100, Rgb([255, 255, 255]));
        let mut p = Pipeline::new(
            FakeDet(vec![quad(10.0), quad(50.0)]),
            FakeRec(vec![("   ", 0.9), ("sortie", 0.9)], 0),
            Some(Upper),
        );
        let out = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap().regions;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source_text, "sortie");
    }

    #[test]
    fn without_translator_regions_are_untranslated() {
        let img = RgbImage::from_pixel(200, 100, Rgb([255, 255, 255]));
        let mut p: Pipeline<_, _, Upper> = Pipeline::new(
            FakeDet(vec![quad(10.0)]),
            FakeRec(vec![("sortie", 0.9)], 0),
            None,
        );
        let out = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap().regions;
        assert_eq!(out[0].translated_text, None);
    }

    #[test]
    fn rejects_source_language_from_another_script_pack() {
        let img = RgbImage::from_pixel(200, 100, Rgb([255, 255, 255]));
        let mut p: Pipeline<_, _, Upper> = Pipeline::new(
            FakeDet(vec![quad(10.0)]),
            FakeRec(vec![("sortie", 0.9)], 0),
            None,
        )
        .with_script_pack("rec-latin");
        let mut other = lang();
        other.script_pack = Some("rec-cjk".into());
        assert!(matches!(
            p.run(&img, &other, &lang(), &mut |_| {}),
            Err(TranslatteError::ScriptPackMismatch { .. })
        ));
        assert!(matches!(
            p.ocr(&img, &lang(), &mut |_| {}),
            Err(TranslatteError::ScriptPackMismatch { .. })
        ));
        let mut latin = lang();
        latin.script_pack = Some("rec-latin".into());
        assert_eq!(p.ocr(&img, &latin, &mut |_| {}).unwrap().len(), 1);
    }

    struct Failing;
    impl Translator for Failing {
        fn translate(
            &mut self,
            _blocks: &[String],
            _s: &Language,
            _t: &Language,
        ) -> Result<Vec<Option<String>>> {
            Err(crate::TranslatteError::Translate("boom".into()))
        }
    }

    #[test]
    fn translation_error_keeps_ocr_results() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let mut p = Pipeline::new(
            FakeDet(vec![quad(10.0), quad(200.0)]),
            FakeRec(vec![("sortie", 0.9), ("entrée", 0.9)], 0),
            Some(Failing),
        );
        let out = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap();
        assert_eq!(out.regions.len(), 2);
        assert!(out.regions.iter().all(|r| r.translated_text.is_none()));
        assert!(out.translation_error.unwrap().contains("boom"));
    }

    struct Counting(usize);
    impl Translator for Counting {
        fn translate(
            &mut self,
            blocks: &[String],
            _s: &Language,
            _t: &Language,
        ) -> Result<Vec<Option<String>>> {
            self.0 += 1;
            Ok(blocks.iter().map(|b| Some(b.clone())).collect())
        }
    }

    #[test]
    fn all_blocks_are_translated_in_one_call() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let mut p = Pipeline::new(
            FakeDet(vec![quad(10.0), quad(150.0), quad(300.0)]),
            FakeRec(vec![("a", 0.9), ("b", 0.9), ("c", 0.9)], 0),
            Some(Counting(0)),
        );
        assert_eq!(
            p.run(&img, &lang(), &lang(), &mut |_| {})
                .unwrap()
                .regions
                .len(),
            3
        );
        assert_eq!(p.translator.as_ref().unwrap().0, 1);
    }

    struct SlowDet(Vec<Quad>);
    impl TextDetector for SlowDet {
        fn detect(&mut self, _img: &RgbImage) -> Result<Vec<Quad>> {
            std::thread::sleep(std::time::Duration::from_millis(5));
            Ok(self.0.clone())
        }
    }

    struct SlowUpper;
    impl Translator for SlowUpper {
        fn translate(
            &mut self,
            blocks: &[String],
            _s: &Language,
            _t: &Language,
        ) -> Result<Vec<Option<String>>> {
            std::thread::sleep(std::time::Duration::from_millis(5));
            Ok(blocks.iter().map(|b| Some(b.to_uppercase())).collect())
        }
    }

    #[test]
    fn reports_how_long_each_stage_took() {
        let img = RgbImage::from_pixel(200, 100, Rgb([255, 255, 255]));
        let mut p = Pipeline::new(
            SlowDet(vec![quad(10.0)]),
            FakeRec(vec![("sortie", 0.9)], 0),
            Some(SlowUpper),
        );
        let t = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap().timings;
        assert!(t.detect_ms >= 5, "{t:?}");
        assert!(t.translate_ms >= 5, "{t:?}");
    }
}
