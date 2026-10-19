use std::path::PathBuf;
#[cfg(feature = "download")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

#[cfg(feature = "download")]
use translatte_core::download::Downloader;
use translatte_core::loader::{load_pipeline, LoadedPipeline};
use translatte_core::manifest::{Language, Manifest, ModelEntry, ModelKind};
use translatte_core::model_store::{InstallState, ModelStore};
use translatte_core::pipeline::{PipelineOutput, Stage};
use translatte_core::TranslatteError;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    // Not `message`: in Kotlin that would clash with `Throwable.message`.
    #[error("{reason}")]
    Failed { reason: String },
    #[error("cancelled")]
    Cancelled,
}

impl From<TranslatteError> for FfiError {
    fn from(e: TranslatteError) -> Self {
        match e {
            TranslatteError::Cancelled => Self::Cancelled,
            e => Self::Failed {
                reason: e.to_string(),
            },
        }
    }
}

impl From<image::ImageError> for FfiError {
    fn from(e: image::ImageError) -> Self {
        Self::Failed {
            reason: format!("could not decode image: {e}"),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ModelState {
    NotInstalled,
    Installed,
    /// Some files are missing or have the wrong size, e.g. after an interrupted download.
    Incomplete,
}

impl From<InstallState> for ModelState {
    fn from(s: InstallState) -> Self {
        match s {
            InstallState::NotInstalled => Self::NotInstalled,
            InstallState::Installed => Self::Installed,
            InstallState::Corrupt => Self::Incomplete,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    /// Core models are required and can't be deleted; the rest are script packs.
    pub core: bool,
    pub size_bytes: u64,
    pub state: ModelState,
    /// Names of the source languages this pack reads.
    pub languages: Vec<String>,
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

#[cfg(feature = "download")]
#[uniffi::export(with_foreign)]
pub trait DownloadListener: Send + Sync {
    /// Byte progress within one model; `done` can drop back to 0 if a file restarts.
    fn on_progress(&self, model_id: String, done: u64, total: u64);
}

/// Points the Hugging Face client caches at an app-writable directory. Call once at startup,
/// before any other thread runs, as it sets process environment variables.
#[cfg(feature = "download")]
#[uniffi::export]
pub fn set_cache_dir(dir: String) {
    let dir = PathBuf::from(dir);
    for (key, sub) in [("HF_HOME", "hf"), ("HF_XET_CACHE", "xet")] {
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, dir.join(sub));
        }
    }
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
    #[cfg(feature = "download")]
    downloading: Mutex<()>,
    #[cfg(feature = "download")]
    cancel: AtomicBool,
}

#[uniffi::export]
impl Engine {
    #[uniffi::constructor]
    pub fn new(models_dir: String, manifest_json: String) -> Result<Arc<Self>, FfiError> {
        let manifest = Manifest::from_json(&manifest_json)?;
        Ok(Arc::new(Self {
            store: ModelStore::new(manifest, PathBuf::from(models_dir)),
            cached: Mutex::new(None),
            #[cfg(feature = "download")]
            downloading: Mutex::new(()),
            #[cfg(feature = "download")]
            cancel: AtomicBool::new(false),
        }))
    }

    pub fn models(&self) -> Result<Vec<ModelInfo>, FfiError> {
        let manifest = self.store.manifest();
        manifest
            .models
            .iter()
            .map(|m| {
                Ok(ModelInfo {
                    id: m.id.clone(),
                    display_name: m.display_name.clone(),
                    core: m.kind == ModelKind::Core,
                    size_bytes: m.total_size(),
                    state: self.store.state(&m.id)?.into(),
                    languages: pack_languages(manifest, m),
                })
            })
            .collect()
    }

    /// Deletes a script pack, unloading it first if it is in use.
    pub fn delete_model(&self, id: String) -> Result<(), FfiError> {
        let mut cached = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if cached.as_ref().is_some_and(|c| c.script_pack == id) {
            *cached = None;
        }
        Ok(self.store.delete(&id)?)
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

    /// Loads the models for `source` and prefills the translator's instruction prefix for
    /// `source` to `target`, so the next `translate_image` for this pair starts translating sooner.
    /// Call it when the languages are picked or change. Blocking; call it off the main thread.
    pub fn prepare(&self, source: String, target: String) -> Result<(), FfiError> {
        let manifest = self.store.manifest();
        let src = manifest.language(&source)?;
        let tgt = manifest.language(&target)?;
        let mut cached = self.lock_cached();
        Ok(self.pipeline(&mut cached, src, true)?.warm(src, tgt)?)
    }

    /// Decodes a PNG or JPEG, runs OCR for `source` and, when `translate` is set, translates into
    /// `target`. Blocking; call it off the main thread. The cache holds one pipeline, so a call with
    /// `translate` unset replaces a cached translating pipeline with an OCR-only one, and the next
    /// translating call (or `prepare`) reloads the translator.
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

        let mut cached = self.lock_cached();
        let pipeline = self.pipeline(&mut cached, src, translate)?;
        let out = pipeline.run(&img, src, tgt, &mut |s| report(&listener, s))?;
        Ok(TranslationResult::from_output(
            out,
            img.width(),
            img.height(),
        ))
    }

    /// Frees loaded models (e.g. when the app is backgrounded).
    pub fn unload(&self) {
        *self.lock_cached() = None;
    }
}

#[cfg(feature = "download")]
#[uniffi::export]
impl Engine {
    /// Downloads the given models one after another, skipping files already installed and
    /// resuming partial ones. Blocking; call it off the main thread. Returns
    /// [`FfiError::Cancelled`] after [`Self::cancel_download`].
    pub fn download(
        &self,
        model_ids: Vec<String>,
        listener: Arc<dyn DownloadListener>,
    ) -> Result<(), FfiError> {
        let _guard = self.downloading.try_lock().map_err(|_| FfiError::Failed {
            reason: "a download is already running".into(),
        })?;
        self.cancel.store(false, Ordering::SeqCst);
        let ids: Vec<&str> = model_ids.iter().map(String::as_str).collect();
        Downloader::new(&self.store)?.download_models_blocking(&ids, &self.cancel, |p| {
            listener.on_progress(p.model_id, p.model_bytes_done, p.model_bytes_total)
        })?;
        Ok(())
    }

    pub fn cancel_download(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

impl Engine {
    /// A panic mid-run can leave the cached pipeline in a bad state (e.g. llama.cpp's KV cache
    /// half-updated), so a poisoned lock drops it rather than reusing it.
    fn lock_cached(&self) -> MutexGuard<'_, Option<Cached>> {
        self.cached.lock().unwrap_or_else(|poisoned| {
            let mut cached = poisoned.into_inner();
            *cached = None;
            self.cached.clear_poison();
            cached
        })
    }

    fn pipeline<'a>(
        &self,
        cached: &'a mut Option<Cached>,
        src: &Language,
        translate: bool,
    ) -> Result<&'a mut LoadedPipeline, FfiError> {
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
        Ok(&mut cached.as_mut().expect("pipeline loaded above").pipeline)
    }
}

fn pack_languages(manifest: &Manifest, model: &ModelEntry) -> Vec<String> {
    manifest
        .languages
        .iter()
        .filter(|l| l.script_pack.as_deref() == Some(model.id.as_str()))
        .map(|l| l.name.clone())
        .collect()
}

/// JNI entry points for the Android app, outside UniFFI.
#[cfg(target_os = "android")]
mod android {
    use jni::errors::ThrowRuntimeExAndDefault;
    use jni::objects::{JClass, JObject};
    use jni::{jni_mangle, EnvUnowned};

    /// Routes Rust `tracing` output to logcat and hands the app context to
    /// rustls-platform-verifier, which checks TLS certificates through Android's trust manager.
    /// Must run once, before the first download.
    #[jni_mangle("co.huggingface.translatte.Platform")]
    pub fn init<'caller>(
        mut env: EnvUnowned<'caller>,
        _class: JClass<'caller>,
        context: JObject<'caller>,
    ) {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(paranoid_android::layer("translatte"))
            .try_init();
        env.with_env(|env| rustls_platform_verifier::android::init_with_env(env, context))
            .resolve::<ThrowRuntimeExAndDefault>();
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
    fn lists_models_with_pack_languages() {
        let dir = tempfile_dir();
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let models = engine.models().unwrap();
        let det = models.iter().find(|m| m.id == "det").unwrap();
        assert!(det.core && det.languages.is_empty());
        let latin = models.iter().find(|m| m.id == "rec-latin").unwrap();
        assert!(!latin.core);
        assert!(
            latin.languages.iter().any(|l| l == "French"),
            "{:?}",
            latin.languages
        );
        assert!(models.iter().all(|m| m.state == ModelState::NotInstalled));
        assert!(engine.delete_model("det".into()).is_err());
        engine.delete_model("rec-latin".into()).unwrap();
    }

    #[cfg(feature = "download")]
    struct NoProgress;
    #[cfg(feature = "download")]
    impl DownloadListener for NoProgress {
        fn on_progress(&self, _: String, _: u64, _: u64) {}
    }

    #[cfg(feature = "download")]
    #[test]
    fn download_reports_unknown_models() {
        let dir = tempfile_dir();
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let err = engine
            .download(vec!["nope".into()], Arc::new(NoProgress))
            .unwrap_err();
        assert!(err.to_string().contains("unknown model"), "{err}");
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
        let err = engine.prepare("xx".into(), "en".into()).unwrap_err();
        assert!(err.to_string().contains("unknown language"), "{err}");
        assert!(engine.prepare("fr".into(), "en".into()).is_err());
        assert!(Engine::new("/tmp".into(), "{}".into()).is_err());
    }

    #[test]
    fn poisoned_lock_drops_the_cached_pipeline() {
        let dir = tempfile_dir();
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _cached = engine.cached.lock().unwrap();
            panic!("panicked mid-run");
        }));
        assert!(engine.cached.is_poisoned());
        assert!(engine.lock_cached().is_none());
        assert!(!engine.cached.is_poisoned());
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
        engine.prepare("en".into(), "de".into()).unwrap();
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
