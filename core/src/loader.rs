use std::path::Path;

use crate::manifest::{Language, DETECTOR_ID, TRANSLATOR_ID};
use crate::model_store::{InstallState, ModelStore};
use crate::ocr::detector::Detector;
use crate::ocr::recognizer::Recognizer;
use crate::pipeline::Pipeline;
use crate::translate::batch::BatchTranslator;
use crate::translate::llama::LlamaGenerator;
use crate::{Result, TranslatteError};

pub type LoadedPipeline = Pipeline<Detector, Recognizer, BatchTranslator<LlamaGenerator>>;

fn require_installed(store: &ModelStore, id: &str) -> Result<()> {
    if store.state(id)? != InstallState::Installed {
        return Err(TranslatteError::ModelNotInstalled(id.to_string()));
    }
    Ok(())
}

/// Loads the detector, the recognizer for `src`'s script pack and, when `translate` is set, the LLM.
pub fn load_pipeline(
    store: &ModelStore,
    src: &Language,
    translate: bool,
) -> Result<LoadedPipeline> {
    load_pipeline_with(store, src, translate, None)
}

/// Like [`load_pipeline`], but `translator_gguf` replaces the installed translator model.
pub fn load_pipeline_with(
    store: &ModelStore,
    src: &Language,
    translate: bool,
    translator_gguf: Option<&Path>,
) -> Result<LoadedPipeline> {
    let Some(pack) = &src.script_pack else {
        return Err(TranslatteError::NoScriptPack(src.code.clone()));
    };
    require_installed(store, DETECTOR_ID)?;
    require_installed(store, pack)?;
    if translate && translator_gguf.is_none() {
        require_installed(store, TRANSLATOR_ID)?;
    }
    let detector = Detector::load(&store.model_file(DETECTOR_ID, "inference.onnx")?)?;
    let recognizer = Recognizer::load(
        &store.model_file(pack, "inference.onnx")?,
        &store.model_file(pack, "dict.txt")?,
    )?;
    let translator = if translate {
        let path = match translator_gguf {
            Some(p) => p.to_path_buf(),
            None => store.model_file(TRANSLATOR_ID, "model.gguf")?,
        };
        Some(BatchTranslator::new(LlamaGenerator::load(&path)?))
    } else {
        None
    };
    Ok(Pipeline::new(detector, recognizer, translator).with_script_pack(pack.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::SAMPLE;
    use crate::manifest::Manifest;

    fn store() -> (tempfile::TempDir, ModelStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(
            Manifest::from_json(SAMPLE).unwrap(),
            dir.path().to_path_buf(),
        );
        (dir, store)
    }

    #[test]
    fn target_only_language_is_rejected() {
        let (_d, s) = store();
        let he = s.manifest().language("he").unwrap();
        assert!(matches!(
            load_pipeline(&s, he, false),
            Err(TranslatteError::NoScriptPack(c)) if c == "he"
        ));
    }

    #[test]
    fn missing_models_are_reported_by_id() {
        let (_d, s) = store();
        let fr = s.manifest().language("fr").unwrap();
        assert!(matches!(
            load_pipeline(&s, fr, true),
            Err(TranslatteError::ModelNotInstalled(id)) if id == DETECTOR_ID
        ));
    }
}
