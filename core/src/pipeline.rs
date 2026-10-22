use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use image::RgbImage;
use serde::Serialize;

use crate::geometry::Quad;
use crate::layout::group_lines;
use crate::manifest::Language;
use crate::ocr::{Line, Precision, TextDetector, TextRecognizer};
use crate::render::style_hints;
use crate::translate::reuse::FrameReuse;
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
    /// The block's typical line height in image pixels: the median of its lines' short sides.
    pub line_height: f32,
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

/// Wall time of each pipeline stage in one run, and the precision OCR ran at, for the live
/// camera's debug readout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageTimings {
    pub detect_ms: u32,
    pub recognize_ms: u32,
    pub translate_ms: u32,
    pub precision: Precision,
    /// Blocks of a live frame that took an earlier frame's translation (see [`FrameReuse`]).
    pub reused: u32,
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

struct Ocr<D, R> {
    detector: D,
    recognizer: R,
    precision: Precision,
}

impl<D: TextDetector, R: TextRecognizer> Ocr<D, R> {
    fn read(
        &mut self,
        img: &RgbImage,
        on_stage: &mut dyn FnMut(Stage),
    ) -> Result<(Vec<Line>, StageTimings)> {
        on_stage(Stage::Detecting);
        let start = Instant::now();
        let quads = self.detector.detect(img)?;
        let detected = Instant::now();
        let total = quads.len();
        let mut lines = self.recognizer.recognize_all(img, &quads, &mut |done| {
            on_stage(Stage::Recognizing { done, total })
        })?;
        lines.retain(|l| l.confidence >= MIN_LINE_CONFIDENCE && !l.text.trim().is_empty());
        let timings = StageTimings {
            detect_ms: millis(detected - start),
            recognize_ms: millis(detected.elapsed()),
            precision: self.precision,
            ..StageTimings::default()
        };
        Ok((lines, timings))
    }
}

type LoadOcr<D, R> = Box<dyn Fn() -> Result<(D, R)> + Send + Sync>;

/// The live frames' OCR models, loaded by the first live frame.
struct LiveOcr<D, R> {
    load: LoadOcr<D, R>,
    precision: Precision,
    ocr: Mutex<Option<Ocr<D, R>>>,
}

struct Translation<T> {
    translator: T,
    reuse: FrameReuse,
}

/// OCR and translation sit behind separate locks, so one live frame can be translated while the
/// next is read; a stage still runs one image at a time. Live frames may have OCR models of
/// their own (see [`Self::with_live_ocr`]).
pub struct Pipeline<D, R, T> {
    ocr: Mutex<Ocr<D, R>>,
    live_ocr: Option<LiveOcr<D, R>>,
    live_on_photo_ocr: AtomicBool,
    translation: Option<Mutex<Translation<T>>>,
    script_pack: Option<String>,
    retired: AtomicBool,
}

