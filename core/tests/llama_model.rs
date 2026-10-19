#![cfg(feature = "llama")]

mod common;

use common::store;
use translatte_core::model_store::ModelStore;
use translatte_core::translate::batch::BatchTranslator;
use translatte_core::translate::llama::LlamaGenerator;
use translatte_core::translate::prompt::build_prompt;
use translatte_core::translate::{TextGenerator, Translator};

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
fn prefix_cache_matches_uncached_generation() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let m = store.manifest();
    let prompts: Vec<_> = [
        ("ja", "en", "非常口"),
        ("ja", "en", "非常口はこちらです"),
        ("ko", "en", "1. 출구\n2. 화장실"),
        ("ko", "en", "1. 출구\n2. 화장실"),
        ("ko", "fr", "출구"),
        ("ja", "en", "禁煙"),
    ]
    .iter()
    .map(|(s, t, text)| build_prompt(m.language(s).unwrap(), m.language(t).unwrap(), text))
    .collect();
    let path = store.model_file("translator", "model.gguf").unwrap();
    let run = |cache: bool| {
        let mut g = LlamaGenerator::load(&path)
            .unwrap()
            .with_prefix_cache(cache);
        prompts
            .iter()
            .map(|p| g.generate(p, 64).unwrap().text)
            .collect::<Vec<_>>()
    };
    let uncached = run(false);
    let cached = run(true);
    eprintln!("{cached:?}");
    assert_eq!(cached, uncached);
}

#[test]
fn warm_prefix_is_reused_and_matches_uncached_generation() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let m = store.manifest();
    let pairs = [("ko", "en", "1. 출구\n2. 화장실"), ("ja", "en", "非常口")];
    let path = store.model_file("translator", "model.gguf").unwrap();
    let mut uncached = LlamaGenerator::load(&path)
        .unwrap()
        .with_prefix_cache(false);
    let mut cached = LlamaGenerator::load(&path).unwrap().with_prefix_cache(true);
    for (s, t, text) in pairs {
        let p = build_prompt(m.language(s).unwrap(), m.language(t).unwrap(), text);
        uncached.warm(&p.prefix).unwrap();
        cached.warm(&p.prefix).unwrap();
        let warmed = cached.cached_tokens();
        assert!(warmed > 0);
        let want = uncached.generate(&p, 64).unwrap().text;
        let got = cached.generate(&p, 64).unwrap().text;
        assert_eq!(got, want, "{s}->{t}");
        assert_eq!(uncached.reused_tokens(), 0);
        assert_eq!(cached.reused_tokens(), warmed, "{s}->{t}");
    }
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
