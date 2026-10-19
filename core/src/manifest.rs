use std::collections::HashSet;

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
        let m: Manifest = serde_json::from_str(s)?;
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
}
