#![cfg(feature = "pair")]

mod common;

use common::store;
use translatte_core::manifest::TARGET_LANGUAGE;
use translatte_core::translate::pair::{PairTranslator, StoreLoader};
use translatte_core::translate::Translator;

/// Translates `blocks` from `src` with the installed pair model; None without TRANSLATTE_MODELS.
fn translate(src: &str, blocks: &[&str]) -> Option<Vec<String>> {
    let Some(store) = store() else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return None;
    };
    let manifest = store.manifest().clone();
    let mut t = PairTranslator::new(StoreLoader::new(store));
    let blocks: Vec<String> = blocks.iter().map(|b| b.to_string()).collect();
    let out = t
        .translate(
            &blocks,
            manifest.language(src).unwrap(),
            manifest.language(TARGET_LANGUAGE).unwrap(),
        )
        .unwrap();
    eprintln!("{src}: {out:?}");
    let out: Vec<String> = out.into_iter().map(|t| t.expect("translated")).collect();
    for (b, t) in blocks.iter().zip(&out) {
        assert!(!t.trim().is_empty(), "{b} translated to nothing");
    }
    Some(out)
}

#[test]
fn translates_a_japanese_menu() {
    let Some(out) = translate(
        "ja",
        &["非常口", "本日のおすすめ", "揚げ出し豆腐", "生ビール (中)"],
    ) else {
        return;
    };
    assert!(out[0].to_lowercase().contains("exit"), "{out:?}");
}

#[test]
fn translates_a_chinese_menu() {
    translate("zh", &["禁止吸烟", "红烧牛肉面", "小笼包", "营业时间"]);
}

#[test]
fn translates_traditional_chinese_with_two_vocabularies() {
    translate("zh-TW", &["出口", "禁止吸菸", "營業時間", "牛肉麵"]);
}

#[test]
fn translates_a_french_menu() {
    let Some(out) = translate(
        "fr",
        &["Attention au chien", "Plat du jour", "Soupe à l'oignon"],
    ) else {
        return;
    };
    assert!(out[0].to_lowercase().contains("dog"), "{out:?}");
}
