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
            .push((format!("{}/{path}", source.repo), range.clone()));
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

    d.download_model_blocking("rec-latin", &cancel, |_| {})
        .unwrap();
    assert_eq!(store.state("rec-latin").unwrap(), InstallState::Installed);
    assert_eq!(store.state("rec-latin").unwrap(), InstallState::Installed);
    let param = file(&store, "rec-latin", "inference.ncnn.param");
    let bin = file(&store, "rec-latin", "inference.ncnn.bin");
    let ncnn = "assafvayner/translatte-ocr-ncnn/rec-latin";
    assert_eq!(
        rec.take(),
        vec![
            (format!("{ncnn}/inference.ncnn.param"), Some(0..param.size)),
            (format!("{ncnn}/inference.ncnn.bin"), Some(0..bin.size)),
            (
                "PaddlePaddle/latin_PP-OCRv5_mobile_rec_onnx/inference.yml".to_string(),
                None
            ),
        ]
    );

    let installed = dir.path().join(&bin.path);
    let cut = bin.size / 2 + 12_345;
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(&installed)
        .unwrap();
    f.set_len(cut).unwrap();
    drop(f);
    std::fs::rename(&installed, store.part_path(bin)).unwrap();
    assert_eq!(store.state("rec-latin").unwrap(), InstallState::Corrupt);

    d.download_model_blocking("rec-latin", &cancel, |_| {})
        .unwrap();
    assert_eq!(
        rec.take(),
        vec![(format!("{ncnn}/inference.ncnn.bin"), Some(cut..bin.size))]
    );
    assert_eq!(store.state("rec-latin").unwrap(), InstallState::Installed);
    assert!(!store.part_path(bin).exists());
}

/// Resumes pair-ja-en's model from a local copy missing its last 8 MiB, so only a range is
/// fetched. Needs TRANSLATTE_MODELS pointing at a models dir with pair-ja-en installed.
#[test]
fn resumes_a_pair_model_tail_from_local_copy() {
    if !enabled() {
        return;
    }
    let Some(models) = std::env::var_os("TRANSLATTE_MODELS") else {
        eprintln!("TRANSLATTE_MODELS not set; skipping");
        return;
    };
    let models = std::path::Path::new(&models);
    let dir = tempfile::tempdir().unwrap();
    let store = ModelStore::new(manifest(), dir.path().to_path_buf());
    std::fs::create_dir_all(dir.path().join("pair-ja-en")).unwrap();
    for name in ["vocab.spm", "lex.bin"] {
        let f = file(&store, "pair-ja-en", name);
        std::fs::copy(models.join(&f.path), dir.path().join(&f.path)).unwrap();
    }
    let model = file(&store, "pair-ja-en", "model.bin");
    let keep = model.size - (8 << 20);
    let mut src = std::fs::File::open(models.join(&model.path)).unwrap();
    let mut part = std::fs::File::create(store.part_path(model)).unwrap();
    src.seek(SeekFrom::Start(0)).unwrap();
    std::io::copy(&mut (&mut src).take(keep), &mut part).unwrap();
    drop(part);

    let rec = Recording::new();
    Downloader::with_source(&store, &rec)
        .download_model_blocking("pair-ja-en", &AtomicBool::new(false), |_| {})
        .unwrap();
    assert_eq!(
        rec.take(),
        vec![(
            format!("assafvayner/translatte-pair-models/{}", model.source_path),
            Some(keep..model.size)
        )]
    );
    assert_eq!(store.state("pair-ja-en").unwrap(), InstallState::Installed);
}
