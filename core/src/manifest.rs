use std::collections::HashSet;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::{Result, TranslatteError};

pub const DETECTOR_ID: &str = "det";
pub const TRANSLATOR_ID: &str = "translator";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Core,
    ScriptPack,
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
    #[serde(default)]
    pub rtl: bool,
    /// False for scripts written without spaces between words (zh, ja, th); controls how OCR lines are joined.
    #[serde(default = "default_true")]
    pub spaced: bool,
    /// English name the translator's prompt uses, when it differs from the display `name`.
    #[serde(default)]
    pub prompt_name: Option<String>,
    /// Chinese name the translator's Chinese instruction uses.
    #[serde(default)]
    pub prompt_name_zh: Option<String>,
    /// Pairs involving this language get the Chinese instruction (Chinese varieties, and languages
    /// the model only translates into reliably when instructed in Chinese).
    #[serde(default)]
    pub zh_instruction: bool,
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
        for core in [DETECTOR_ID, TRANSLATOR_ID] {
            match self.model(core) {
                Some(m) if m.kind == ModelKind::Core => {}
                _ => {
                    return Err(TranslatteError::Manifest(format!(
                        "missing core model {core}"
                    )))
                }
            }
        }
        for l in &self.languages {
            if let Some(pack) = &l.script_pack {
                match self.model(pack) {
                    Some(m) if m.kind == ModelKind::ScriptPack => {}
                    _ => {
                        return Err(TranslatteError::Manifest(format!(
                            "language {} references unknown script pack {pack}",
                            l.code
                        )))
                    }
                }
            }
        }
        Ok(())
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const SAMPLE: &str = r#"{
      "version": 1,
      "models": [
        {"id": "det", "kind": "core", "display_name": "Text detector",
         "source": {"repo": "acme/det", "revision": "0123456789abcdef0123456789abcdef01234567"},
         "files": [{"path": "det/inference.onnx", "size": 3, "source_path": "inference.onnx"}]},
        {"id": "translator", "kind": "core", "display_name": "Translator",
         "source": {"repo": "acme/translator-GGUF", "revision": "0123456789abcdef0123456789abcdef01234567"},
         "files": [{"path": "translator/model.gguf", "size": 3, "source_path": "translator.Q4_K_M.gguf"}]},
        {"id": "rec-latin", "kind": "script_pack", "display_name": "Latin script",
         "source": {"repo": "acme/latin_rec", "revision": "0123456789abcdef0123456789abcdef01234567"},
         "files": [
           {"path": "rec-latin/inference.onnx", "size": 3, "source_path": "inference.onnx"},
           {"path": "rec-latin/dict.txt", "size": 3, "source_path": "inference.yml", "derive": "paddle_dict"}
         ]}
      ],
      "languages": [
        {"code": "fr", "name": "French", "script_pack": "rec-latin"},
        {"code": "he", "name": "Hebrew", "script_pack": null, "rtl": true}
      ]
    }"#;

    #[test]
    fn parses_sample_manifest() {
        let m = Manifest::from_json(SAMPLE).unwrap();
        assert_eq!(m.models.len(), 3);
        assert_eq!(m.model("rec-latin").unwrap().kind, ModelKind::ScriptPack);
        assert_eq!(m.model("rec-latin").unwrap().total_size(), 6);
        let he = m.language("he").unwrap();
        assert!(he.rtl);
        assert!(he.spaced);
        assert_eq!(he.script_pack, None);
        assert_eq!(he.prompt_name, None);
        assert_eq!(he.prompt_name_zh, None);
        assert!(!he.zh_instruction);
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
        let bad = SAMPLE.replace(r#""id": "translator""#, r#""id": "something-else""#);
        assert!(matches!(
            Manifest::from_json(&bad),
            Err(TranslatteError::Manifest(_))
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
        assert_rejected(SAMPLE.replace(r#""code": "he""#, r#""code": "fr""#));
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
        assert!(m.language("ja").unwrap().script_pack.is_some());
        assert_eq!(
            m.language("zh-TW").unwrap().prompt_name.as_deref(),
            Some("Traditional Chinese")
        );
        assert_eq!(m.language("ja").unwrap().prompt_name, None);
        for l in &m.languages {
            assert!(l.prompt_name_zh.is_some(), "{} has no Chinese name", l.code);
        }
        assert!(m.language("zh").unwrap().zh_instruction);
        assert!(!m.language("ja").unwrap().zh_instruction);
        let translator = m.model("translator").unwrap();
        assert_eq!(translator.source.repo, "tencent/Hy-MT2-1.8B-GGUF");
        assert_eq!(translator.files[0].source_path, "Hy-MT2-1.8B-Q4_K_M.gguf");
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
