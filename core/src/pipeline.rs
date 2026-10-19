use image::RgbImage;
use serde::Serialize;

use crate::geometry::Quad;
use crate::layout::group_lines;
use crate::manifest::Language;
use crate::ocr::{Line, TextDetector, TextRecognizer};
use crate::render::style_hints;
use crate::translate::{chunk_ranges, Translator};
use crate::Result;

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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PipelineOutput {
    pub regions: Vec<TranslatedRegion>,
    pub translation_error: Option<String>,
}

pub struct Pipeline<D, R, T> {
    detector: D,
    recognizer: R,
    translator: Option<T>,
}

impl<D: TextDetector, R: TextRecognizer, T: Translator> Pipeline<D, R, T> {
    pub fn new(detector: D, recognizer: R, translator: Option<T>) -> Self {
        Self {
            detector,
            recognizer,
            translator,
        }
    }

    pub fn ocr(&mut self, img: &RgbImage, on_stage: &mut dyn FnMut(Stage)) -> Result<Vec<Line>> {
        on_stage(Stage::Detecting);
        let quads = self.detector.detect(img)?;
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
        Ok(lines)
    }

    pub fn run(
        &mut self,
        img: &RgbImage,
        src: &Language,
        tgt: &Language,
        on_stage: &mut dyn FnMut(Stage),
    ) -> Result<PipelineOutput> {
        let mut blocks = group_lines(self.ocr(img, on_stage)?, src);
        blocks.retain(|b| !b.text.trim().is_empty());
        let texts: Vec<String> = blocks.iter().map(|b| b.text.clone()).collect();
        let mut translation_error = None;
        let mut translations = Vec::with_capacity(texts.len());
        if let Some(t) = &mut self.translator {
            let ranges = chunk_ranges(&texts);
            for (i, range) in ranges.iter().enumerate() {
                on_stage(Stage::Translating {
                    done: i,
                    total: ranges.len(),
                });
                match t.translate(&texts[range.clone()], src, tgt) {
                    Ok(out) => translations.extend(out),
                    Err(e) => {
                        translation_error = Some(e.to_string());
                        break;
                    }
                }
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
            rtl: false,
            spaced: true,
            prompt_name: None,
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

    struct FailAfter(usize);
    impl Translator for FailAfter {
        fn translate(
            &mut self,
            blocks: &[String],
            _s: &Language,
            _t: &Language,
        ) -> Result<Vec<Option<String>>> {
            if self.0 == 0 {
                return Err(crate::TranslatteError::Translate("boom".into()));
            }
            self.0 -= 1;
            Ok(blocks.iter().map(|b| Some(b.to_uppercase())).collect())
        }
    }

    #[test]
    fn translation_error_keeps_ocr_results() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let long = "x".repeat(500);
        let long: &'static str = Box::leak(long.into_boxed_str());
        let mut p = Pipeline::new(
            FakeDet(vec![quad(10.0), quad(100.0), quad(200.0)]),
            FakeRec(vec![(long, 0.9), (long, 0.9), (long, 0.9)], 0),
            Some(FailAfter(1)),
        );
        let out = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap();
        assert_eq!(out.regions.len(), 3);
        assert!(out.regions[0].translated_text.is_some());
        assert!(out.regions[1].translated_text.is_none());
        assert!(out.regions[2].translated_text.is_none());
        assert!(out.translation_error.unwrap().contains("boom"));
    }
}
