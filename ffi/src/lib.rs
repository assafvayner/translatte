use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use translatte_core::loader::{load_pipeline, LoadedPipeline};
use translatte_core::manifest::{Language, Manifest};
use translatte_core::model_store::ModelStore;
use translatte_core::pipeline::{PipelineOutput, Stage};
use translatte_core::TranslatteError;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("{message}")]
    Failed { message: String },
}

impl From<TranslatteError> for FfiError {
    fn from(e: TranslatteError) -> Self {
        Self::Failed {
            message: e.to_string(),
        }
    }
}

impl From<image::ImageError> for FfiError {
    fn from(e: image::ImageError) -> Self {
        Self::Failed {
            message: format!("could not decode image: {e}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LanguageInfo {
    pub code: String,
    pub name: String,
    pub rtl: bool,
}

impl From<&Language> for LanguageInfo {
    fn from(l: &Language) -> Self {
        Self {
            code: l.code.clone(),
            name: l.name.clone(),
            rtl: l.rtl,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<[u8; 3]> for Rgb {
    fn from([r, g, b]: [u8; 3]) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Region {
    /// Top-left, top-right, bottom-right, bottom-left, in image pixels.
    pub corners: Vec<Point>,
    pub source_text: String,
    pub translated_text: Option<String>,
    pub background: Rgb,
    pub foreground: Rgb,
    pub angle: f32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TranslationResult {
    pub image_width: u32,
    pub image_height: u32,
    pub regions: Vec<Region>,
    pub translation_error: Option<String>,
}

impl TranslationResult {
    fn from_output(out: PipelineOutput, width: u32, height: u32) -> Self {
        let regions = out
            .regions
            .into_iter()
            .map(|r| Region {
                corners: r.quad.0.iter().map(|p| Point { x: p.x, y: p.y }).collect(),
                source_text: r.source_text,
                translated_text: r.translated_text,
                background: r.bg_rgb.into(),
                foreground: r.fg_rgb.into(),
                angle: r.angle,
            })
            .collect();
        Self {
            image_width: width,
            image_height: height,
            regions,
            translation_error: out.translation_error,
        }
    }
}

#[uniffi::export(with_foreign)]
pub trait ProgressListener: Send + Sync {
    fn on_stage(&self, stage: String, done: u32, total: u32);
}

fn report(listener: &Option<Arc<dyn ProgressListener>>, stage: Stage) {
    let Some(l) = listener else { return };
    let (done, total) = match stage {
        Stage::Detecting => (0, 1),
        Stage::Recognizing { done, total } | Stage::Translating { done, total } => {
            (done as u32, total as u32)
        }
    };
    l.on_stage(stage.name().to_string(), done, total);
}

struct Cached {
    script_pack: String,
    translate: bool,
    pipeline: LoadedPipeline,
}

/// Entry point for native apps. Pipelines are cached per script pack, so switching between
/// languages that share a pack (e.g. French and German) doesn't reload models.
#[derive(uniffi::Object)]
pub struct Engine {
    store: ModelStore,
    cached: Mutex<Option<Cached>>,
}

#[uniffi::export]
impl Engine {
    #[uniffi::constructor]
    pub fn new(models_dir: String, manifest_json: String) -> Result<Arc<Self>, FfiError> {
        let manifest = Manifest::from_json(&manifest_json)?;
        Ok(Arc::new(Self {
            store: ModelStore::new(manifest, PathBuf::from(models_dir)),
            cached: Mutex::new(None),
        }))
    }

    pub fn core_installed(&self) -> Result<bool, FfiError> {
        Ok(self.store.core_installed()?)
    }

    pub fn source_languages(&self) -> Result<Vec<LanguageInfo>, FfiError> {
        Ok(self
            .store
            .available_source_languages()?
            .into_iter()
            .map(LanguageInfo::from)
            .collect())
    }

    pub fn target_languages(&self) -> Vec<LanguageInfo> {
        self.store
            .target_languages()
            .into_iter()
            .map(LanguageInfo::from)
            .collect()
    }

    /// Decodes a PNG or JPEG, runs OCR for `source` and, when `translate` is set, translates into
    /// `target`. Blocking; call it off the main thread.
    pub fn translate_image(
        &self,
        image: Vec<u8>,
        source: String,
        target: String,
        translate: bool,
        listener: Option<Arc<dyn ProgressListener>>,
    ) -> Result<TranslationResult, FfiError> {
        let manifest = self.store.manifest();
        let src = manifest.language(&source)?;
        let tgt = manifest.language(&target)?;
        let img = image::load_from_memory(&image)?.to_rgb8();

        let mut cached = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        let reusable = cached.as_ref().is_some_and(|c| {
            Some(&c.script_pack) == src.script_pack.as_ref() && c.translate == translate
        });
        if !reusable {
            *cached = None;
            let pipeline = load_pipeline(&self.store, src, translate)?;
            *cached = Some(Cached {
                script_pack: src.script_pack.clone().unwrap_or_default(),
                translate,
                pipeline,
            });
        }
        let pipeline = &mut cached.as_mut().expect("pipeline loaded above").pipeline;
        let out = pipeline.run(&img, src, tgt, &mut |s| report(&listener, s))?;
        Ok(TranslationResult::from_output(
            out,
            img.width(),
            img.height(),
        ))
    }

    /// Frees loaded models (e.g. when the app is backgrounded).
    pub fn unload(&self) {
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use translatte_core::geometry::{BBox, Quad};
    use translatte_core::pipeline::TranslatedRegion;

    fn manifest_json() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../models/manifest.json"
        ))
        .unwrap()
    }

    #[test]
    fn region_conversion_keeps_corner_order_and_colors() {
        let out = PipelineOutput {
            regions: vec![TranslatedRegion {
                quad: Quad::from_bbox(BBox {
                    x0: 1.0,
                    y0: 2.0,
                    x1: 3.0,
                    y1: 4.0,
                }),
                source_text: "Sortie".into(),
                translated_text: Some("Exit".into()),
                bg_rgb: [1, 2, 3],
                fg_rgb: [4, 5, 6],
                angle: 0.5,
            }],
            translation_error: None,
        };
        let r = TranslationResult::from_output(out, 10, 20);
        assert_eq!((r.image_width, r.image_height), (10, 20));
        let region = &r.regions[0];
        assert_eq!(
            region.corners,
            vec![
                Point { x: 1.0, y: 2.0 },
                Point { x: 3.0, y: 2.0 },
                Point { x: 3.0, y: 4.0 },
                Point { x: 1.0, y: 4.0 },
            ]
        );
        assert_eq!(region.background, Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(region.foreground, Rgb { r: 4, g: 5, b: 6 });
        assert_eq!(region.translated_text.as_deref(), Some("Exit"));
    }

    #[test]
    fn empty_models_dir_has_no_source_languages() {
        let dir = tempfile_dir();
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        assert!(!engine.core_installed().unwrap());
        assert!(engine.source_languages().unwrap().is_empty());
        assert!(engine.target_languages().iter().any(|l| l.code == "en"));
    }

    #[test]
    fn errors_are_reported_not_panicked() {
        let dir = tempfile_dir();
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let err = engine
            .translate_image(vec![1, 2, 3], "fr".into(), "en".into(), false, None)
            .unwrap_err();
        assert!(err.to_string().contains("decode"), "{err}");
        let err = engine
            .translate_image(vec![], "xx".into(), "en".into(), false, None)
            .unwrap_err();
        assert!(err.to_string().contains("unknown language"), "{err}");
        assert!(Engine::new("/tmp".into(), "{}".into()).is_err());
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "translatte-ffi-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Recorder(Mutex<Vec<String>>);
    impl ProgressListener for Recorder {
        fn on_stage(&self, stage: String, _done: u32, _total: u32) {
            self.0.lock().unwrap().push(stage);
        }
    }

    #[test]
    fn translates_hello_world_with_real_models() {
        let Some(models) = std::env::var_os("TRANSLATTE_MODELS") else {
            eprintln!("TRANSLATTE_MODELS not set; skipping");
            return;
        };
        let engine = Engine::new(models.to_string_lossy().into(), manifest_json()).unwrap();
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/data/hello_world.png"
        ))
        .unwrap();
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let r = engine
            .translate_image(png, "en".into(), "de".into(), true, Some(recorder.clone()))
            .unwrap();
        assert_eq!(r.regions.len(), 1);
        assert_eq!(r.regions[0].source_text, "HELLO WORLD");
        let de = r.regions[0].translated_text.clone().unwrap().to_lowercase();
        assert!(de.contains("hallo"), "{de}");
        let stages = recorder.0.lock().unwrap();
        assert_eq!(stages.first().map(String::as_str), Some("Detecting"));
        assert!(stages.iter().any(|s| s == "Translating"));
    }
}
