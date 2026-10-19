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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub kind: ModelKind,
    pub display_name: String,
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
    /// Name TranslateGemma's template uses for this language, when it differs from the display `name`.
    #[serde(default)]
    pub prompt_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub base_url: String,
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

    pub fn url_for(&self, file: &ModelFile) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), file.path)
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const SAMPLE: &str = r#"{
      "version": 1,
      "base_url": "https://example.com/repo/resolve/abc/",
      "models": [
        {"id": "det", "kind": "core", "display_name": "Text detector",
         "files": [{"path": "det/inference.onnx", "size": 3, "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}]},
        {"id": "translator", "kind": "core", "display_name": "Translator",
         "files": [{"path": "translator/model.gguf", "size": 3, "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}]},
        {"id": "rec-latin", "kind": "script_pack", "display_name": "Latin script",
         "files": [
           {"path": "rec-latin/inference.onnx", "size": 3, "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"},
           {"path": "rec-latin/dict.txt", "size": 3, "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}
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
    }

    #[test]
    fn url_for_joins_base_and_path() {
        let m = Manifest::from_json(SAMPLE).unwrap();
        let f = &m.model("det").unwrap().files[0];
        assert_eq!(
            m.url_for(f),
            "https://example.com/repo/resolve/abc/det/inference.onnx"
        );
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
            r#""files": [{"path": "det/inference.onnx", "size": 3, "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}]"#,
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
            Some("Chinese")
        );
        assert_eq!(m.language("ja").unwrap().prompt_name, None);
    }
}
