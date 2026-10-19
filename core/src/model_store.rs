use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::manifest::{
    Language, Manifest, ModelEntry, ModelFile, ModelKind, DETECTOR_ID, TRANSLATOR_ID,
};
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
        Ok(self.failed_files(id)?.is_empty())
    }

    /// Checks size, then streams the sha256. A missing file is `Ok(false)`.
    pub fn verify_file(&self, file: &ModelFile, path: &Path) -> Result<bool> {
        let mut f = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        if f.metadata()?.len() != file.size {
            return Ok(false);
        }
        let mut hasher = Sha256::new();
        io::copy(&mut f, &mut hasher)?;
        Ok(hex::encode(hasher.finalize()).eq_ignore_ascii_case(&file.sha256))
    }

    pub fn failed_files(&self, id: &str) -> Result<Vec<&ModelFile>> {
        let mut out = Vec::new();
        for f in &self.entry(id)?.files {
            if !self.verify_file(f, &self.root.join(&f.path))? {
                out.push(f);
            }
        }
        Ok(out)
    }

    pub fn part_path(&self, file: &ModelFile) -> PathBuf {
        let mut p = self.root.join(&file.path).into_os_string();
        p.push(".part");
        PathBuf::from(p)
    }

    /// Verifies `<file>.part` and atomically renames it into place; a corrupt `.part` is deleted.
    pub fn commit_download(&self, file: &ModelFile) -> Result<()> {
        let part = self.part_path(file);
        if !part.exists() {
            return Err(TranslatteError::ModelNotInstalled(file.path.clone()));
        }
        if !self.verify_file(file, &part)? {
            std::fs::remove_file(&part)?;
            return Err(TranslatteError::ChecksumMismatch {
                path: file.path.clone(),
            });
        }
        let dest = self.root.join(&file.path);
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::rename(&part, &dest)?;
        Ok(())
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

    fn file<'a>(s: &'a ModelStore, id: &str, name: &str) -> &'a ModelFile {
        s.manifest()
            .model(id)
            .unwrap()
            .files
            .iter()
            .find(|f| f.path.ends_with(name))
            .unwrap()
    }

    #[test]
    fn verify_file_checks_size_then_hash_and_handles_missing() {
        let (d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        let p = d.path().join("x.bin");
        assert!(!s.verify_file(f, &p).unwrap());
        std::fs::write(&p, "abc").unwrap();
        assert!(s.verify_file(f, &p).unwrap());
        std::fs::write(&p, "abd").unwrap();
        assert!(!s.verify_file(f, &p).unwrap());
        std::fs::write(&p, "abcd").unwrap();
        assert!(!s.verify_file(f, &p).unwrap());
    }

    #[test]
    fn failed_files_lists_missing_and_bad_files() {
        let (_d, s) = store();
        assert_eq!(s.failed_files("rec-latin").unwrap().len(), 2);
        write(&s, "rec-latin/inference.onnx", "abc");
        write(&s, "rec-latin/dict.txt", "abd");
        let failed = s.failed_files("rec-latin").unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].path, "rec-latin/dict.txt");
        write(&s, "rec-latin/dict.txt", "abc");
        assert!(s.failed_files("rec-latin").unwrap().is_empty());
        assert!(s.failed_files("nope").is_err());
    }

    #[test]
    fn part_path_appends_part_suffix() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        assert_eq!(s.part_path(f), s.root().join("rec-latin/dict.txt.part"));
    }

    #[test]
    fn commit_download_renames_verified_part() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        write(&s, "rec-latin/dict.txt.part", "abc");
        s.commit_download(f).unwrap();
        assert!(!s.part_path(f).exists());
        assert_eq!(
            std::fs::read_to_string(s.root().join("rec-latin/dict.txt")).unwrap(),
            "abc"
        );
    }

    #[test]
    fn commit_download_deletes_bad_part() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        write(&s, "rec-latin/dict.txt.part", "abd");
        assert!(matches!(
            s.commit_download(f),
            Err(TranslatteError::ChecksumMismatch { .. })
        ));
        assert!(!s.part_path(f).exists());
        assert!(!s.root().join("rec-latin/dict.txt").exists());
    }

    #[test]
    fn commit_download_without_part_is_an_error() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        assert!(matches!(
            s.commit_download(f),
            Err(TranslatteError::ModelNotInstalled(_))
        ));
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
