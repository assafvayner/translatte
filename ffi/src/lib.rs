use std::path::PathBuf;
#[cfg(feature = "download")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "download")]
use std::sync::TryLockError;
use std::sync::{Arc, Mutex, MutexGuard};

#[cfg(feature = "download")]
use translatte_core::download::Downloader;
use translatte_core::frame::upright_rgb;
use translatte_core::geometry::{Pt, Quad};
use translatte_core::loader::{load_pipeline, LoadedPipeline};
use translatte_core::manifest::{Language, Manifest, ModelEntry, ModelKind};
use translatte_core::model_store::{InstallState, ModelStore};
use translatte_core::pipeline::{PipelineOutput, Stage, TranslatedRegion};
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
    /// Core models are required and can't be deleted; the rest are script packs and pair models.
    pub core: bool,
    pub size_bytes: u64,
    /// Bytes already on disk, including partial downloads that will resume.
    pub downloaded_bytes: u64,
    pub state: ModelState,
    /// Names of the source languages this pack reads.
    pub languages: Vec<String>,
}

/// What a user downloads to read and translate a language: its OCR script pack and its
/// translation model. Languages that share a script pack download it once.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LanguagePack {
    pub code: String,
    pub name: String,
    /// The script pack, then the translation model; download both to install the pack.
    pub model_ids: Vec<String>,
    pub size_bytes: u64,
    /// Bytes already on disk, including a script pack another language installed and partial
    /// downloads that will resume.
    pub downloaded_bytes: u64,
    pub state: ModelState,
    /// Deleting the pack would free space: its translation model, or a script pack no other
    /// installed language needs, is on disk.
    pub deletable: bool,
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

impl Region {
    fn to_core(&self) -> Result<TranslatedRegion, FfiError> {
        let corners: [Point; 4] =
            self.corners
                .as_slice()
                .try_into()
                .map_err(|_| FfiError::Failed {
                    reason: format!("a region has {} corners, expected 4", self.corners.len()),
                })?;
        Ok(TranslatedRegion {
            quad: Quad(corners.map(|p| Pt { x: p.x, y: p.y })),
            source_text: self.source_text.clone(),
            translated_text: self.translated_text.clone(),
            bg_rgb: [self.background.r, self.background.g, self.background.b],
            fg_rgb: [self.foreground.r, self.foreground.g, self.foreground.b],
            angle: self.angle,
        })
    }
}

/// Milliseconds each pipeline stage took and the precision OCR ran at (`fp32` for photos, `fp16`
/// for live frames), for the live camera's debug readout.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct StageTimings {
    pub detect_ms: u32,
    pub recognize_ms: u32,
    pub translate_ms: u32,
    #[uniffi(default = "fp32")]
    pub precision: String,
    /// Blocks of a live frame that took an earlier frame's translation.
    #[uniffi(default = 0)]
    pub reused: u32,
}