/// A stage's lock. A panic mid-run leaves no state that the next run depends on.
fn lock<X>(m: &Mutex<X>) -> MutexGuard<'_, X> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl<D: TextDetector, R: TextRecognizer, T: Translator> Pipeline<D, R, T> {
    pub fn new(detector: D, recognizer: R, translator: Option<T>) -> Self {
        Self {
            ocr: Mutex::new(Ocr {
                detector,
                recognizer,
                precision: Precision::default(),
            }),
            live_ocr: None,
            live_on_photo_ocr: AtomicBool::new(false),
            translation: translator.map(|translator| {
                Mutex::new(Translation {
                    translator,
                    reuse: FrameReuse::default(),
                })
            }),
            script_pack: None,
            retired: AtomicBool::new(false),
        }
    }

    /// Restricts the pipeline to source languages using this script pack.
    pub fn with_script_pack(mut self, id: impl Into<String>) -> Self {
        self.script_pack = Some(id.into());
        self
    }

    /// Records the precision the detector and recognizer run at, for [`StageTimings`].
    pub fn with_precision(self, precision: Precision) -> Self {
        lock(&self.ocr).precision = precision;
        self
    }

    /// Reads live frames with the models `load` returns, which run at `precision`, instead of the
    /// photo ones. They're loaded by the first live frame, so a pipeline only used for photos never
    /// loads them.
    pub fn with_live_ocr(
        mut self,
        precision: Precision,
        load: impl Fn() -> Result<(D, R)> + Send + Sync + 'static,
    ) -> Self {
        self.live_ocr = Some(LiveOcr {
            load: Box::new(load),
            precision,
            ocr: Mutex::new(None),
        });
        self
    }

    /// Picks the precision live frames are read at: the photo models' precision reads them with
    /// the photo models (freeing the live ones), any other the live models'. Ignored when no live
    /// models were set up or neither runs at `precision`.
    pub fn set_live_precision(&self, precision: Precision) {
        let Some(live_ocr) = &self.live_ocr else {
            return;
        };
        if precision == lock(&self.ocr).precision {
            self.live_on_photo_ocr.store(true, Ordering::Relaxed);
            *lock(&live_ocr.ocr) = None;
        } else if precision == live_ocr.precision {
            self.live_on_photo_ocr.store(false, Ordering::Relaxed);
        }
    }

    /// Stops live frames from loading models, for a pipeline its owner is done with but that calls
    /// still in flight may use: from now on a live frame whose OCR models aren't loaded yet fails
    /// with [`TranslatteError::Unloaded`], and live frames are no longer translated (which could load
    /// a translation model). Photos are unaffected.
    pub fn retire(&self) {
        self.retired.store(true, Ordering::Relaxed);
    }

    fn retired(&self) -> bool {
        self.retired.load(Ordering::Relaxed)
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

    /// The lines OCR reads in `img`, with the live models when `live` is set, and its timings.
    pub fn ocr(
        &self,
        img: &RgbImage,
        src: &Language,
        live: bool,
        on_stage: &mut dyn FnMut(Stage),
    ) -> Result<(Vec<Line>, StageTimings)> {
        self.check_source(src)?;
        let Some(live_ocr) = self
            .live_ocr
            .as_ref()
            .filter(|_| live && !self.live_on_photo_ocr.load(Ordering::Relaxed))
        else {
            return lock(&self.ocr).read(img, on_stage);
        };
        let mut ocr = lock(&live_ocr.ocr);
        if ocr.is_none() {
            if self.retired() {
                return Err(TranslatteError::Unloaded);
            }
            let (detector, recognizer) = (live_ocr.load)()?;
            *ocr = Some(Ocr {
                detector,
                recognizer,
                precision: live_ocr.precision,
            });
        }
        ocr.as_mut().expect("loaded above").read(img, on_stage)
    }

    /// Prepares the translator for `src` to `tgt` (e.g. when the user picks the languages), so the
    /// next `run` spends less time translating. A no-op without a translator.
    pub fn warm(&self, src: &Language, tgt: &Language) -> Result<()> {
        match &self.translation {
            Some(t) => lock(t).translator.warm(src, tgt),
            None => Ok(()),
        }
    }

    pub fn run(
        &self,
        img: &RgbImage,
        src: &Language,
        tgt: &Language,
        on_stage: &mut dyn FnMut(Stage),
    ) -> Result<PipelineOutput> {
        let out = self.read(img, src, false, on_stage)?;
        Ok(self.translate(out, src, tgt, false, on_stage))
    }

    /// The untranslated regions of a live camera frame.
    pub fn read_frame(&self, img: &RgbImage, src: &Language) -> Result<PipelineOutput> {
        self.read(img, src, true, &mut |_| {})
    }

    /// Translates the regions of [`Self::read_frame`]. A block close to one of the last frames'
    /// in place and text takes that block's translation (see [`FrameReuse`]).
    pub fn translate_frame(
        &self,
        out: PipelineOutput,
        src: &Language,
        tgt: &Language,
    ) -> PipelineOutput {
        self.translate(out, src, tgt, true, &mut |_| {})
    }

    fn read(
        &self,
        img: &RgbImage,
        src: &Language,
        live: bool,
        on_stage: &mut dyn FnMut(Stage),
    ) -> Result<PipelineOutput> {
        let (lines, timings) = self.ocr(img, src, live, on_stage)?;
        let mut blocks = group_lines(lines, src);
        blocks.retain(|b| !b.text.trim().is_empty());
        let regions = blocks
            .into_iter()
            .map(|b| {
                let hints = style_hints(img, &b.quad);
                let line_height = line_height(&b.lines);
                TranslatedRegion {
                    quad: b.quad,
                    source_text: b.text,
                    translated_text: None,
                    bg_rgb: hints.bg,
                    fg_rgb: hints.fg,
                    angle: hints.angle,
                    line_height,
                }
            })
            .collect();
        Ok(PipelineOutput {
            regions,
            translation_error: None,
            timings,
        })
    }

    fn translate(
        &self,
        mut out: PipelineOutput,
        src: &Language,
        tgt: &Language,
        live: bool,
        on_stage: &mut dyn FnMut(Stage),
    ) -> PipelineOutput {
        let Some(translation) = &self.translation else {
            return out;
        };
        if live && self.retired() {
            return out;
        }
        let mut guard = lock(translation);
        let Translation { translator, reuse } = &mut *guard;
        let regions = &mut out.regions;
        let mut reused_from: Vec<Option<String>> = vec![None; regions.len()];
        if live {
            reuse.set_source(&src.code);
            for (r, from) in regions.iter_mut().zip(&mut reused_from) {
                if let Some(hit) = reuse.lookup(&r.quad, &r.source_text) {
                    r.translated_text = Some(hit.translation.to_string());
                    *from = Some(hit.source.to_string());
                }
            }
            out.timings.reused = regions
                .iter()
                .filter(|r| r.translated_text.is_some())
                .count() as u32;
        }
        let todo: Vec<usize> = (0..regions.len())
            .filter(|&i| regions[i].translated_text.is_none())
            .collect();
        if !todo.is_empty() {
            let texts: Vec<String> = todo
                .iter()
                .map(|&i| regions[i].source_text.clone())
                .collect();
            on_stage(Stage::Translating { done: 0, total: 1 });
            let start = Instant::now();
            match translator.translate(&texts, src, tgt) {
                Ok(translated) => {
                    for (&i, t) in todo.iter().zip(translated) {
                        regions[i].translated_text = t;
                    }
                }
                Err(e) => out.translation_error = Some(e.to_string()),
            }
            out.timings.translate_ms = millis(start.elapsed());
        }
        if live {
            reuse.push(regions.iter().zip(&reused_from).filter_map(|(r, from)| {
                Some((
                    &r.quad,
                    from.as_deref().unwrap_or(&r.source_text),
                    r.translated_text.as_deref()?,
                ))
            }));
        }
        out
    }
}

