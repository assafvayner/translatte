use std::path::{Path, PathBuf};

use bergamot::{PairFiles, PairModel};

/// `TRANSLATTE_PAIR_MODELS` points at a dir with one subdirectory per source language holding the
/// upstream files: `model.*.bin`, `lex.*.bin`, and `vocab.*.spm` or `srcvocab.*.spm` + `trgvocab.*.spm`.
fn load(lang: &str) -> Option<PairModel> {
    let Some(root) = std::env::var_os("TRANSLATTE_PAIR_MODELS") else {
        eprintln!("TRANSLATTE_PAIR_MODELS not set; skipping");
        return None;
    };
    let dir = PathBuf::from(root).join(lang);
    let model = file(&dir, "model.").expect("model file");
    let lex = file(&dir, "lex.").expect("lex file");
    let (src, trg) = match file(&dir, "vocab.") {
        Some(v) => (v.clone(), v),
        None => (
            file(&dir, "srcvocab.").expect("source vocab"),
            file(&dir, "trgvocab.").expect("target vocab"),
        ),
    };
    let files = PairFiles {
        model: &model,
        src_vocab: &src,
        trg_vocab: &trg,
        shortlist: &lex,
    };
    Some(PairModel::load(&files).unwrap())
}

fn file(dir: &Path, prefix: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix))
        })
}

fn translate(model: &mut PairModel, texts: &[&str]) -> Vec<String> {
    let texts: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
    let out = model.translate(&texts).unwrap();
    eprintln!("{out:?}");
    assert_eq!(out.len(), texts.len());
    for (src, tgt) in texts.iter().zip(&out) {
        assert!(!tgt.trim().is_empty(), "{src} translated to nothing");
    }
    out
}

#[test]
fn translates_a_japanese_menu_and_sign() {
    let Some(mut m) = load("ja") else { return };
    let out = translate(
        &mut m,
        &[
            "非常口",
            "本日のおすすめ",
            "鶏の唐揚げ",
            "現金のみ・カード不可",
        ],
    );
    assert!(out[0].to_lowercase().contains("exit"), "{out:?}");
}

#[test]
fn translates_a_chinese_menu() {
    let Some(mut m) = load("zh") else { return };
    let out = translate(&mut m, &["禁止吸烟", "红烧牛肉面", "小笼包", "营业时间"]);
    assert!(
        out.iter()
            .all(|t| t.chars().any(|c| c.is_ascii_alphabetic())),
        "{out:?}"
    );
}

#[test]
fn translates_a_french_menu() {
    let Some(mut m) = load("fr") else { return };
    let out = translate(
        &mut m,
        &["Attention au chien", "Plat du jour", "Soupe à l'oignon"],
    );
    assert!(out[0].to_lowercase().contains("dog"), "{out:?}");
}

#[test]
fn translation_is_deterministic_and_empty_input_is_empty() {
    let Some(mut m) = load("ja") else { return };
    let texts = ["焼き鳥盛り合わせ 5本", "枝豆"];
    assert_eq!(translate(&mut m, &texts), translate(&mut m, &texts));
    assert!(m.translate(&[]).unwrap().is_empty());
}
