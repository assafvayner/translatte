#![cfg(feature = "llama")]

use std::path::PathBuf;

use translatte_core::manifest::Manifest;
use translatte_core::model_store::ModelStore;
use translatte_core::translate::batch::BatchTranslator;
use translatte_core::translate::llama::LlamaGenerator;
use translatte_core::translate::Translator;

fn store() -> Option<ModelStore> {
    let dir = PathBuf::from(std::env::var_os("TRANSLATTE_MODELS")?);
    let manifest_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../models/manifest.json");
    let manifest = Manifest::from_json(&std::fs::read_to_string(manifest_path).unwrap()).unwrap();
    Some(ModelStore::new(manifest, dir))
}

#[test]
fn translates_french_blocks_to_english() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let generator =
        LlamaGenerator::load(&store.model_file("translator", "model.gguf").unwrap()).unwrap();
    let mut t = BatchTranslator::new(generator);
    let m = store.manifest();
    let blocks = vec![
        "Sortie de secours".to_string(),
        "Défense de fumer".to_string(),
    ];
    let out = t
        .translate(
            &blocks,
            m.language("fr").unwrap(),
            m.language("en").unwrap(),
        )
        .unwrap();
    eprintln!("{out:?}");
    let joined = out
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    assert!(
        joined.contains("exit") || joined.contains("evacuat"),
        "{joined}"
    );
    assert!(joined.contains("smok"), "{joined}");
}
