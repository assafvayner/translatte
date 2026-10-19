use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

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

    /// True when `file` exists in the store with the expected size.
    pub fn is_file_installed(&self, file: &ModelFile) -> Result<bool> {
        match std::fs::metadata(self.root.join(&file.path)) {
            Ok(m) => Ok(m.len() == file.size),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn part_path(&self, file: &ModelFile) -> PathBuf {
        let mut p = self.root.join(&file.path).into_os_string();
        p.push(".part");
        PathBuf::from(p)
    }

    /// Checks the size of `<file>.part` and atomically renames it into place.
    ///
    /// A `.part` shorter than expected is kept (`Incomplete`) so the download can resume; an
    /// oversize `.part` is deleted (`SizeMismatch`). Assumes a single writer per
    /// file (one download job per model); concurrent commits of the same file are not supported.
    pub fn commit_download(&self, file: &ModelFile) -> Result<()> {
        let part = self.part_path(file);
        let have = match std::fs::metadata(&part) {
            Ok(m) => m.len(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(TranslatteError::PartMissing {
                    path: file.path.clone(),
                })
            }
            Err(e) => return Err(e.into()),
        };
        if have < file.size {
            return Err(TranslatteError::Incomplete {
                path: file.path.clone(),
                have,
                want: file.size,
            });
        }
        if have > file.size {
            std::fs::remove_file(&part)?;
            return Err(TranslatteError::SizeMismatch {
                path: file.path.clone(),
                have,
                want: file.size,
            });
        }
        let dest = self.root.join(&file.path);
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::OpenOptions::new()
            .write(true)
            .open(&part)?
            .sync_all()?;
        std::fs::rename(&part, &dest)?;
        #[cfg(unix)]
        if let Some(dir) = dest.parent() {
            File::open(dir)?.sync_all()?;
        }
        Ok(())
    }

    /// Bytes of `id` already on disk: installed files plus partial downloads, for resuming.
    pub fn downloaded_bytes(&self, id: &str) -> Result<u64> {
        let mut total = 0;
        for f in &self.entry(id)?.files {
            total += if self.is_file_installed(f)? {
                f.size
            } else {
                match std::fs::metadata(self.part_path(f)) {
                    Ok(m) => m.len().min(f.size),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => 0,
                    Err(e) => return Err(e.into()),
                }
            };
        }
        Ok(total)
    }

    /// Removes a script pack's files, including partial downloads.
    pub fn delete(&self, id: &str) -> Result<()> {
        let entry = self.entry(id)?;
        if entry.kind == ModelKind::Core {
            return Err(TranslatteError::CoreModelDelete(id.to_string()));
        }
        for f in &entry.files {
            for path in [self.root.join(&f.path), self.part_path(f)] {
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
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
    fn is_file_installed_checks_size_and_handles_missing() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        assert!(!s.is_file_installed(f).unwrap());
        write(&s, "rec-latin/dict.txt", "abc");
        assert!(s.is_file_installed(f).unwrap());
        write(&s, "rec-latin/dict.txt", "abcd");
        assert!(!s.is_file_installed(f).unwrap());
    }

    #[test]
    fn part_path_appends_part_suffix() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        assert_eq!(s.part_path(f), s.root().join("rec-latin/dict.txt.part"));
    }

    #[test]
    fn commit_download_renames_full_size_part() {
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
    fn commit_download_without_part_is_an_error() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        assert!(matches!(
            s.commit_download(f),
            Err(TranslatteError::PartMissing { .. })
        ));
    }

    #[test]
    fn commit_download_keeps_short_part_for_resume() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        write(&s, "rec-latin/dict.txt.part", "ab");
        assert!(matches!(
            s.commit_download(f),
            Err(TranslatteError::Incomplete {
                have: 2,
                want: 3,
                ..
            })
        ));
        assert_eq!(std::fs::read_to_string(s.part_path(f)).unwrap(), "ab");
    }

    #[test]
    fn commit_download_deletes_oversize_part() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        write(&s, "rec-latin/dict.txt.part", "abcd");
        assert!(matches!(
            s.commit_download(f),
            Err(TranslatteError::SizeMismatch {
                have: 4,
                want: 3,
                ..
            })
        ));
        assert!(!s.part_path(f).exists());
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
    fn delete_removes_partial_downloads() {
        let (_d, s) = store();
        write(&s, "rec-latin/inference.onnx", "abc");
        write(&s, "rec-latin/dict.txt.part", "ab");
        s.delete("rec-latin").unwrap();
        assert!(!s.root().join("rec-latin").exists());
    }

    #[test]
    fn downloaded_bytes_counts_installed_and_partial_files() {
        let (_d, s) = store();
        assert_eq!(s.downloaded_bytes("rec-latin").unwrap(), 0);
        write(&s, "rec-latin/inference.onnx", "abc");
        write(&s, "rec-latin/dict.txt.part", "ab");
        assert_eq!(s.downloaded_bytes("rec-latin").unwrap(), 5);
        write(&s, "rec-latin/dict.txt.part", "abcdef");
        assert_eq!(s.downloaded_bytes("rec-latin").unwrap(), 6);
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