impl From<translatte_core::pipeline::StageTimings> for StageTimings {
    fn from(t: translatte_core::pipeline::StageTimings) -> Self {
        Self {
            detect_ms: t.detect_ms,
            recognize_ms: t.recognize_ms,
            translate_ms: t.translate_ms,
            precision: t.precision.name().into(),
            reused: t.reused,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TranslationResult {
    pub image_width: u32,
    pub image_height: u32,
    pub regions: Vec<Region>,
    pub translation_error: Option<String>,
    pub timings: StageTimings,
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
            timings: out.timings.into(),
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

/// Points the Hugging Face client caches at an app-writable directory and turns off hf-xet's
/// transfer telemetry. hf-hub builds its Xet session from the environment with no way to pass a
/// config, so this sets process environment variables: call it once, as early as possible (on
/// Android, from `Application.attachBaseContext`, before content providers start threads).
#[cfg(feature = "download")]
#[uniffi::export]
pub fn configure_downloads(cache_dir: String) {
    let dir = PathBuf::from(cache_dir);
    let vars = [
        ("HF_HOME", dir.join("hf").into_os_string()),
        ("HF_XET_CACHE", dir.join("xet").into_os_string()),
        ("HF_HUB_DISABLE_TELEMETRY", "1".into()),
    ];
    for (key, value) in vars {
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
}

/// Cancels the [`Engine::download`] call it was passed to. Create one per call, before starting
/// it, so a cancel can't be lost or leak into the next download.
#[cfg(feature = "download")]
#[derive(Default, uniffi::Object)]
pub struct DownloadToken {
    cancelled: AtomicBool,
}

#[cfg(feature = "download")]
#[uniffi::export]
impl DownloadToken {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

struct Cached {
    script_pack: String,
    translate: bool,
    pipeline: Arc<LoadedPipeline>,
}

/// Entry point for native apps. Pipelines are cached per script pack, so switching between
/// languages that share a pack (e.g. French and German) only swaps the translation model.
#[derive(uniffi::Object)]
pub struct Engine {
    store: ModelStore,
    cached: Mutex<Option<Cached>>,
    #[cfg(feature = "download")]
    downloading: Mutex<()>,
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
        }))
    }

    /// Deletes model files the manifest no longer describes (retired models such as the old
    /// LLM translator, the OCR models' ONNX files, replaced files, partial downloads of other
    /// upstream files), so they don't keep taking space after an app update. Returns how many files or directories were removed. Call it once after
    /// creating the engine; it blocks, so call it off the main thread.
    pub fn prune_models(&self) -> Result<u32, FfiError> {
        // A download started meanwhile waits rather than racing the cleanup.
        #[cfg(feature = "download")]
        let _downloading = self.downloading.lock().unwrap_or_else(|e| e.into_inner());
        Ok(self.store.prune()?.len() as u32)
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
                    downloaded_bytes: self.store.downloaded_bytes(&m.id)?,
                    state: self.store.state(&m.id)?.into(),
                    languages: pack_languages(manifest, m),
                })
            })
            .collect()
    }

    /// Every source language's pack, sorted by name.
    pub fn language_packs(&self) -> Result<Vec<LanguagePack>, FfiError> {
        let manifest = self.store.manifest();
        let mut packs = Vec::new();
        for l in self.store.source_languages() {
            let ids = self.store.language_models(l);
            let mut size_bytes = 0;
            let mut downloaded_bytes = 0;
            for id in &ids {
                size_bytes += manifest.model(id).map_or(0, ModelEntry::total_size);
                downloaded_bytes += self.store.downloaded_bytes(id)?;
            }
            packs.push(LanguagePack {
                code: l.code.clone(),
                name: l.name.clone(),
                model_ids: ids.iter().map(|id| id.to_string()).collect(),
                size_bytes,
                downloaded_bytes,
                state: self.store.language_state(&l.code)?.into(),
                deletable: self.store.language_deletable(&l.code)?,
            });
        }
        Ok(packs)
    }

    /// Deletes a language pack: its translation model, and its script pack unless another
    /// installed language reads with it. If that script is the one loaded, all loaded models are
    /// freed first and this returns true. Fails while a download is running, which could otherwise
    /// write the files again.
    pub fn delete_language(&self, code: String) -> Result<bool, FfiError> {
        #[cfg(feature = "download")]
        let _downloading = match self.downloading.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
            Err(TryLockError::WouldBlock) => {
                return Err(FfiError::Failed {
                    reason: "Can't delete while a download is running. Cancel it first.".into(),
                })
            }
        };
        let language = self.store.manifest().language(&code)?;
        let mut cached = self.lock_cached();
        let unloaded = cached
            .as_ref()
            .is_some_and(|c| Some(&c.script_pack) == language.script_pack.as_ref());
        if unloaded {
            *cached = None;
        }
        self.store.delete_language(&code)?;
        Ok(unloaded)
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

    /// The language every translation is into.
    pub fn target_language(&self) -> Result<LanguageInfo, FfiError> {
        Ok(self.store.target_language()?.into())
    }

    /// Loads the models for `source`, including its translation model, and runs that once, so the
    /// next `translate_image` from `source` starts right away. Call it when the source language is
    /// picked or changes and when the app returns to the foreground. Blocking; call it off the main
    /// thread.
    pub fn prepare(&self, source: String) -> Result<(), FfiError> {
        let src = self.store.manifest().language(&source)?;
        let tgt = self.store.target_language()?;
        Ok(self.pipeline(src, true)?.warm(src, tgt)?)
    }

    /// Decodes a PNG or JPEG, runs OCR for `source` and, when `translate` is set, translates the
    /// text into English. Blocking; call it off the main thread. The cache holds one pipeline, so a
    /// call with `translate` unset replaces a cached translating pipeline with an OCR-only one, and
    /// the next translating call (or `prepare`) loads the translation model again.
    pub fn translate_image(
        &self,
        image: Vec<u8>,
        source: String,
        translate: bool,
        listener: Option<Arc<dyn ProgressListener>>,
    ) -> Result<TranslationResult, FfiError> {
        let src = self.store.manifest().language(&source)?;
        let tgt = self.store.target_language()?;
        let img = image::load_from_memory(&image)?.to_rgb8();

        let pipeline = self.pipeline(src, translate)?;
        let out = pipeline.run(&img, src, tgt, &mut |s| report(&listener, s))?;
        Ok(TranslationResult::from_output(
            out,
            img.width(),
            img.height(),
        ))
    }

    /// Runs the same pipeline as `translate_image` on a camera frame: tightly packed RGBA rows of
    /// `width` x `height`, turned upright by rotating clockwise by `rotation_degrees` (0, 90, 180 or
    /// 270). Region coordinates, `image_width` and `image_height` are in the upright frame. Always
    /// translates; shares the pipeline and translation cache with `translate_image`. Blocking; call
    /// it off the main thread.
    pub fn translate_frame(
        &self,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        rotation_degrees: u32,
        source: String,
    ) -> Result<TranslationResult, FfiError> {
        let src = self.store.manifest().language(&source)?;
        let tgt = self.store.target_language()?;
        let img = upright_rgb(rgba, width, height, rotation_degrees)?;

        let out = self.pipeline(src, true)?.run_frame(&img, src, tgt)?;
        Ok(TranslationResult::from_output(
            out,
            img.width(),
            img.height(),
        ))
    }

    /// The first half of `translate_frame`: reads the frame's text without translating it
    /// (`translated_text` is `None`, `translate_ms` 0). Pass the result to `translate_regions`;
    /// that may run on another thread while the next frame is read. Blocking; call it off the main
    /// thread.
    pub fn ocr_frame(
        &self,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        rotation_degrees: u32,
        source: String,
    ) -> Result<TranslationResult, FfiError> {
        let src = self.store.manifest().language(&source)?;
        let img = upright_rgb(rgba, width, height, rotation_degrees)?;
        let out = self.pipeline(src, true)?.read_frame(&img, src)?;
        Ok(TranslationResult::from_output(
            out,
            img.width(),
            img.height(),
        ))
    }

    /// The second half of `translate_frame`: translates the regions `ocr_frame` read from
    /// `source`, reusing the last frames' translations of blocks that barely changed, and fills in
    /// `translate_ms`. Blocking; call it off the main thread.
    pub fn translate_regions(
        &self,
        frame: TranslationResult,
        source: String,
    ) -> Result<TranslationResult, FfiError> {
        let src = self.store.manifest().language(&source)?;
        let tgt = self.store.target_language()?;
        let regions = frame
            .regions
            .iter()
            .map(Region::to_core)
            .collect::<Result<Vec<_>, _>>()?;
        let out = self.pipeline(src, true)?.translate_frame(
            PipelineOutput {
                regions,
                translation_error: None,
                timings: Default::default(),
            },
            src,
            tgt,
        );
        let mut result = frame;
        for (r, o) in result.regions.iter_mut().zip(out.regions) {
            r.translated_text = o.translated_text;
        }
        result.translation_error = out.translation_error;
        result.timings.translate_ms = out.timings.translate_ms;
        result.timings.reused = out.timings.reused;
        Ok(result)
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
    /// resuming partial ones. Waits for a download already running (e.g. one still unwinding after
    /// a cancel) to finish first. Blocking; call it off the main thread. Returns
    /// [`FfiError::Cancelled`] once `token` is cancelled.
    pub fn download(
        &self,
        model_ids: Vec<String>,
        listener: Arc<dyn DownloadListener>,
        token: Arc<DownloadToken>,
    ) -> Result<(), FfiError> {
        let _guard = self.downloading.lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<&str> = model_ids.iter().map(String::as_str).collect();
        Downloader::new(&self.store)?.download_models_blocking(&ids, &token.cancelled, |p| {
            listener.on_progress(p.model_id, p.model_bytes_done, p.model_bytes_total)
        })?;
        Ok(())
    }
}

