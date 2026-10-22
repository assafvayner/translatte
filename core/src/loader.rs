use crate::manifest::{Language, DETECTOR_ID};
use crate::model_store::ModelStore;
use crate::ocr::detector::Detector;
use crate::ocr::recognizer::Recognizer;
use crate::ocr::{load_detector, load_recognizer, Precision};
use crate::pipeline::Pipeline;
use crate::translate::cache::CachingTranslator;
use crate::translate::pair::{PairTranslator, StoreLoader};
use crate::{Result, TranslatteError};

pub type LoadedPipeline =
    Pipeline<Detector, Recognizer, CachingTranslator<PairTranslator<StoreLoader>>>;

/// Loads the detector and the recognizer for `src`'s script pack, fp32 for photos and fp16 for
/// live frames, loaded by the first live frame (or `precision` for both, loaded now), and, when `translate` is set, a translator that loads
/// `src`'s pair model on first use (or `warm`) and swaps it when another language with the same
/// script pack is translated, behind a cache of recent translations (see [`CachingTranslator`]).
pub fn load_pipeline(
    store: &ModelStore,
    src: &Language,
    translate: bool,
    precision: Option<Precision>,
) -> Result<LoadedPipeline> {
    let Some(pack) = &src.script_pack else {
        return Err(TranslatteError::NoScriptPack(src.code.clone()));
    };
    store.require_installed(DETECTOR_ID)?;
    store.require_installed(pack)?;
    if translate {
        let pair = src
            .pair_model
            .as_ref()
            .ok_or_else(|| TranslatteError::NoPairModel(src.code.clone()))?;
        store.require_installed(pair)?;
    }
    let photo = precision.unwrap_or(Precision::Fp32);
    let translator = translate
        .then(|| CachingTranslator::new(PairTranslator::new(StoreLoader::new(store.clone()))));
    let pipeline = Pipeline::new(
        load_detector(store, photo)?,
        load_recognizer(store, pack, photo)?,
        translator,
    )
    .with_script_pack(pack.as_str())
    .with_precision(photo);
    if precision.is_some() {
        return Ok(pipeline);
    }
    let (store, pack) = (store.clone(), pack.clone());
    Ok(pipeline.with_live_ocr(Precision::Fp16, move || {
        Ok((
            load_detector(&store, Precision::Fp16)?,
            load_recognizer(&store, &pack, Precision::Fp16)?,
        ))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::SAMPLE;
    use crate::manifest::{Manifest, TARGET_LANGUAGE};

    fn store() -> (tempfile::TempDir, ModelStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(
            Manifest::from_json(SAMPLE).unwrap(),
            dir.path().to_path_buf(),
        );
        (dir, store)
    }

    fn install(store: &ModelStore, id: &str) {
        for f in &store.manifest().model(id).unwrap().files {
            let p = store.root().join(&f.path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "abc").unwrap();
        }
    }

    #[test]
    fn the_target_language_is_not_a_source() {
        let (_d, s) = store();
        let en = s.manifest().language(TARGET_LANGUAGE).unwrap();
        assert!(matches!(
            load_pipeline(&s, en, false, None),
            Err(TranslatteError::NoScriptPack(c)) if c == "en"
        ));
    }

    #[test]
    fn missing_models_are_reported_by_id() {
        let (_d, s) = store();
        let fr = s.manifest().language("fr").unwrap();
        assert!(matches!(
            load_pipeline(&s, fr, true, None),
            Err(TranslatteError::ModelNotInstalled(id)) if id == DETECTOR_ID
        ));
        install(&s, "det");
        install(&s, "rec-latin");
        assert!(matches!(
            load_pipeline(&s, fr, true, None),
            Err(TranslatteError::ModelNotInstalled(id)) if id == "pair-fr-en"
        ));
    }
}
