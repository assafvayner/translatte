use std::path::Path;

use anyhow::{Context, Result};
use translatte_core::loader::{load_pipeline_with, LoadedPipeline};
use translatte_core::manifest::{Language, Manifest};
use translatte_core::model_store::ModelStore;
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
    translator_gguf: Option<&Path>,
) -> Result<LoadedPipeline> {
    load_pipeline_with(store, src, translate, translator_gguf).map_err(|e| match e {
        TranslatteError::ModelNotInstalled(id) => anyhow::anyhow!(
            "model {id} is not installed in {}; run `translatte fetch`",
            store.root().display()
        ),
        e => e.into(),
    })
}
