#![cfg(feature = "llama")]

mod common;

use common::store;
use translatte_core::model_store::ModelStore;
use translatte_core::translate::batch::BatchTranslator;
use translatte_core::translate::llama::LlamaGenerator;
use translatte_core::translate::prompt::{build_prompt, Prompt};
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
fn translates_with_hy_mt2() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let zh = translate(&store, &["安全出口", "禁止吸烟"], "zh", "en");
    assert!(lower(&zh[0]).contains("exit"), "{zh:?}");
    assert!(lower(&zh[1]).contains("smok"), "{zh:?}");

    // 非常口's translation is checked by `translates_emergency_exit`, a known failure.
    let ja = translate(&store, &["非常口", "禁煙"], "ja", "en");
    assert!(ja[0].as_deref().is_some_and(|t| !t.is_empty()), "{ja:?}");
    assert!(lower(&ja[1]).contains("smok"), "{ja:?}");

    let fr = translate(
        &store,
        &["Je voudrais un café, s'il vous plaît."],
        "fr",
        "en",
    );
    assert!(lower(&fr[0]).contains("coffee"), "{fr:?}");
}

/// Known failure: Hy-MT2-1.8B Q4_K_M translates 非常口 (emergency exit) as "Very mouth", alone or
/// in a numbered list; Q8_0 gets it right. Run with `--ignored` to check a new quantization.
#[test]
#[ignore = "Hy-MT2 Q4_K_M translates 非常口 as \"Very mouth\""]
fn translates_emergency_exit() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let ja = translate(&store, &["非常口"], "ja", "en");
    assert!(lower(&ja[0]).contains("exit"), "{ja:?}");
}

#[test]
fn generation_stops_at_end_of_turn() {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let m = store.manifest();
    let path = store.model_file("translator", "model.gguf").unwrap();
    let mut g = LlamaGenerator::load(&path).unwrap();
    // Without the persistent pool decode is several times slower on phones; fail loudly instead.
    assert!(g.threadpool_attached());
    let p = build_prompt(m.language("en").unwrap(), m.language("de").unwrap(), "Exit");
    let out = g.generate(&p, 64).unwrap();
    assert!(!out.truncated, "{:?}", out.text);
    assert!(!out.text.contains('<'), "{:?}", out.text);
    assert!(
        out.text.to_lowercase().contains("ausgang"),
        "{:?}",
        out.text
    );
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
    assert!(out[0].as_deref().is_some_and(|t| !t.is_empty()), "{out:?}");
}

/// Prefix reuse once a sequence has grown past the sliding attention window. Hy-MT2 has no SWA, so
/// this runs only with TRANSLATTE_SWA_GGUF pointing at a model that has one (e.g. a Gemma 3 GGUF).
#[test]
fn prefix_cache_past_sliding_window_matches_uncached_generation() {
    let Some(path) = std::env::var_os("TRANSLATTE_SWA_GGUF") else {
        eprintln!("TRANSLATTE_SWA_GGUF not set; skipping");
        return;
    };
    let prompt = |text: &str| Prompt {
        prefix: "Translate the following text into English:\n\n".into(),
        text: text.into(),
        suffix: "\n\nEnglish:".into(),
    };
    // Well over Gemma 3's 1024-token window.
    let long = "本日のおすすめは焼き鳥と枝豆です。".repeat(150);
    let prompts = [
        prompt("非常口"),
        prompt(&long),
        prompt("禁煙"),
        prompt(&long),
        prompt("出口"),
    ];
    let run = |cache: bool| {
        let mut g = LlamaGenerator::load(path.as_ref())
            .unwrap()
            .with_prefix_cache(cache);
        let mut reused = Vec::new();
        let out = prompts
            .iter()
            .map(|p| {
                let text = g.generate(p, 16).unwrap().text;
                reused.push(g.reused_tokens());
                text
            })
            .collect::<Vec<_>>();
        (out, reused)
    };
    let (uncached, _) = run(false);
    let (cached, reused) = run(true);
    // The short prompts share the prefix with the previous call; after a long one nothing is reused.
    assert!(reused[1] > 0, "{reused:?}");
    assert_eq!(reused[2], 0, "{reused:?}");
    assert_eq!(reused[4], 0, "{reused:?}");
    assert_eq!(cached, uncached);
}