impl Engine {
    /// A panic mid-run can leave the cached pipeline in a bad state (e.g. a half-run native batch),
    /// so a poisoned lock drops it rather than reusing it.
    fn lock_cached(&self) -> MutexGuard<'_, Option<Cached>> {
        self.cached.lock().unwrap_or_else(|poisoned| {
            let mut cached = poisoned.into_inner();
            *cached = None;
            self.cached.clear_poison();
            cached
        })
    }

    /// The pipeline for `src`, loading it unless the cached one fits. Runs use it without holding
    /// the cache lock, so a frame can be translated while the next one is read.
    fn pipeline(&self, src: &Language, translate: bool) -> Result<Arc<LoadedPipeline>, FfiError> {
        let mut cached = self.lock_cached();
        let reusable = cached.as_ref().is_some_and(|c| {
            Some(&c.script_pack) == src.script_pack.as_ref() && c.translate == translate
        });
        if !reusable {
            *cached = None;
            let pipeline = load_pipeline(&self.store, src, translate, None)?;
            *cached = Some(Cached {
                script_pack: src.script_pack.clone().unwrap_or_default(),
                translate,
                pipeline: Arc::new(pipeline),
            });
        }
        Ok(cached
            .as_ref()
            .expect("pipeline loaded above")
            .pipeline
            .clone())
    }
}

