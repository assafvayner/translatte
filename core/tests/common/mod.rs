use std::path::PathBuf;

use translatte_core::manifest::Manifest;
use translatte_core::model_store::ModelStore;

/// Real-model tests run only when TRANSLATTE_MODELS points at a fetched models dir (see `translatte fetch`).
pub fn store() -> Option<ModelStore> {
    let dir = PathBuf::from(std::env::var_os("TRANSLATTE_MODELS")?);
    let manifest_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../models/manifest.json");
    let manifest = Manifest::from_json(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
    Some(ModelStore::new(manifest, dir))
}
