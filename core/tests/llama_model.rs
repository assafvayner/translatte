#![cfg(feature = "llama")]

mod common;

use common::store;
use translatte_core::model_store::ModelStore;
use translatte_core::translate::batch::BatchTranslator;
use translatte_core::translate::llama::LlamaGenerator;
use translatte_core::translate::Translator;

fn translate(store: &ModelStore, blocks: &[&str], src: &str, tgt: &str) -> Vec<Option<String>> {
    let generator =
        LlamaGenerator::load(&store.model_file("translator", "model.gguf").unwrap()).unwrap();
    let mut t = BatchTranslator::new(generator);
    let m = store.manifest();
    let blocks: Vec<String> = blocks.iter().map(|s| s.to_string()).collect();
    let out = t
        .translate(&blocks, m.language(src).unwrap(), m.language(tgt).unwrap())
        .unwrap();
    eprintln!("{src}->{tgt}: {out:?}");
    out
}

fn lower(o: &Option<String>) -> String {
    o.as_deref().unwrap_or_default().to_lowercase()
}

#[test]
fn translates_with_translategemma() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let ja = translate(&store, &["非常口", "禁煙"], "ja", "en");
    assert!(lower(&ja[0]).contains("exit"), "{ja:?}");
    assert!(lower(&ja[1]).contains("smok"), "{ja:?}");

    let fr = translate(
        &store,
        &["Je voudrais un café, s'il vous plaît."],
        "fr",
        "en",
    );
    assert!(lower(&fr[0]).contains("coffee"), "{fr:?}");
}

#[test]
fn two_generators_can_coexist() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let path = store.model_file("translator", "model.gguf").unwrap();
    let first = LlamaGenerator::load(&path).unwrap();
    let second = LlamaGenerator::load(&path).unwrap();
    drop(first);
    let mut t = BatchTranslator::new(second);
    let m = store.manifest();
    let out = t
        .translate(
            &["非常口".to_string()],
            m.language("ja").unwrap(),
            m.language("en").unwrap(),
        )
        .unwrap();
    assert!(lower(&out[0]).contains("exit"), "{out:?}");
}
