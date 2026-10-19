use std::path::Path;

use anyhow::{bail, Context, Result};
use translatte_core::manifest::{Language, Manifest};
use translatte_core::model_store::{InstallState, ModelStore};
use translatte_core::ocr::detector::Detector;
use translatte_core::ocr::recognizer::Recognizer;
use translatte_core::pipeline::Pipeline;
use translatte_core::translate::batch::BatchTranslator;
use translatte_core::translate::llama::LlamaGenerator;

pub type CliPipeline = Pipeline<Detector, Recognizer, BatchTranslator<LlamaGenerator>>;

pub fn load_store(manifest: &Path, models: &Path) -> Result<ModelStore> {
    let json = std::fs::read_to_string(manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    Ok(ModelStore::new(
        Manifest::from_json(&json)?,
        models.to_path_buf(),
    ))
}

fn require_installed(store: &ModelStore, id: &str) -> Result<()> {
    if store.state(id)? != InstallState::Installed {
        bail!(
            "model {id} is not installed in {}; run `uv run scripts/fetch_models.py`",
            store.root().display()
        );
    }
    Ok(())
}

pub fn build_pipeline(store: &ModelStore, src: &Language, translate: bool) -> Result<CliPipeline> {
    let Some(pack) = &src.script_pack else {
        bail!(
            "{} has no OCR script pack, so it can only be a target language",
            src.code
        );
    };
    require_installed(store, "det")?;
    require_installed(store, pack)?;
    let detector = Detector::load(&store.model_file("det", "inference.onnx")?)?;
    let recognizer = Recognizer::load(
        &store.model_file(pack, "inference.onnx")?,
        &store.model_file(pack, "dict.txt")?,
    )?;
    let translator = if translate {
        require_installed(store, "translator")?;
        Some(BatchTranslator::new(LlamaGenerator::load(
            &store.model_file("translator", "model.gguf")?,
        )?))
    } else {
        None
    };
    Ok(Pipeline::new(detector, recognizer, translator))
}
