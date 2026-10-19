use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::manifest::{Language, Manifest, ModelEntry, ModelKind, DETECTOR_ID, TRANSLATOR_ID};
use crate::{Result, TranslatteError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallState {
    NotInstalled,
    Installed,
    /// Some files are missing or have the wrong size.
    Corrupt,
}

pub struct ModelStore {
    manifest: Manifest,
    root: PathBuf,
}

impl ModelStore {
    pub fn new(manifest: Manifest, root: PathBuf) -> Self {
        Self { manifest, root }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn entry(&self, id: &str) -> Result<&ModelEntry> {
        self.manifest
            .model(id)
            .ok_or_else(|| TranslatteError::Manifest(format!("unknown model {id}")))
    }

    pub fn state(&self, id: &str) -> Result<InstallState> {
        let entry = self.entry(id)?;
        let mut present = 0;
        let mut size_ok = true;
        for f in &entry.files {
            match std::fs::metadata(self.root.join(&f.path)) {
                Ok(meta) => {
                    present += 1;
                    size_ok &= meta.len() == f.size;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(match present {
            0 => InstallState::NotInstalled,
            n if n == entry.files.len() && size_ok => InstallState::Installed,
            _ => InstallState::Corrupt,
        })
    }

    pub fn verify(&self, id: &str) -> Result<bool> {
        for f in &self.entry(id)?.files {
            let mut file = match File::open(self.root.join(&f.path)) {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(e.into()),
            };
            let mut hasher = Sha256::new();
            io::copy(&mut file, &mut hasher)?;
            if !hex::encode(hasher.finalize()).eq_ignore_ascii_case(&f.sha256) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let entry = self.entry(id)?;
        if entry.kind == ModelKind::Core {
            return Err(TranslatteError::CoreModelDelete(id.to_string()));
        }
        for f in &entry.files {
            match std::fs::remove_file(self.root.join(&f.path)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        // Best effort: the directory may hold unrelated files we shouldn't touch.
        let _ = std::fs::remove_dir(self.root.join(id));
        Ok(())
    }

    pub fn core_installed(&self) -> Result<bool> {
        Ok(self.state(DETECTOR_ID)? == InstallState::Installed
            && self.state(TRANSLATOR_ID)? == InstallState::Installed)
    }

    pub fn available_source_languages(&self) -> Result<Vec<&Language>> {
        if !self.core_installed()? {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for l in &self.manifest.languages {
            if let Some(pack) = &l.script_pack {
                if self.state(pack)? == InstallState::Installed {
                    out.push(l);
                }
            }
        }
        Ok(out)
    }

    pub fn target_languages(&self) -> Vec<&Language> {
        self.manifest.languages.iter().collect()
    }

    /// Model files are addressed by their file name inside the model's directory, e.g. ("rec-latin", "dict.txt").
    pub fn model_file(&self, id: &str, file_name: &str) -> Result<PathBuf> {
        let entry = self.entry(id)?;
        entry
            .files
            .iter()
            .find(|f| {
                Path::new(&f.path)
                    .file_name()
                    .is_some_and(|n| n == file_name)
            })
            .map(|f| self.root.join(&f.path))
            .ok_or_else(|| TranslatteError::Manifest(format!("model {id} has no file {file_name}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::SAMPLE;

    fn store() -> (tempfile::TempDir, ModelStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(
            Manifest::from_json(SAMPLE).unwrap(),
            dir.path().to_path_buf(),
        );
        (dir, store)
    }

    fn write(store: &ModelStore, rel: &str, contents: &str) {
        let p = store.root().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }

    fn install_core(store: &ModelStore) {
        write(store, "det/inference.onnx", "abc");
        write(store, "translator/model.gguf", "abc");
    }

    #[test]
    fn state_reflects_files_on_disk() {
        let (_d, s) = store();
        assert_eq!(s.state("rec-latin").unwrap(), InstallState::NotInstalled);
        write(&s, "rec-latin/inference.onnx", "abc");
        assert_eq!(s.state("rec-latin").unwrap(), InstallState::Corrupt);
        write(&s, "rec-latin/dict.txt", "abcd");
        assert_eq!(s.state("rec-latin").unwrap(), InstallState::Corrupt);
        write(&s, "rec-latin/dict.txt", "abc");
        assert_eq!(s.state("rec-latin").unwrap(), InstallState::Installed);
    }

    #[test]
    fn verify_checks_sha256() {
        let (_d, s) = store();
        write(&s, "rec-latin/inference.onnx", "abc");
        write(&s, "rec-latin/dict.txt", "abc");
        assert!(s.verify("rec-latin").unwrap());
        write(&s, "rec-latin/dict.txt", "abd");
        assert!(!s.verify("rec-latin").unwrap());
    }

    #[test]
    fn delete_removes_pack_but_refuses_core() {
        let (_d, s) = store();
        write(&s, "rec-latin/inference.onnx", "abc");
        write(&s, "rec-latin/dict.txt", "abc");
        s.delete("rec-latin").unwrap();
        assert_eq!(s.state("rec-latin").unwrap(), InstallState::NotInstalled);
        assert!(!s.root().join("rec-latin").exists());
        assert!(matches!(
            s.delete("det"),
            Err(TranslatteError::CoreModelDelete(_))
        ));
    }

    #[test]
    fn available_source_languages_requires_core_and_pack() {
        let (_d, s) = store();
        write(&s, "rec-latin/inference.onnx", "abc");
        write(&s, "rec-latin/dict.txt", "abc");
        assert!(s.available_source_languages().unwrap().is_empty());
        install_core(&s);
        let codes: Vec<_> = s
            .available_source_languages()
            .unwrap()
            .iter()
            .map(|l| l.code.clone())
            .collect();
        assert_eq!(codes, vec!["fr"]);
        assert_eq!(s.target_languages().len(), 2);
    }

    #[test]
    fn model_file_resolves_by_file_name() {
        let (_d, s) = store();
        assert_eq!(
            s.model_file("rec-latin", "dict.txt").unwrap(),
            s.root().join("rec-latin/dict.txt")
        );
        assert!(s.model_file("rec-latin", "nope.txt").is_err());
        assert!(s.model_file("nope", "dict.txt").is_err());
    }
}