fn pack_languages(manifest: &Manifest, model: &ModelEntry) -> Vec<String> {
    let id = Some(model.id.as_str());
    manifest
        .languages
        .iter()
        .filter(|l| l.script_pack.as_deref() == id || l.pair_model.as_deref() == id)
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

    /// Writes sparse files of the manifest sizes, so `id` reads as installed.
    fn install(dir: &std::path::Path, id: &str) {
        let manifest = Manifest::from_json(&manifest_json()).unwrap();
        for f in &manifest.model(id).unwrap().files {
            let path = dir.join(&f.path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(&path)
                .unwrap()
                .set_len(f.size)
                .unwrap();
        }
    }

    fn pack(engine: &Engine, code: &str) -> LanguagePack {
        engine
            .language_packs()
            .unwrap()
            .into_iter()
            .find(|p| p.code == code)
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
            timings: translatte_core::pipeline::StageTimings {
                detect_ms: 1,
                recognize_ms: 2,
                translate_ms: 3,
                reused: 4,
                ..Default::default()
            },
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
        assert_eq!(
            r.timings,
            StageTimings {
                detect_ms: 1,
                recognize_ms: 2,
                translate_ms: 3,
                precision: "fp32".into(),
                reused: 4,
            }
        );
    }

    #[test]
    fn bad_frames_are_reported_not_panicked() {
        let dir = tempfile_dir().join("frames");
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let err = engine
            .translate_frame(vec![0; 3], 1, 1, 0, "fr".into())
            .unwrap_err();
        assert!(err.to_string().contains("expected 4"), "{err}");
        let err = engine
            .translate_frame(vec![0; 4], 1, 1, 45, "fr".into())
            .unwrap_err();
        assert!(err.to_string().contains("rotation"), "{err}");
        let err = engine
            .translate_frame(vec![0; 4], 1, 1, 0, "xx".into())
            .unwrap_err();
        assert!(err.to_string().contains("unknown language"), "{err}");
    }

    #[test]
    fn translate_frame_matches_translate_image_with_real_models() {
        let Some(models) = std::env::var_os("TRANSLATTE_MODELS") else {
            eprintln!("TRANSLATTE_MODELS not set; skipping");
            return;
        };
        let engine = Engine::new(models.to_string_lossy().into(), manifest_json()).unwrap();
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/data/bonjour.png"
        ))
        .unwrap();
        let rgba = image::load_from_memory(&png).unwrap().to_rgba8();
        let (w, h) = rgba.dimensions();
        let photo = engine
            .translate_image(png, "fr".into(), true, None)
            .unwrap();

        let frame = engine
            .translate_frame(rgba.clone().into_raw(), w, h, 0, "fr".into())
            .unwrap();
        assert_eq!((frame.image_width, frame.image_height), (w, h));
        assert_eq!(frame.regions, photo.regions);
        assert!(frame.timings.detect_ms + frame.timings.recognize_ms > 0);

        // A sensor-landscape frame that needs a clockwise quarter turn to be upright.
        let sideways = image::imageops::rotate270(&rgba);
        let turned = engine
            .translate_frame(sideways.into_raw(), h, w, 90, "fr".into())
            .unwrap();
        assert_eq!((turned.image_width, turned.image_height), (w, h));
        assert_eq!(turned.regions, photo.regions);
    }

    #[test]
    fn ocr_frame_then_translate_regions_matches_translate_frame_with_real_models() {
        let Some(models) = std::env::var_os("TRANSLATTE_MODELS") else {
            eprintln!("TRANSLATTE_MODELS not set; skipping");
            return;
        };
        let engine = Engine::new(models.to_string_lossy().into(), manifest_json()).unwrap();
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/data/bonjour.png"
        ))
        .unwrap();
        let rgba = image::load_from_memory(&png).unwrap().to_rgba8();
        let (w, h) = rgba.dimensions();
        let whole = engine
            .translate_frame(rgba.clone().into_raw(), w, h, 0, "fr".into())
            .unwrap();

        let read = engine
            .ocr_frame(rgba.into_raw(), w, h, 0, "fr".into())
            .unwrap();
        assert!(read.regions.iter().all(|r| r.translated_text.is_none()));
        assert_eq!(read.timings.translate_ms, 0);
        let split = engine.translate_regions(read, "fr".into()).unwrap();
        assert_eq!(split.regions, whole.regions);
        assert_eq!(split.timings.reused as usize, split.regions.len());
    }

    #[test]
    fn translate_regions_rejects_a_region_without_four_corners() {
        let dir = tempfile_dir().join("corners");
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let frame = TranslationResult {
            image_width: 1,
            image_height: 1,
            regions: vec![Region {
                corners: vec![Point { x: 0.0, y: 0.0 }],
                source_text: "a".into(),
                translated_text: None,
                background: Rgb { r: 0, g: 0, b: 0 },
                foreground: Rgb { r: 0, g: 0, b: 0 },
                angle: 0.0,
            }],
            translation_error: None,
            timings: StageTimings {
                detect_ms: 0,
                recognize_ms: 0,
                translate_ms: 0,
                precision: "fp32".into(),
                reused: 0,
            },
        };
        let err = engine.translate_regions(frame, "fr".into()).unwrap_err();
        assert!(err.to_string().contains("corners"), "{err}");
    }

    #[test]
    fn empty_models_dir_has_no_source_languages() {
        let dir = tempfile_dir().join("empty");
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        assert!(!engine.core_installed().unwrap());
        assert!(engine.source_languages().unwrap().is_empty());
        assert_eq!(engine.target_language().unwrap().code, "en");
        let packs = engine.language_packs().unwrap();
        assert_eq!(packs.len(), 52);
        assert!(packs
            .iter()
            .all(|p| p.state == ModelState::NotInstalled && !p.deletable));
    }

    #[test]
    fn lists_models_with_their_languages() {
        let dir = tempfile_dir().join("models");
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
        assert!(!engine.delete_language("fr".into()).unwrap());
        assert!(engine.delete_language("en".into()).is_ok());
    }

    #[test]
    fn a_language_pack_counts_a_shared_script_pack_as_downloaded() {
        let dir = tempfile_dir().join("packs");
        for id in ["det", "rec-latin", "pair-fr-en"] {
            install(&dir, id);
        }
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let fr = pack(&engine, "fr");
        assert_eq!(fr.model_ids, vec!["rec-latin", "pair-fr-en"]);
        assert_eq!(fr.state, ModelState::Installed);
        assert_eq!(fr.downloaded_bytes, fr.size_bytes);
        assert!(fr.deletable);
        let de = pack(&engine, "de");
        let latin = engine
            .models()
            .unwrap()
            .into_iter()
            .find(|m| m.id == "rec-latin")
            .unwrap();
        assert_eq!(de.state, ModelState::NotInstalled);
        assert_eq!(de.downloaded_bytes, latin.size_bytes);
        assert!(!de.deletable, "French still needs the Latin pack");
        let codes: Vec<_> = engine
            .source_languages()
            .unwrap()
            .into_iter()
            .map(|l| l.code)
            .collect();
        assert_eq!(codes, vec!["fr"]);
    }

    #[test]
    fn deleting_a_language_keeps_a_script_pack_another_one_uses() {
        let dir = tempfile_dir().join("delete-language");
        for id in ["det", "rec-latin", "pair-fr-en", "pair-de-en"] {
            install(&dir, id);
        }
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        assert!(!engine.delete_language("de".into()).unwrap());
        assert_eq!(pack(&engine, "de").state, ModelState::NotInstalled);
        assert_eq!(pack(&engine, "fr").state, ModelState::Installed);
        engine.delete_language("fr".into()).unwrap();
        assert!(!dir.join("rec-latin").exists());
        assert!(!dir.join("pair-fr-en").exists());
    }

    #[test]
    fn pruning_an_old_install_removes_the_llm_retired_packs_and_onnx_files() {
        let dir = tempfile_dir().join("prune");
        let old_gguf = dir.join("translator/model.gguf");
        std::fs::create_dir_all(old_gguf.parent().unwrap()).unwrap();
        std::fs::File::create(&old_gguf)
            .unwrap()
            .set_len(1_133_080_448)
            .unwrap();
        std::fs::write(dir.join("translator/model.gguf.part"), "old").unwrap();
        for retired in ["rec-el", "rec-en"] {
            std::fs::create_dir_all(dir.join(retired)).unwrap();
            std::fs::write(dir.join(retired).join("inference.onnx"), "x").unwrap();
        }
        std::fs::create_dir_all(dir.join("rec-latin")).unwrap();
        std::fs::write(dir.join("rec-latin/inference.onnx"), "x").unwrap();
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();

        assert_eq!(engine.prune_models().unwrap(), 4);
        for gone in ["translator", "rec-el", "rec-en", "rec-latin/inference.onnx"] {
            assert!(!dir.join(gone).exists(), "{gone}");
        }
        let models = engine.models().unwrap();
        assert!(models.iter().all(|m| m.id != "translator"));
        let pair = models.iter().find(|m| m.id == "pair-fr-en").unwrap();
        assert!(!pair.core);
        assert_eq!(pair.languages, vec!["French"]);
        assert_eq!(engine.prune_models().unwrap(), 0);
    }

    #[cfg(feature = "download")]
    struct NoProgress;
    #[cfg(feature = "download")]
    impl DownloadListener for NoProgress {
        fn on_progress(&self, _: String, _: u64, _: u64) {}
    }

    #[cfg(feature = "download")]
    #[test]
    fn delete_is_refused_while_downloading() {
        let dir = tempfile_dir();
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let downloading = engine.downloading.lock().unwrap();
        let err = engine.delete_language("fr".into()).unwrap_err();
        assert!(err.to_string().contains("download is running"), "{err}");
        drop(downloading);
        assert!(!engine.delete_language("fr".into()).unwrap());
    }

    #[cfg(feature = "download")]
    #[test]
    fn download_reports_unknown_models() {
        let dir = tempfile_dir();
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let err = engine
            .download(
                vec!["nope".into()],
                Arc::new(NoProgress),
                DownloadToken::new(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("unknown model"), "{err}");
    }

    #[cfg(feature = "download")]
    #[test]
    fn cancelled_token_stops_download() {
        let dir = tempfile_dir();
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let token = DownloadToken::new();
        token.cancel();
        let err = engine
            .download(vec!["rec-latin".into()], Arc::new(NoProgress), token)
            .unwrap_err();
        assert!(matches!(err, FfiError::Cancelled), "{err}");
        // A fresh token isn't affected by the earlier cancel.
        let err = engine
            .download(
                vec!["nope".into()],
                Arc::new(NoProgress),
                DownloadToken::new(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("unknown model"), "{err}");
    }

    #[test]
    fn errors_are_reported_not_panicked() {
        let dir = tempfile_dir().join("errors");
        let engine = Engine::new(dir.to_string_lossy().into(), manifest_json()).unwrap();
        let err = engine
            .translate_image(vec![1, 2, 3], "fr".into(), false, None)
            .unwrap_err();
        assert!(err.to_string().contains("decode"), "{err}");
        let err = engine
            .translate_image(vec![], "xx".into(), false, None)
            .unwrap_err();
        assert!(err.to_string().contains("unknown language"), "{err}");
        let err = engine.prepare("xx".into()).unwrap_err();
        assert!(err.to_string().contains("unknown language"), "{err}");
        assert!(engine.prepare("fr".into()).is_err());
        assert!(engine.delete_language("xx".into()).is_err());
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
    fn translates_bonjour_with_real_models() {
        let Some(models) = std::env::var_os("TRANSLATTE_MODELS") else {
            eprintln!("TRANSLATTE_MODELS not set; skipping");
            return;
        };
        let engine = Engine::new(models.to_string_lossy().into(), manifest_json()).unwrap();
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/data/bonjour.png"
        ))
        .unwrap();
        engine.prepare("fr".into()).unwrap();
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let r = engine
            .translate_image(png, "fr".into(), true, Some(recorder.clone()))
            .unwrap();
        assert_eq!(r.regions.len(), 1);
        assert_eq!(r.regions[0].source_text.to_lowercase(), "bonjour le monde");
        let en = r.regions[0].translated_text.clone().unwrap().to_lowercase();
        assert!(en.contains("world") || en.contains("hello"), "{en}");
        assert!(r.translation_error.is_none());
        let stages = recorder.0.lock().unwrap();
        assert_eq!(stages.first().map(String::as_str), Some("Detecting"));
        assert!(stages.iter().any(|s| s == "Translating"));
    }
}
