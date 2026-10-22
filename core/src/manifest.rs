use std::collections::HashSet;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::{Result, TranslatteError};

pub const DETECTOR_ID: &str = "det";
/// The only language translated into: pair models translate from a source language into it.
pub const TARGET_LANGUAGE: &str = "en";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Core,
    ScriptPack,
    /// Translates one source language into [`TARGET_LANGUAGE`].
    PairModel,
}

/// How a local file is produced from its upstream `source_path` when it is not a verbatim copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Derive {
    /// `PostProcess.character_dict` of a PaddleOCR `inference.yml`, one entry per line.
    PaddleDict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFile {
    pub path: String,
    pub size: u64,
    /// File name in the upstream repo (`ModelEntry::source`).
    pub source_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derive: Option<Derive>,
}

/// Upstream Hugging Face model repo pinned to a commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSource {
    pub repo: String,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub kind: ModelKind,
    pub display_name: String,
    pub source: ModelSource,
    pub files: Vec<ModelFile>,
}

impl ModelEntry {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Language {
    pub code: String,
    pub name: String,
    pub script_pack: Option<String>,
    /// Translates this language into [`TARGET_LANGUAGE`]. A language with a script pack and a pair
    /// model is a source language.
    #[serde(default)]
    pub pair_model: Option<String>,
    #[serde(default)]
    pub rtl: bool,
    /// False for scripts written without spaces between words (zh, ja, th); controls how OCR lines are joined.
    #[serde(default = "default_true")]
    pub spaced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub models: Vec<ModelEntry>,
    pub languages: Vec<Language>,
}

impl Manifest {
    pub fn from_json(s: &str) -> Result<Self> {
        let m: Manifest =
            serde_json::from_str(s).map_err(|e| TranslatteError::Manifest(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    pub fn model(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }

    pub fn language(&self, code: &str) -> Result<&Language> {
        self.languages
            .iter()
            .find(|l| l.code == code)
            .ok_or_else(|| TranslatteError::UnknownLanguage(code.to_string()))
    }

    fn validate(&self) -> Result<()> {
        let mut ids = HashSet::new();
        for m in &self.models {
            if !ids.insert(m.id.as_str()) {
                return Err(TranslatteError::Manifest(format!(
                    "duplicate model id {}",
                    m.id
                )));
            }
        }
        for m in &self.models {
            validate_source(m)?;
            if m.files.is_empty() {
                return Err(TranslatteError::Manifest(format!(
                    "model {} has no files",
                    m.id
                )));
            }
            let prefix = format!("{}/", m.id);
            let mut names = HashSet::new();
            for f in &m.files {
                let path = Path::new(&f.path);
                let safe = !path.is_absolute()
                    && path.components().all(|c| matches!(c, Component::Normal(_)));
                if !safe || !f.path.starts_with(&prefix) {
                    return Err(TranslatteError::Manifest(format!(
                        "invalid file path {} for model {}",
                        f.path, m.id
                    )));
                }
                if !is_safe_repo_path(&f.source_path) {
                    return Err(TranslatteError::Manifest(format!(
                        "invalid source path {} for {}",
                        f.source_path, f.path
                    )));
                }
                let name = path.file_name().map(|n| n.to_owned());
                if !names.insert(name) {
                    return Err(TranslatteError::Manifest(format!(
                        "duplicate file name in model {}: {}",
                        m.id, f.path
                    )));
                }
            }
        }
        let mut codes = HashSet::new();
        for l in &self.languages {
            if !codes.insert(l.code.as_str()) {
                return Err(TranslatteError::Manifest(format!(
                    "duplicate language code {}",
                    l.code
                )));
            }
        }
        match self.model(DETECTOR_ID) {
            Some(m) if m.kind == ModelKind::Core => {}
            _ => {
                return Err(TranslatteError::Manifest(format!(
                    "missing core model {DETECTOR_ID}"
                )))
            }
        }
        for l in &self.languages {
            if let Some(pack) = &l.script_pack {
                self.expect_model(pack, ModelKind::ScriptPack, "script pack", &l.code)?;
            }
            if let Some(pair) = &l.pair_model {
                if l.script_pack.is_none() {
                    return Err(TranslatteError::Manifest(format!(
                        "language {} has a pair model but no script pack to read it with",
                        l.code
                    )));
                }
                self.expect_model(pair, ModelKind::PairModel, "pair model", &l.code)?;
            }
        }
        for m in self
            .models
            .iter()
            .filter(|m| m.kind == ModelKind::PairModel)
        {
            validate_pair_files(m)?;
        }
        Ok(())
    }

    fn expect_model(&self, id: &str, kind: ModelKind, what: &str, language: &str) -> Result<()> {
        match self.model(id) {
            Some(m) if m.kind == kind => Ok(()),
            _ => Err(TranslatteError::Manifest(format!(
                "language {language} references unknown {what} {id}"
            ))),
        }
    }
}

fn is_safe_repo_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && p.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

fn validate_source(m: &ModelEntry) -> Result<()> {
    let ModelSource { repo, revision } = &m.source;
    let repo_ok = match repo.split_once('/') {
        Some((owner, name)) => {
            let ok = |s: &str| {
                !s.is_empty()
                    && s != "."
                    && s != ".."
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            };
            ok(owner) && ok(name)
        }
        None => false,
    };
    if !repo_ok {
        return Err(TranslatteError::Manifest(format!(
            "invalid source repo {repo:?} for model {}",
            m.id
        )));
    }
    let rev_ok = revision.len() == 40
        && revision
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if !rev_ok {
        return Err(TranslatteError::Manifest(format!(
            "source revision for model {} must be a 40-hex commit sha, got {revision:?}",
            m.id
        )));
    }
    Ok(())
}

/// A pair model needs `model.bin`, `lex.bin` and either one shared `vocab.spm` or separate
/// `srcvocab.spm` and `trgvocab.spm`.
fn validate_pair_files(m: &ModelEntry) -> Result<()> {
    let has = |name: &str| {
        m.files
            .iter()
            .any(|f| Path::new(&f.path).file_name().is_some_and(|n| n == name))
    };
    let vocab = has("vocab.spm") != (has("srcvocab.spm") && has("trgvocab.spm"));
    if has("model.bin") && has("lex.bin") && vocab {
        Ok(())
    } else {
        Err(TranslatteError::Manifest(format!(
            "pair model {} needs model.bin, lex.bin and vocab.spm (or srcvocab.spm and trgvocab.spm)",
            m.id
        )))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const SAMPLE: &str = r#"{
      "version": 1,
      "models": [
        {"id": "det", "kind": "core", "display_name": "Text detector",
         "source": {"repo": "acme/det", "revision": "0123456789abcdef0123456789abcdef01234567"},
         "files": [{"path": "det/inference.onnx", "size": 3, "source_path": "inference.onnx"}]},
        {"id": "rec-latin", "kind": "script_pack", "display_name": "Latin script",
         "source": {"repo": "acme/latin_rec", "revision": "0123456789abcdef0123456789abcdef01234567"},
         "files": [
           {"path": "rec-latin/inference.onnx", "size": 3, "source_path": "inference.onnx"},
           {"path": "rec-latin/dict.txt", "size": 3, "source_path": "inference.yml", "derive": "paddle_dict"}
         ]},
        {"id": "pair-fr-en", "kind": "pair_model", "display_name": "French → English",
         "source": {"repo": "acme/pairs", "revision": "0123456789abcdef0123456789abcdef01234567"},
         "files": [
           {"path": "pair-fr-en/lex.bin", "size": 3, "source_path": "fr-en/lex.50.50.fren.s2t.bin"},
           {"path": "pair-fr-en/model.bin", "size": 3, "source_path": "fr-en/model.fren.intgemm.alphas.bin"},
           {"path": "pair-fr-en/vocab.spm", "size": 3, "source_path": "fr-en/vocab.fren.spm"}
         ]},
        {"id": "pair-de-en", "kind": "pair_model", "display_name": "German → English",
         "source": {"repo": "acme/pairs", "revision": "0123456789abcdef0123456789abcdef01234567"},
         "files": [
           {"path": "pair-de-en/lex.bin", "size": 3, "source_path": "de-en/lex.50.50.deen.s2t.bin"},
           {"path": "pair-de-en/model.bin", "size": 3, "source_path": "de-en/model.deen.intgemm.alphas.bin"},
           {"path": "pair-de-en/vocab.spm", "size": 3, "source_path": "de-en/vocab.deen.spm"}
         ]}
      ],
      "languages": [
        {"code": "en", "name": "English", "script_pack": null},
        {"code": "fr", "name": "French", "script_pack": "rec-latin", "pair_model": "pair-fr-en"},
        {"code": "de", "name": "German", "script_pack": "rec-latin", "pair_model": "pair-de-en"}
      ]
    }"#;

    #[test]
    fn parses_sample_manifest() {
        let m = Manifest::from_json(SAMPLE).unwrap();
        assert_eq!(m.models.len(), 4);
        assert_eq!(m.model("rec-latin").unwrap().kind, ModelKind::ScriptPack);
        assert_eq!(m.model("rec-latin").unwrap().total_size(), 6);
        let en = m.language(TARGET_LANGUAGE).unwrap();
        assert!(!en.rtl);
        assert!(en.spaced);
        assert_eq!(en.script_pack, None);
        assert_eq!(en.pair_model, None);
    }

    #[test]
    fn parses_pair_models() {
        let m = Manifest::from_json(SAMPLE).unwrap();
        assert_eq!(m.model("pair-fr-en").unwrap().kind, ModelKind::PairModel);
        assert_eq!(
            m.language("fr").unwrap().pair_model.as_deref(),
            Some("pair-fr-en")
        );
    }

    #[test]
    fn accepts_a_pair_model_with_separate_vocabularies() {
        let two = SAMPLE.replace(
            r#"{"path": "pair-fr-en/vocab.spm", "size": 3, "source_path": "fr-en/vocab.fren.spm"}"#,
            r#"{"path": "pair-fr-en/srcvocab.spm", "size": 3, "source_path": "fr-en/srcvocab.fren.spm"},
               {"path": "pair-fr-en/trgvocab.spm", "size": 3, "source_path": "fr-en/trgvocab.fren.spm"}"#,
        );
        Manifest::from_json(&two).unwrap();
    }

    #[test]
    fn rejects_pair_models_missing_files() {
        for name in ["lex.bin", "model.bin", "vocab.spm"] {
            assert_rejected(SAMPLE.replace(&format!("pair-fr-en/{name}"), "pair-fr-en/other.bin"));
        }
        assert_rejected(SAMPLE.replace(
            r#"{"path": "pair-fr-en/vocab.spm", "size": 3, "source_path": "fr-en/vocab.fren.spm"}"#,
            r#"{"path": "pair-fr-en/srcvocab.spm", "size": 3, "source_path": "fr-en/srcvocab.fren.spm"}"#,
        ));
    }

    #[test]
    fn rejects_bad_pair_model_references() {
        assert_rejected(SAMPLE.replace(
            r#""pair_model": "pair-fr-en""#,
            r#""pair_model": "pair-xx-en""#,
        ));
        assert_rejected(SAMPLE.replace(
            r#""pair_model": "pair-fr-en""#,
            r#""pair_model": "rec-latin""#,
        ));
        assert_rejected(SAMPLE.replace(
            r#""script_pack": "rec-latin", "pair_model": "pair-fr-en""#,
            r#""script_pack": null, "pair_model": "pair-fr-en""#,
        ));
    }

    #[test]
    fn parses_sources_and_derive() {
        let m = Manifest::from_json(SAMPLE).unwrap();
        let latin = m.model("rec-latin").unwrap();
        assert_eq!(latin.source.repo, "acme/latin_rec");
        assert_eq!(latin.files[0].source_path, "inference.onnx");
        assert_eq!(latin.files[0].derive, None);
        assert_eq!(latin.files[1].source_path, "inference.yml");
        assert_eq!(latin.files[1].derive, Some(Derive::PaddleDict));
    }

    #[test]
    fn rejects_missing_or_invalid_source() {
        assert_rejected(SAMPLE.replace(
            r#""source": {"repo": "acme/det", "revision": "0123456789abcdef0123456789abcdef01234567"},"#,
            "",
        ));
        assert_rejected(SAMPLE.replace("acme/det", "acme"));
        assert_rejected(SAMPLE.replace("acme/det", "acme/det/x"));
        assert_rejected(SAMPLE.replace("acme/det", "../det"));
        assert_rejected(SAMPLE.replacen("0123456789abcdef0123456789abcdef01234567", "main", 1));
        assert_rejected(SAMPLE.replacen(
            "0123456789abcdef0123456789abcdef01234567",
            "0123456789ABCDEF0123456789ABCDEF01234567",
            1,
        ));
        assert_rejected(SAMPLE.replacen(
            "0123456789abcdef0123456789abcdef01234567",
            "0123456789abcdef0123456789abcdef0123456",
            1,
        ));
    }

    #[test]
    fn rejects_missing_or_unsafe_source_path() {
        assert_rejected(SAMPLE.replace(r#", "source_path": "inference.onnx"}]}"#, "}]}"));
        assert_rejected(SAMPLE.replace(
            r#""source_path": "inference.yml""#,
            r#""source_path": "../x.yml""#,
        ));
        assert_rejected(SAMPLE.replace(
            r#""source_path": "inference.yml""#,
            r#""source_path": "/x.yml""#,
        ));
        assert_rejected(
            SAMPLE.replace(r#""source_path": "inference.yml""#, r#""source_path": """#),
        );
        assert_rejected(SAMPLE.replace(r#""derive": "paddle_dict""#, r#""derive": "nope""#));
    }

    #[test]
    fn unknown_language_is_an_error() {
        let m = Manifest::from_json(SAMPLE).unwrap();
        assert!(matches!(
            m.language("xx"),
            Err(TranslatteError::UnknownLanguage(_))
        ));
    }

    #[test]
    fn rejects_language_referencing_missing_pack() {
        let bad = SAMPLE.replace(
            r#""script_pack": "rec-latin""#,
            r#""script_pack": "rec-nope""#,
        );
        assert!(matches!(
            Manifest::from_json(&bad),
            Err(TranslatteError::Manifest(_))
        ));
    }

    #[test]
    fn rejects_missing_core_model() {
        assert_rejected(SAMPLE.replace(
            r#""id": "det", "kind": "core""#,
            r#""id": "det", "kind": "script_pack""#,
        ));
    }

    fn assert_rejected(bad: String) {
        assert!(matches!(
            Manifest::from_json(&bad),
            Err(TranslatteError::Manifest(_))
        ));
    }

    #[test]
    fn rejects_unsafe_or_misplaced_paths() {
        assert_rejected(SAMPLE.replace("det/inference.onnx", "/det/inference.onnx"));
        assert_rejected(SAMPLE.replace("det/inference.onnx", "det/../x/inference.onnx"));
        assert_rejected(SAMPLE.replace("det/inference.onnx", "other/inference.onnx"));
    }

    #[test]
    fn rejects_model_without_files() {
        let bad = SAMPLE.replace(
            r#""files": [{"path": "det/inference.onnx", "size": 3, "source_path": "inference.onnx"}]"#,
            r#""files": []"#,
        );
        assert_rejected(bad);
    }

    #[test]
    fn rejects_duplicate_file_names_in_model() {
        assert_rejected(SAMPLE.replace("rec-latin/dict.txt", "rec-latin/inference.onnx"));
    }

    #[test]
    fn rejects_duplicate_language_codes() {
        assert_rejected(SAMPLE.replace(r#""code": "de""#, r#""code": "fr""#));
    }

    #[test]
    fn repo_manifest_is_valid() {
        let s = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../models/manifest.json"
        ))
        .unwrap();
        let m = Manifest::from_json(&s).unwrap();
        assert!(m.model("rec-cjk").is_some());
        assert!(m.model("rec-en").is_none());
        assert!(m.model("translator").is_none());
        let en = m.language(TARGET_LANGUAGE).unwrap();
        assert_eq!(
            (en.script_pack.as_deref(), en.pair_model.as_deref()),
            (None, None)
        );
        let sources: Vec<_> = m
            .languages
            .iter()
            .filter(|l| l.code != TARGET_LANGUAGE)
            .collect();
        assert_eq!(sources.len(), 52);
        for l in &sources {
            assert!(
                l.script_pack.is_some() && l.pair_model.is_some(),
                "{}",
                l.code
            );
        }
        for gone in [
            "he", "vi", "az", "el", "kk", "mn", "bo", "km", "my", "bn", "gu",
        ] {
            assert!(m.language(gone).is_err(), "{gone} has no OCR script pack");
        }
        assert_eq!(
            m.language("yue").unwrap().pair_model,
            m.language("zh-TW").unwrap().pair_model
        );
        let hant = m.model("pair-zh-Hant-en").unwrap();
        let names: Vec<_> = hant.files.iter().map(|f| f.path.as_str()).collect();
        assert!(names.contains(&"pair-zh-Hant-en/srcvocab.spm"), "{names:?}");
        assert!(names.contains(&"pair-zh-Hant-en/trgvocab.spm"), "{names:?}");
        let fr = m.model("pair-fr-en").unwrap();
        assert_eq!(fr.source.repo, "assafvayner/translatte-pair-models");
        assert!(fr
            .files
            .iter()
            .any(|f| f.source_path == "fr-en/model.fren.intgemm.alphas.bin"));
        for model in m.models.iter().filter(|m| m.kind == ModelKind::ScriptPack) {
            let dict = model
                .files
                .iter()
                .find(|f| f.path.ends_with("/dict.txt"))
                .unwrap();
            assert_eq!(dict.derive, Some(Derive::PaddleDict), "{}", model.id);
            assert_eq!(dict.source_path, "inference.yml");
        }
    }
}
