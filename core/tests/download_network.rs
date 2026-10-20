#![cfg(feature = "download")]

use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use futures::future::BoxFuture;
use translatte_core::download::{Downloader, HubSource, Opened, Source};
use translatte_core::manifest::{Manifest, ModelFile, ModelSource};
use translatte_core::model_store::{InstallState, ModelStore};
use translatte_core::Result;

fn enabled() -> bool {
    let on = std::env::var("TRANSLATTE_NETWORK_TESTS").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("TRANSLATTE_NETWORK_TESTS != 1; skipping");
    }
    on
}

fn manifest() -> Manifest {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../models/manifest.json");
    Manifest::from_json(&std::fs::read_to_string(path).unwrap()).unwrap()
}

struct Recording {
    inner: HubSource,
    requests: Mutex<Vec<(String, Option<Range<u64>>)>>,
}

impl Recording {
    fn new() -> Self {
        Self {
            inner: HubSource::from_env().unwrap(),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn take(&self) -> Vec<(String, Option<Range<u64>>)> {
        std::mem::take(&mut *self.requests.lock().unwrap())
    }
}

impl Source for &Recording {
    fn open<'a>(
        &'a self,
        source: &'a ModelSource,
        path: &'a str,
        range: Option<Range<u64>>,
    ) -> BoxFuture<'a, Result<Opened>> {
        self.requests
            .lock()
            .unwrap()
            .push((path.to_string(), range.clone()));
        self.inner.open(source, path, range)
    }
}

fn file<'a>(store: &'a ModelStore, id: &str, name: &str) -> &'a ModelFile {
    let m = store.manifest().model(id).unwrap();
    m.files.iter().find(|f| f.path.ends_with(name)).unwrap()
}

#[test]
fn downloads_small_pack_and_resumes_truncated_part() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let store = ModelStore::new(manifest(), dir.path().to_path_buf());
    let rec = Recording::new();
    let d = Downloader::with_source(&store, &rec);
    let cancel = AtomicBool::new(false);

    d.download_model_blocking("rec-en", &cancel, |_| {})
        .unwrap();
    assert_eq!(store.state("rec-en").unwrap(), InstallState::Installed);
    assert_eq!(store.state("rec-en").unwrap(), InstallState::Installed);
    let onnx = file(&store, "rec-en", "inference.onnx");
    assert_eq!(
        rec.take(),
        vec![
            ("inference.onnx".to_string(), Some(0..onnx.size)),
            ("inference.yml".to_string(), None),
        ]
    );

    let installed = dir.path().join(&onnx.path);
    let cut = onnx.size / 2 + 12_345;
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(&installed)
        .unwrap();
    f.set_len(cut).unwrap();
    drop(f);
    std::fs::rename(&installed, store.part_path(onnx)).unwrap();
    assert_eq!(store.state("rec-en").unwrap(), InstallState::Corrupt);

    d.download_model_blocking("rec-en", &cancel, |_| {})
        .unwrap();
    assert_eq!(
        rec.take(),
        vec![("inference.onnx".to_string(), Some(cut..onnx.size))]
    );
    assert_eq!(store.state("rec-en").unwrap(), InstallState::Installed);
    assert!(!store.part_path(onnx).exists());
}

/// Resumes the 1.1GB translator from a local copy missing its last 8 MiB, so only a range is
/// fetched. Needs TRANSLATTE_MODELS pointing at an installed models dir.
#[test]
fn resumes_translator_tail_from_local_copy() {
    if !enabled() {
        return;
    }
    let Some(models) = std::env::var_os("TRANSLATTE_MODELS") else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let store = ModelStore::new(manifest(), dir.path().to_path_buf());
    let gguf = file(&store, "translator", "model.gguf");
    let keep = gguf.size - (8 << 20);
    let mut src = std::fs::File::open(std::path::Path::new(&models).join(&gguf.path)).unwrap();
    std::fs::create_dir_all(dir.path().join("translator")).unwrap();
    let mut part = std::fs::File::create(store.part_path(gguf)).unwrap();
    src.seek(SeekFrom::Start(0)).unwrap();
    std::io::copy(&mut (&mut src).take(keep), &mut part).unwrap();
    drop(part);

    let rec = Recording::new();
    Downloader::with_source(&store, &rec)
        .download_model_blocking("translator", &AtomicBool::new(false), |_| {})
        .unwrap();
    assert_eq!(
        rec.take(),
        vec![(gguf.source_path.clone(), Some(keep..gguf.size))]
    );
    assert_eq!(store.state("translator").unwrap(), InstallState::Installed);
}