/// The median of the lines' short sides: the glyph size of horizontal and vertical lines alike.
fn line_height(lines: &[Line]) -> f32 {
    let mut sides: Vec<f32> = lines
        .iter()
        .map(|l| l.quad.width().min(l.quad.height()))
        .collect();
    sides.sort_by(f32::total_cmp);
    sides.get(sides.len() / 2).copied().unwrap_or(0.0)
}

#[cfg(test)]
impl<D, R, T> Pipeline<D, R, T> {
    fn translator(&self) -> MutexGuard<'_, Translation<T>> {
        lock(self.translation.as_ref().expect("a translator"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{BBox, Direction};
    use image::Rgb;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Barrier};

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
                direction: Direction::Horizontal,
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

    fn frame<D: TextDetector, R: TextRecognizer, T: Translator>(
        p: &Pipeline<D, R, T>,
        img: &RgbImage,
    ) -> PipelineOutput {
        let out = p.read_frame(img, &lang()).unwrap();
        p.translate_frame(out, &lang(), &lang())
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

    #[test]
    fn line_height_is_the_median_short_side() {
        let line = |x1: f32, y1: f32| Line {
            quad: Quad::from_bbox(BBox {
                x0: 0.0,
                y0: 0.0,
                x1,
                y1,
            }),
            text: "x".into(),
            confidence: 1.0,
            direction: Direction::Horizontal,
        };
        assert_eq!(line_height(&[]), 0.0);
        assert_eq!(
            line_height(&[line(200.0, 20.0), line(18.0, 300.0), line(90.0, 40.0)]),
            20.0
        );
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
    fn live_frames_use_the_live_ocr_models() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let p = Pipeline::new(
            FakeDet(vec![quad(10.0)]),
            FakeRec(vec![("sortie", 0.9); 3], 0),
            Some(Upper),
        );
        let out = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap();
        assert_eq!(out.timings.precision, Precision::Fp32);
        assert_eq!(frame(&p, &img).timings.precision, Precision::Fp32);
        let loads = Arc::new(AtomicUsize::new(0));
        let p = p.with_live_ocr(Precision::Fp16, {
            let loads = loads.clone();
            move || {
                loads.fetch_add(1, Ordering::Relaxed);
                Ok((
                    FakeDet(vec![quad(10.0)]),
                    FakeRec(vec![("entrée", 0.9); 2], 0),
                ))
            }
        });
        let out = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap();
        assert_eq!(out.timings.precision, Precision::Fp32);
        assert_eq!(
            loads.load(Ordering::Relaxed),
            0,
            "photos don't load the live models"
        );
        let out = frame(&p, &img);
        assert_eq!(out.timings.precision, Precision::Fp16);
        assert_eq!(out.regions[0].translated_text.as_deref(), Some("ENTRÉE"));
        frame(&p, &img);
        assert_eq!(loads.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn live_frames_can_use_the_photo_models() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let loads = Arc::new(AtomicUsize::new(0));
        let p = Pipeline::new(
            FakeDet(vec![quad(10.0)]),
            FakeRec(vec![("sortie", 0.9); 3], 0),
            Some(Upper),
        )
        .with_precision(Precision::Fp32)
        .with_live_ocr(Precision::Fp16, {
            let loads = loads.clone();
            move || {
                loads.fetch_add(1, Ordering::Relaxed);
                Ok((
                    FakeDet(vec![quad(10.0)]),
                    FakeRec(vec![("entrée", 0.9); 2], 0),
                ))
            }
        });
        assert_eq!(frame(&p, &img).timings.precision, Precision::Fp16);
        p.set_live_precision(Precision::Fp32);
        let out = frame(&p, &img);
        assert_eq!(out.timings.precision, Precision::Fp32);
        assert_eq!(out.regions[0].translated_text.as_deref(), Some("SORTIE"));
        p.set_live_precision(Precision::Fp16);
        assert_eq!(frame(&p, &img).timings.precision, Precision::Fp16);
        assert_eq!(
            loads.load(Ordering::Relaxed),
            2,
            "fp32 freed the live models"
        );
    }

    #[test]
    fn a_retired_pipeline_loads_nothing_for_live_frames() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let p = Pipeline::new(
            FakeDet(vec![quad(10.0)]),
            FakeRec(vec![("sortie", 0.9); 2], 0),
            Some(Recording::default()),
        )
        .with_live_ocr(Precision::Fp16, || panic!("loaded the live models"));
        p.retire();
        assert!(matches!(
            p.read_frame(&img, &lang()),
            Err(TranslatteError::Unloaded)
        ));
        let read = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap();
        assert_eq!(read.regions[0].translated_text.as_deref(), Some("SORTIE"));
        let mut read = read;
        read.regions[0].translated_text = None;
        let out = p.translate_frame(read, &lang(), &lang());
        assert_eq!(out.regions[0].translated_text, None);
        assert_eq!(p.translator().translator.0.len(), 1, "only the photo");
    }

    #[test]
    fn runs_all_stages_and_drops_low_confidence_lines() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let p = Pipeline::new(
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
        let p = Pipeline::new(
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
        let p: Pipeline<_, _, Upper> = Pipeline::new(
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
        let p: Pipeline<_, _, Upper> = Pipeline::new(
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
            p.ocr(&img, &lang(), false, &mut |_| {}),
            Err(TranslatteError::ScriptPackMismatch { .. })
        ));
        let mut latin = lang();
        latin.script_pack = Some("rec-latin".into());
        assert_eq!(p.ocr(&img, &latin, false, &mut |_| {}).unwrap().0.len(), 1);
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
        let p = Pipeline::new(
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
        let p = Pipeline::new(
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
        assert_eq!(p.translator().translator.0, 1);
    }

    #[derive(Default)]
    struct Recording(Vec<Vec<String>>);
    impl Translator for Recording {
        fn translate(
            &mut self,
            blocks: &[String],
            _s: &Language,
            _t: &Language,
        ) -> Result<Vec<Option<String>>> {
            self.0.push(blocks.to_vec());
            Ok(blocks.iter().map(|b| Some(b.to_uppercase())).collect())
        }
    }

    fn jittered_pipeline() -> Pipeline<FakeDet, FakeRec, Recording> {
        Pipeline::new(
            FakeDet(vec![quad(10.0), quad(200.0)]),
            FakeRec(
                vec![
                    ("bonne journée", 0.9),
                    ("sortie", 0.9),
                    ("bonne jouruée", 0.9),
                    ("entrée", 0.9),
                ],
                0,
            ),
            Some(Recording::default()),
        )
    }

    #[test]
    fn live_frames_reuse_the_translation_of_a_jittered_block() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let p = jittered_pipeline();
        frame(&p, &img);
        let out = frame(&p, &img);
        assert_eq!(
            p.translator().translator.0,
            vec![
                vec!["bonne journée".to_string(), "sortie".to_string()],
                vec!["entrée".to_string()]
            ]
        );
        assert_eq!(out.regions[0].source_text, "bonne jouruée");
        assert_eq!(
            out.regions[0].translated_text.as_deref(),
            Some("BONNE JOURNÉE")
        );
        assert_eq!(out.regions[1].translated_text.as_deref(), Some("ENTRÉE"));
        assert_eq!(out.timings.reused, 1);
    }

    #[test]
    fn reused_blocks_stay_matched_to_the_text_that_was_translated() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let drifting = ["aaaaaaaaaa", "aaaaaaaaab", "aaaaaaaabb", "aaaaaaabbb"];
        let p = Pipeline::new(
            FakeDet(vec![quad(10.0)]),
            FakeRec(drifting.iter().map(|&t| (t, 0.9)).collect(), 0),
            Some(Recording::default()),
        );
        for _ in drifting {
            frame(&p, &img);
        }
        assert_eq!(
            p.translator().translator.0,
            vec![
                vec!["aaaaaaaaaa".to_string()],
                vec!["aaaaaaabbb".to_string()]
            ]
        );
    }

    #[test]
    fn photos_always_translate() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let p = jittered_pipeline();
        frame(&p, &img);
        let out = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap();
        assert_eq!(p.translator().translator.0.len(), 2);
        assert_eq!(p.translator().translator.0[1].len(), 2);
        assert_eq!(
            out.regions[0].translated_text.as_deref(),
            Some("BONNE JOURUÉE")
        );
        assert_eq!(out.timings.reused, 0);
    }

    /// A detector that waits for the translator to be running.
    struct WaitingDet(Arc<Barrier>);
    impl TextDetector for WaitingDet {
        fn detect(&mut self, _img: &RgbImage) -> Result<Vec<Quad>> {
            self.0.wait();
            Ok(vec![quad(10.0)])
        }
    }

    /// A translator that waits for the detector to be running.
    struct WaitingTranslator(Arc<Barrier>);
    impl Translator for WaitingTranslator {
        fn translate(
            &mut self,
            blocks: &[String],
            _s: &Language,
            _t: &Language,
        ) -> Result<Vec<Option<String>>> {
            self.0.wait();
            Ok(blocks.iter().map(|b| Some(b.to_uppercase())).collect())
        }
    }

    #[test]
    fn a_frame_is_translated_while_the_next_is_read() {
        let img = RgbImage::from_pixel(200, 400, Rgb([255, 255, 255]));
        let barrier = Arc::new(Barrier::new(2));
        let p = Pipeline::new(
            WaitingDet(barrier.clone()),
            FakeRec(vec![("sortie", 0.9)], 0),
            Some(WaitingTranslator(barrier)),
        );
        let out = PipelineOutput {
            regions: vec![TranslatedRegion {
                quad: quad(10.0),
                source_text: "entrée".into(),
                translated_text: None,
                bg_rgb: [255; 3],
                fg_rgb: [0; 3],
                angle: 0.0,
                line_height: 0.0,
            }],
            translation_error: None,
            timings: StageTimings::default(),
        };
        let (read, translated) = std::thread::scope(|s| {
            let translated = s.spawn(|| p.translate_frame(out, &lang(), &lang()));
            let read = p.read_frame(&img, &lang()).unwrap();
            (read, translated.join().unwrap())
        });
        assert_eq!(read.regions[0].source_text, "sortie");
        assert_eq!(read.regions[0].translated_text, None);
        assert_eq!(
            translated.regions[0].translated_text.as_deref(),
            Some("ENTRÉE")
        );
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
        let p = Pipeline::new(
            SlowDet(vec![quad(10.0)]),
            FakeRec(vec![("sortie", 0.9)], 0),
            Some(SlowUpper),
        );
        let t = p.run(&img, &lang(), &lang(), &mut |_| {}).unwrap().timings;
        assert!(t.detect_ms >= 5, "{t:?}");
        assert!(t.translate_ms >= 5, "{t:?}");
    }
}
