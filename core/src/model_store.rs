use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::manifest::{
    Language, Manifest, ModelEntry, ModelFile, ModelKind, ModelSource, DETECTOR_ID, TARGET_LANGUAGE,
};
use crate::{Result, TranslatteError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallState {
    NotInstalled,
    Installed,
    /// Some files are missing or have the wrong size.
    Corrupt,
}

/// Models earlier manifests shipped and later dropped; [`ModelStore::prune`] deletes their
/// directories. Add an id here whenever the manifest drops a model.
pub const RETIRED_MODEL_IDS: &[&str] = &["rec-el", "rec-cyrillic", "rec-en", "translator"];

#[derive(Clone)]
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

    pub fn require_installed(&self, id: &str) -> Result<()> {
        if self.state(id)? != InstallState::Installed {
            return Err(TranslatteError::ModelNotInstalled(id.to_string()));
        }
        Ok(())
    }

    /// True when `file` exists in the store with the expected size.
    pub fn is_file_installed(&self, file: &ModelFile) -> Result<bool> {
        match std::fs::metadata(self.root.join(&file.path)) {
            Ok(m) => Ok(m.len() == file.size),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// `<file>.<tag>.part`, where the tag identifies the upstream file, so a partial download is
    /// only ever resumed from the file it started from (e.g. not after the manifest swaps the model).
    pub fn part_path(&self, file: &ModelFile) -> PathBuf {
        let source = self
            .manifest
            .models
            .iter()
            .find(|m| m.files.iter().any(|f| f.path == file.path))
            .map(|m| &m.source);
        let mut p = self.root.join(&file.path).into_os_string();
        p.push(format!(".{}.part", part_tag(source, file)));
        PathBuf::from(p)
    }

    /// Removes model data an app update left behind: directories of [`RETIRED_MODEL_IDS`] that the
    /// manifest no longer lists, installed files whose size no longer matches (a replaced model),
    /// and partial downloads of other upstream files. Other directories in the root (e.g. a
    /// model pushed by hand for comparisons) are left alone. Returns the removed paths. Must not
    /// run concurrently with a download.
    pub fn prune(&self) -> Result<Vec<PathBuf>> {
        let mut removed = Vec::new();
        let dirs = match std::fs::read_dir(&self.root) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(removed),
            Err(e) => return Err(e.into()),
        };
        for dir in dirs {
            let dir = dir?;
            let retired = dir.file_name().to_str().is_some_and(|name| {
                RETIRED_MODEL_IDS.contains(&name) && self.manifest.model(name).is_none()
            });
            if retired && dir.file_type()?.is_dir() {
                std::fs::remove_dir_all(dir.path())?;
                removed.push(dir.path());
            }
        }
        let mut keep = std::collections::HashSet::new();
        let mut model_dirs = std::collections::BTreeSet::new();
        for f in self.manifest.models.iter().flat_map(|m| &m.files) {
            let dest = self.root.join(&f.path);
            match std::fs::metadata(&dest) {
                Ok(m) if m.is_file() && m.len() != f.size => {
                    std::fs::remove_file(&dest)?;
                    removed.push(dest.clone());
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            keep.insert(self.part_path(f));
            if let Some(dir) = dest.parent() {
                model_dirs.insert(dir.to_path_buf());
            }
        }
        for dir in model_dirs {
            let entries = match std::fs::read_dir(&dir) {
                Ok(d) => d,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for entry in entries {
                let path = entry?.path();
                let is_part = path.extension().is_some_and(|e| e == "part");
                if is_part && !keep.contains(&path) && path.is_file() {
                    std::fs::remove_file(&path)?;
                    removed.push(path);
                }
            }
        }
        Ok(removed)
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

    /// Removes a script pack's or pair model's files, including partial downloads.
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
        Ok(self.state(DETECTOR_ID)? == InstallState::Installed)
    }

    /// Languages that can be read and translated (they have a script pack and a pair model),
    /// installed or not, sorted by name.
    pub fn source_languages(&self) -> Vec<&Language> {
        let mut out: Vec<_> = self
            .manifest
            .languages
            .iter()
            .filter(|l| l.script_pack.is_some() && l.pair_model.is_some())
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Source languages whose pack is installed, once the core models are.
    pub fn available_source_languages(&self) -> Result<Vec<&Language>> {
        if !self.core_installed()? {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for l in self.source_languages() {
            if self.language_state(&l.code)? == InstallState::Installed {
                out.push(l);
            }
        }
        Ok(out)
    }

    /// The language pair models translate into.
    pub fn target_language(&self) -> Result<&Language> {
        self.manifest.language(TARGET_LANGUAGE)
    }

    /// The models of a source language's pack: its script pack, then its pair model.
    pub fn language_models<'a>(&self, l: &'a Language) -> Vec<&'a str> {
        l.script_pack
            .iter()
            .chain(&l.pair_model)
            .map(String::as_str)
            .collect()
    }

    /// Installed once the script pack and the pair model are. Not installed while the pair model
    /// isn't, even if another language installed the shared script pack; otherwise partial.
    pub fn language_state(&self, code: &str) -> Result<InstallState> {
        let l = self.manifest.language(code)?;
        let (Some(pack), Some(pair)) = (&l.script_pack, &l.pair_model) else {
            return Err(TranslatteError::Manifest(format!(
                "{code} is not a source language"
            )));
        };
        Ok(match (self.state(pack)?, self.state(pair)?) {
            (InstallState::Installed, InstallState::Installed) => InstallState::Installed,
            (InstallState::NotInstalled | InstallState::Installed, InstallState::NotInstalled) => {
                InstallState::NotInstalled
            }
            _ => InstallState::Corrupt,
        })
    }

    /// Deleting `code` would free space: its pair model, or a script pack no other language needs,
    /// is at least partly on disk.
    pub fn language_deletable(&self, code: &str) -> Result<bool> {
        let l = self.manifest.language(code)?;
        let mut bytes = 0;
        if let Some(pair) = &l.pair_model {
            bytes += self.downloaded_bytes(pair)?;
        }
        if let Some(pack) = &l.script_pack {
            if !self.script_pack_shared(l)? {
                bytes += self.downloaded_bytes(pack)?;
            }
        }
        Ok(bytes > 0)
    }

    /// Deletes a source language's pair model, and its script pack unless another language with a
    /// pair model on disk reads with it.
    pub fn delete_language(&self, code: &str) -> Result<()> {
        let l = self.manifest.language(code)?;
        if let Some(pair) = &l.pair_model {
            self.delete(pair)?;
        }
        if let Some(pack) = &l.script_pack {
            if !self.script_pack_shared(l)? {
                self.delete(pack)?;
            }
        }
        Ok(())
    }

    /// Whether a language with another pair model that is at least partly on disk reads with `l`'s
    /// script pack.
    fn script_pack_shared(&self, l: &Language) -> Result<bool> {
        for other in &self.manifest.languages {
            let Some(pair) = &other.pair_model else {
                continue;
            };
            if other.script_pack != l.script_pack || Some(pair) == l.pair_model.as_ref() {
                continue;
            }
            if self.downloaded_bytes(pair)? > 0 {
                return Ok(true);
            }
        }
        Ok(false)
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

/// FNV-1a over the upstream identity of `file`; stable across builds and platforms.
fn part_tag(source: Option<&ModelSource>, file: &ModelFile) -> String {
    let (repo, revision) = source.map_or(("", ""), |s| (s.repo.as_str(), s.revision.as_str()));
    let size = file.size.to_le_bytes();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for field in [
        repo.as_bytes(),
        revision.as_bytes(),
        file.source_path.as_bytes(),
        &size,
    ] {
        for &b in field.iter().chain(&[0]) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::SAMPLE;
    use crate::manifest::TARGET_LANGUAGE;

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

    fn write_part(store: &ModelStore, name: &str, contents: &str) {
        let p = store.part_path(file(store, "rec-latin", name));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }

    fn install(store: &ModelStore, id: &str) {
        for f in &store.manifest().model(id).unwrap().files {
            write(store, &f.path, "abc");
        }
    }

    fn install_core(store: &ModelStore) {
        install(store, "det");
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
    fn part_path_is_tagged_with_the_upstream_file() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        let part = s.part_path(f);
        let name = part.file_name().unwrap().to_str().unwrap();
        assert_eq!(part.parent().unwrap(), s.root().join("rec-latin"));
        assert!(
            name.starts_with("dict.txt.") && name.ends_with(".part"),
            "{name}"
        );
        assert_eq!(name.len(), "dict.txt..part".len() + 16);

        let moved =
            Manifest::from_json(&SAMPLE.replace("acme/latin_rec", "acme/latin_rec2")).unwrap();
        let other = ModelStore::new(moved, s.root().to_path_buf());
        assert_ne!(other.part_path(file(&other, "rec-latin", "dict.txt")), part);
        let resized = Manifest::from_json(&SAMPLE.replace(
            r#""path": "rec-latin/dict.txt", "size": 3"#,
            r#""path": "rec-latin/dict.txt", "size": 4"#,
        ))
        .unwrap();
        let other = ModelStore::new(resized, s.root().to_path_buf());
        assert_ne!(other.part_path(file(&other, "rec-latin", "dict.txt")), part);
    }

    /// pair-fr-en's model as an older manifest had it: same path, another upstream file and size.
    fn old_pair(s: &ModelStore) -> ModelStore {
        let old = SAMPLE
            .replace(
                "fr-en/model.fren.intgemm.alphas.bin",
                "fr-en/model.fren.v1.bin",
            )
            .replace(
                r#""path": "pair-fr-en/model.bin", "size": 3"#,
                r#""path": "pair-fr-en/model.bin", "size": 5"#,
            );
        ModelStore::new(Manifest::from_json(&old).unwrap(), s.root().to_path_buf())
    }

    #[test]
    fn replaced_model_needs_download_and_its_old_files_are_pruned() {
        let (_d, s) = store();
        let old = old_pair(&s);
        install_core(&s);
        write(&s, "pair-fr-en/model.bin", "abcde");
        write(&s, "pair-fr-en/vocab.spm", "abc");
        write(&s, "pair-fr-en/lex.bin", "abc");
        assert_eq!(old.state("pair-fr-en").unwrap(), InstallState::Installed);
        assert_eq!(s.state("pair-fr-en").unwrap(), InstallState::Corrupt);
        assert_eq!(s.downloaded_bytes("pair-fr-en").unwrap(), 6);

        let removed = s.prune().unwrap();
        assert_eq!(removed, vec![s.root().join("pair-fr-en/model.bin")]);
        assert!(!s
            .is_file_installed(file(&s, "pair-fr-en", "model.bin"))
            .unwrap());
        assert_eq!(s.state("det").unwrap(), InstallState::Installed);
    }

    #[test]
    fn prune_drops_partial_downloads_of_other_upstream_files() {
        let (_d, s) = store();
        let old = old_pair(&s);
        let f = file(&s, "pair-fr-en", "model.bin");
        let stale = old.part_path(file(&old, "pair-fr-en", "model.bin"));
        let legacy = s.root().join("pair-fr-en/model.bin.part");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, "ab").unwrap();
        std::fs::write(&legacy, "ab").unwrap();
        std::fs::write(s.part_path(f), "ab").unwrap();
        assert_eq!(s.downloaded_bytes("pair-fr-en").unwrap(), 2);

        let mut removed = s.prune().unwrap();
        removed.sort();
        let mut want = vec![stale, legacy];
        want.sort();
        assert_eq!(removed, want);
        assert_eq!(std::fs::read_to_string(s.part_path(f)).unwrap(), "ab");
    }

    #[test]
    fn prune_removes_dirs_of_retired_models_only() {
        let (_d, s) = store();
        write(&s, "rec-el/inference.onnx", "abc");
        write(&s, "rec-en/inference.onnx", "abc");
        write(&s, "translator/model.gguf", "abc");
        write(&s, "rec-latin/inference.onnx", "abc");
        write(&s, "translator-alt/model.gguf", "abc");
        write(&s, "loose-file", "x");
        let mut removed = s.prune().unwrap();
        removed.sort();
        assert_eq!(
            removed,
            vec![
                s.root().join("rec-el"),
                s.root().join("rec-en"),
                s.root().join("translator")
            ]
        );
        assert!(!s.root().join("rec-el").exists());
        assert!(s.root().join("rec-latin/inference.onnx").exists());
        assert!(s.root().join("translator-alt/model.gguf").exists());
        assert!(s.root().join("loose-file").exists());
        assert!(s.prune().unwrap().is_empty());
    }

    #[test]
    fn prune_of_missing_root_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let s = ModelStore::new(
            Manifest::from_json(SAMPLE).unwrap(),
            dir.path().join("missing"),
        );
        assert!(s.prune().unwrap().is_empty());
    }

    #[test]
    fn commit_download_renames_full_size_part() {
        let (_d, s) = store();
        let f = file(&s, "rec-latin", "dict.txt");
        write_part(&s, "dict.txt", "abc");
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
        write_part(&s, "dict.txt", "ab");
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
        write_part(&s, "dict.txt", "abcd");
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
        write_part(&s, "dict.txt", "ab");
        s.delete("rec-latin").unwrap();
        assert!(!s.root().join("rec-latin").exists());
    }

    #[test]
    fn downloaded_bytes_counts_installed_and_partial_files() {
        let (_d, s) = store();
        assert_eq!(s.downloaded_bytes("rec-latin").unwrap(), 0);
        write(&s, "rec-latin/inference.onnx", "abc");
        write_part(&s, "dict.txt", "ab");
        assert_eq!(s.downloaded_bytes("rec-latin").unwrap(), 5);
        write_part(&s, "dict.txt", "abcdef");
        assert_eq!(s.downloaded_bytes("rec-latin").unwrap(), 6);
    }

    #[test]
    fn available_source_languages_require_core_pack_and_pair_model() {
        let (_d, s) = store();
        install(&s, "rec-latin");
        install_core(&s);
        assert!(s.available_source_languages().unwrap().is_empty());
        install(&s, "pair-fr-en");
        let codes: Vec<_> = s
            .available_source_languages()
            .unwrap()
            .iter()
            .map(|l| l.code.as_str())
            .collect();
        assert_eq!(codes, vec!["fr"]);
    }

    #[test]
    fn source_languages_are_sorted_by_name_and_exclude_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let s = ModelStore::new(
            Manifest::from_json(&SAMPLE.replace(r#""French""#, r#""Zulu""#)).unwrap(),
            dir.path().to_path_buf(),
        );
        let codes: Vec<_> = s
            .source_languages()
            .iter()
            .map(|l| l.code.as_str())
            .collect();
        assert_eq!(codes, vec!["de", "fr"]);
    }

    #[test]
    fn the_target_language_is_english() {
        let (_d, s) = store();
        assert_eq!(s.target_language().unwrap().code, TARGET_LANGUAGE);
    }

    #[test]
    fn language_state_needs_the_pack_and_the_pair_model() {
        let (_d, s) = store();
        assert_eq!(s.language_state("fr").unwrap(), InstallState::NotInstalled);
        install(&s, "rec-latin");
        assert_eq!(s.language_state("fr").unwrap(), InstallState::NotInstalled);
        install(&s, "pair-fr-en");
        assert_eq!(s.language_state("fr").unwrap(), InstallState::Installed);
        write(&s, "pair-de-en/model.bin", "abc");
        assert_eq!(s.language_state("de").unwrap(), InstallState::Corrupt);
        assert!(s.language_state(TARGET_LANGUAGE).is_err());
        assert_eq!(
            s.language_models(s.manifest().language("fr").unwrap()),
            vec!["rec-latin", "pair-fr-en"]
        );
    }

    #[test]
    fn deleting_a_language_keeps_a_script_pack_another_language_uses() {
        let (_d, s) = store();
        for id in ["rec-latin", "pair-fr-en", "pair-de-en"] {
            install(&s, id);
        }
        assert!(s.language_deletable("de").unwrap());
        s.delete_language("de").unwrap();
        assert_eq!(s.state("pair-de-en").unwrap(), InstallState::NotInstalled);
        assert_eq!(s.state("rec-latin").unwrap(), InstallState::Installed);
        assert!(
            !s.language_deletable("de").unwrap(),
            "only French's pack is left"
        );
        s.delete_language("fr").unwrap();
        assert_eq!(s.state("rec-latin").unwrap(), InstallState::NotInstalled);
        assert!(!s.root().join("rec-latin").exists());
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
