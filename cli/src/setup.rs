use std::path::Path;

use anyhow::{Context, Result};
use translatte_core::loader::{load_pipeline, LoadedPipeline};
use translatte_core::manifest::{Language, Manifest};
use translatte_core::model_store::ModelStore;
use translatte_core::ocr::OcrEngine;
use translatte_core::TranslatteError;

pub fn load_store(manifest: &Path, models: &Path) -> Result<ModelStore> {
    let json = std::fs::read_to_string(manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    Ok(ModelStore::new(
        Manifest::from_json(&json)?,
        models.to_path_buf(),
    ))
}

pub fn build_pipeline(
    store: &ModelStore,
    src: &Language,
    translate: bool,
    engine: OcrEngine,
) -> Result<LoadedPipeline> {
    load_pipeline(store, src, translate, engine).map_err(|e| match e {
        TranslatteError::ModelNotInstalled(id) => anyhow::anyhow!(
            "model {id} is not installed in {}; run `translatte fetch --langs {}`",
            store.root().display(),
            src.code
        ),
        e => e.into(),
    })
}
