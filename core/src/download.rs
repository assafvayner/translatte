//! Resumable, verified model downloads straight from the pinned upstream Hugging Face repos.
//!
//! Files are streamed into `<file>.part` (appending from its current length via a byte-range
//! request), then `ModelStore::commit_download` verifies the sha256 and renames it into place.
//! Downloaded bytes are untrusted until that check passes.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::ops::Range;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::future::BoxFuture;
use futures::{Stream, StreamExt};
use serde::Deserialize;

use crate::manifest::{Derive, ModelEntry, ModelFile, ModelSource};
use crate::model_store::ModelStore;
use crate::{Result, TranslatteError};

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>;

/// Where file bytes come from: [`HubSource`] in production, fakes in tests.
pub trait Source: Send + Sync {
    /// Streams `path` from the pinned `source` repo, limited to `range` when given.
    fn open<'a>(
        &'a self,
        source: &'a ModelSource,
        path: &'a str,
        range: Option<Range<u64>>,
    ) -> BoxFuture<'a, Result<ByteStream>>;
}

/// Fetches files from the Hugging Face Hub (or a mirror) with `hf-hub`.
pub struct HubSource {
    client: hf_hub::HFClient,
}

impl HubSource {
    pub fn new(client: hf_hub::HFClient) -> Self {
        Self { client }
    }

    /// Default Hub client; honors `HF_ENDPOINT` and `HF_TOKEN` when set.
    pub fn from_env() -> Result<Self> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let mut builder = hf_hub::HFClient::builder()
            .cache_enabled(false)
            .user_agent(format!("translatte/{}", env!("CARGO_PKG_VERSION")));
        if let Some(endpoint) = env("HF_ENDPOINT") {
            builder = builder.endpoint(endpoint);
        }
        if let Some(token) = env("HF_TOKEN") {
            builder = builder.token(token);
        }
        Ok(Self::new(builder.build().map_err(download_err)?))
    }
}

impl Source for HubSource {
    fn open<'a>(
        &'a self,
        source: &'a ModelSource,
        path: &'a str,
        range: Option<Range<u64>>,
    ) -> BoxFuture<'a, Result<ByteStream>> {
        Box::pin(async move {
            let (owner, name) = hf_hub::split_id(&source.repo);
            let (_, stream) = self
                .client
                .model(owner, name)
                .download_file_stream()
                .filename(path)
                .revision(source.revision.as_str())
                .maybe_range(range)
                .send()
                .await
                .map_err(download_err)?;
            Ok(Box::pin(stream.map(|r| r.map_err(download_err))) as ByteStream)
        })
    }
}

fn download_err(e: impl std::fmt::Display) -> TranslatteError {
    TranslatteError::Download(e.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadProgress {
    pub model_id: String,
    pub file_path: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub model_bytes_done: u64,
    pub model_bytes_total: u64,
}

const MAX_STALLED_ATTEMPTS: u32 = 5;
const RETRY_DELAY: Duration = Duration::from_secs(1);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const CANCEL_POLL: Duration = Duration::from_millis(200);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
/// Upper bound on a source file that is parsed rather than stored (e.g. `inference.yml`).
const MAX_DERIVE_SOURCE_BYTES: usize = 16 << 20;

pub struct Downloader<'a> {
    store: &'a ModelStore,
    source: Box<dyn Source + 'a>,
    retry_delay: Duration,
    idle_timeout: Duration,
    progress_interval: Duration,
}

impl<'a> Downloader<'a> {
    pub fn new(store: &'a ModelStore) -> Result<Self> {
        Ok(Self::with_source(store, HubSource::from_env()?))
    }

    pub fn with_source(store: &'a ModelStore, source: impl Source + 'a) -> Self {
        Self {
            store,
            source: Box::new(source),
            retry_delay: RETRY_DELAY,
            idle_timeout: IDLE_TIMEOUT,
            progress_interval: PROGRESS_INTERVAL,
        }
    }

    /// Downloads every file of `id` that is not already installed and verified.
    ///
    /// Interrupted transfers resume from the existing `.part`; a `.part` that fails verification
    /// is discarded and fetched again once. When `cancel` is set the call returns
    /// [`TranslatteError::Cancelled`] and leaves the `.part` in place for a later resume.
    /// Must run inside a tokio runtime with the time driver enabled.
    pub async fn download_model(
        &self,
        id: &str,
        cancel: &AtomicBool,
        progress: impl Fn(DownloadProgress) + Send + Sync,
    ) -> Result<()> {
        let entry = self
            .store
            .manifest()
            .model(id)
            .ok_or_else(|| TranslatteError::Manifest(format!("unknown model {id}")))?;
        let mut reporter = Reporter {
            progress: &progress,
            model_id: &entry.id,
            model_bytes_total: entry.total_size(),
            model_bytes_before: 0,
            interval: self.progress_interval,
            last: None,
        };
        for f in &entry.files {
            check_cancel(cancel)?;
            if !self
                .store
                .verify_file(f, &self.store.root().join(&f.path))?
            {
                if let Some(dir) = self.store.part_path(f).parent() {
                    std::fs::create_dir_all(dir)?;
                }
                match f.derive {
                    None => self.fetch_file(entry, f, cancel, &mut reporter).await?,
                    Some(kind) => self.derive_file(entry, f, kind, cancel).await?,
                }
            }
            reporter.report(f, f.size, true);
            reporter.model_bytes_before += f.size;
        }
        Ok(())
    }

    /// Downloads the models one after another, stopping at the first error.
    pub async fn download_models(
        &self,
        ids: &[&str],
        cancel: &AtomicBool,
        progress: impl Fn(DownloadProgress) + Send + Sync,
    ) -> Result<()> {
        for id in ids {
            self.download_model(id, cancel, &progress).await?;
        }
        Ok(())
    }

    /// [`Self::download_model`] on a private current-thread runtime.
    pub fn download_model_blocking(
        &self,
        id: &str,
        cancel: &AtomicBool,
        progress: impl Fn(DownloadProgress) + Send + Sync,
    ) -> Result<()> {
        block_on(self.download_model(id, cancel, progress))
    }

    /// [`Self::download_models`] on a private current-thread runtime.
    pub fn download_models_blocking(
        &self,
        ids: &[&str],
        cancel: &AtomicBool,
        progress: impl Fn(DownloadProgress) + Send + Sync,
    ) -> Result<()> {
        block_on(self.download_models(ids, cancel, progress))
    }

    async fn fetch_file(
        &self,
        entry: &ModelEntry,
        f: &ModelFile,
        cancel: &AtomicBool,
        reporter: &mut Reporter<'_>,
    ) -> Result<()> {
        let part = self.store.part_path(f);
        OpenOptions::new().create(true).append(true).open(&part)?;
        let mut stalled = 0;
        let mut restarted = false;
        loop {
            check_cancel(cancel)?;
            let before = part_len(&part)?;
            let failure = if before < f.size {
                let stream = self
                    .source
                    .open(&entry.source, &f.source_path, Some(before..f.size))
                    .await?;
                match self
                    .append(stream, &part, before, f, cancel, reporter)
                    .await
                {
                    Ok(()) => None,
                    Err(e @ TranslatteError::Download(_)) => Some(e),
                    Err(e) => return Err(e),
                }
            } else {
                None
            };
            let failure = match failure {
                Some(e) => e,
                None => match self.store.commit_download(f) {
                    Ok(()) => return Ok(()),
                    Err(e @ TranslatteError::Incomplete { .. }) => e,
                    Err(e @ TranslatteError::ChecksumMismatch { .. }) => {
                        if restarted {
                            return Err(e);
                        }
                        restarted = true;
                        stalled = 0;
                        continue;
                    }
                    Err(e) => return Err(e),
                },
            };
            if part_len(&part)? > before {
                stalled = 0;
            } else {
                stalled += 1;
                if stalled >= MAX_STALLED_ATTEMPTS {
                    return Err(failure);
                }
            }
            self.sleep_cancellable(self.retry_delay * stalled, cancel)
                .await?;
        }
    }

    async fn append(
        &self,
        mut stream: ByteStream,
        part: &Path,
        mut have: u64,
        f: &ModelFile,
        cancel: &AtomicBool,
        reporter: &mut Reporter<'_>,
    ) -> Result<()> {
        let mut out = OpenOptions::new().create(true).append(true).open(part)?;
        let mut idle = Duration::ZERO;
        while have < f.size {
            check_cancel(cancel)?;
            let chunk = match tokio::time::timeout(CANCEL_POLL, stream.next()).await {
                Err(_) => {
                    idle += CANCEL_POLL;
                    if idle >= self.idle_timeout {
                        return Err(TranslatteError::Download(format!(
                            "no data received for {}s",
                            idle.as_secs()
                        )));
                    }
                    continue;
                }
                Ok(None) => break,
                Ok(Some(chunk)) => chunk?,
            };
            idle = Duration::ZERO;
            // Bytes past the expected size are never written; the hash check catches a server
            // that ignored the range.
            let take = chunk
                .len()
                .min(usize::try_from(f.size - have).unwrap_or(usize::MAX));
            out.write_all(&chunk[..take])?;
            have += take as u64;
            reporter.report(f, have, false);
        }
        Ok(())
    }

    async fn derive_file(
        &self,
        entry: &ModelEntry,
        f: &ModelFile,
        kind: Derive,
        cancel: &AtomicBool,
    ) -> Result<()> {
        let mut attempt = 0;
        loop {
            check_cancel(cancel)?;
            let stream = self
                .source
                .open(&entry.source, &f.source_path, None)
                .await?;
            let raw = collect(stream, MAX_DERIVE_SOURCE_BYTES, cancel).await?;
            let data = match kind {
                Derive::PaddleDict => {
                    let text = std::str::from_utf8(&raw).map_err(|_| {
                        TranslatteError::Download(format!("{} is not UTF-8", f.source_path))
                    })?;
                    paddle_dict(text)?
                }
            };
            std::fs::write(self.store.part_path(f), data)?;
            match self.store.commit_download(f) {
                Err(TranslatteError::ChecksumMismatch { .. }) if attempt == 0 => attempt += 1,
                Err(TranslatteError::Incomplete { path, have, want }) => {
                    let _ = std::fs::remove_file(self.store.part_path(f));
                    return Err(TranslatteError::Download(format!(
                        "derived {path} has {have} bytes, expected {want}"
                    )));
                }
                r => return r,
            }
        }
    }

    async fn sleep_cancellable(&self, d: Duration, cancel: &AtomicBool) -> Result<()> {
        let end = Instant::now() + d;
        loop {
            check_cancel(cancel)?;
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            tokio::time::sleep(left.min(CANCEL_POLL)).await;
        }
    }
}

fn block_on<T>(fut: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(fut)
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(TranslatteError::Cancelled)
    } else {
        Ok(())
    }
}

fn part_len(p: &Path) -> Result<u64> {
    match std::fs::metadata(p) {
        Ok(m) => Ok(m.len()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e.into()),
    }
}

async fn collect(mut stream: ByteStream, limit: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        check_cancel(cancel)?;
        let chunk = chunk?;
        if buf.len() + chunk.len() > limit {
            return Err(TranslatteError::Download(format!(
                "source file exceeds {limit} bytes"
            )));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

struct Reporter<'p> {
    progress: &'p (dyn Fn(DownloadProgress) + Send + Sync),
    model_id: &'p str,
    model_bytes_total: u64,
    model_bytes_before: u64,
    interval: Duration,
    last: Option<Instant>,
}

impl Reporter<'_> {
    fn report(&mut self, f: &ModelFile, bytes_done: u64, force: bool) {
        let now = Instant::now();
        if !force && self.last.is_some_and(|t| now - t < self.interval) {
            return;
        }
        self.last = Some(now);
        (self.progress)(DownloadProgress {
            model_id: self.model_id.to_string(),
            file_path: f.path.clone(),
            bytes_done,
            bytes_total: f.size,
            model_bytes_done: self.model_bytes_before + bytes_done,
            model_bytes_total: self.model_bytes_total,
        });
    }
}

/// The recognizer dictionary embedded in a PaddleOCR `inference.yml`
/// (`PostProcess.character_dict`), one entry per line with a trailing newline.
///
/// Byte-identical to `paddle_dict` in `scripts/gen_manifest.py`, which produced the manifest
/// checksums.
pub fn paddle_dict(yml: &str) -> Result<Vec<u8>> {
    #[derive(Deserialize)]
    struct Config {
        #[serde(rename = "PostProcess")]
        post_process: PostProcess,
    }
    #[derive(Deserialize)]
    struct PostProcess {
        character_dict: Vec<String>,
    }
    let cfg: Config = serde_yaml_ng::from_str(yml)
        .map_err(|e| TranslatteError::Download(format!("invalid inference.yml: {e}")))?;
    let chars = cfg.post_process.character_dict;
    if let Some(bad) = chars.iter().find(|c| c.contains('\n')) {
        return Err(TranslatteError::Download(format!(
            "dictionary entry contains a newline: {bad:?}"
        )));
    }
    let mut out = chars.join("\n");
    out.push('\n');
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::manifest::Manifest;
    use crate::model_store::InstallState;

    const REV: &str = "0123456789abcdef0123456789abcdef01234567";

    const YAML: &str = r#"
Global:
  model_name: test_rec
Hpi:
  shapes: &id001
  - - 1
    - 3
  other: *id001
PostProcess:
  name: CTCLabelDecode
  character_dict:
  - '!'
  - '"'
  - '#'
  - $
  - ''''
  - "\""
  - ':'
  - '-'
  - a:b
  - '0'
  - 1
  - 007
  - 1.50
  - 0x1F
  - 1e3
  - .inf
  - yes
  - 'null'
  - "　"
  - '　'
  - ' '
  - é
  - 中
  - "\\"
  - '\'
  - 'a # b'
"#;

    #[test]
    fn paddle_dict_keeps_entries_as_written() {
        let got = String::from_utf8(paddle_dict(YAML).unwrap()).unwrap();
        let want = [
            "!", "\"", "#", "$", "'", "\"", ":", "-", "a:b", "0", "1", "007", "1.50", "0x1F",
            "1e3", ".inf", "yes", "null", "\u{3000}", "\u{3000}", " ", "é", "中", "\\", "\\",
            "a # b",
        ];
        assert_eq!(got, want.join("\n") + "\n");
    }

    #[test]
    fn paddle_dict_rejects_bad_input() {
        assert!(paddle_dict("PostProcess: {}").is_err());
        assert!(paddle_dict("PostProcess:\n  character_dict: [[a]]").is_err());
        assert!(paddle_dict("PostProcess:\n  character_dict: [\"a\\nb\"]").is_err());
        assert!(paddle_dict("{").is_err());
    }

    #[derive(Default)]
    struct Fake {
        files: HashMap<String, Vec<u8>>,
        chunk: usize,
        /// Per-path byte budgets for successive requests; a request ends with an error once its
        /// budget is spent.
        cut_after: Mutex<HashMap<String, Vec<usize>>>,
        /// Paths whose next request ignores the range and serves the whole file.
        ignore_range: Mutex<Vec<String>>,
        /// Paths whose next request serves this content instead.
        corrupt_once: Mutex<HashMap<String, Vec<u8>>>,
        requests: Mutex<Vec<(String, Option<Range<u64>>)>>,
    }

    impl Source for Fake {
        fn open<'a>(
            &'a self,
            source: &'a ModelSource,
            path: &'a str,
            range: Option<Range<u64>>,
        ) -> BoxFuture<'a, Result<ByteStream>> {
            assert_eq!(source.revision, REV);
            let key = format!("{}/{path}", source.repo);
            self.requests
                .lock()
                .unwrap()
                .push((key.clone(), range.clone()));
            let full = match self.corrupt_once.lock().unwrap().remove(&key) {
                Some(d) => d,
                None => self.files.get(&key).cloned().unwrap_or_default(),
            };
            let mut ignore = self.ignore_range.lock().unwrap();
            let data = match (&range, ignore.iter().position(|p| *p == key)) {
                (_, Some(i)) => {
                    ignore.remove(i);
                    full
                }
                (Some(r), None) => full[r.start as usize..r.end as usize].to_vec(),
                (None, None) => full,
            };
            let budget = self
                .cut_after
                .lock()
                .unwrap()
                .get_mut(&key)
                .and_then(|v| (!v.is_empty()).then(|| v.remove(0)));
            let chunk = self.chunk.max(1);
            let mut items: Vec<Result<Bytes>> = Vec::new();
            let mut sent = 0;
            for c in data.chunks(chunk) {
                if let Some(b) = budget {
                    if sent + c.len() > b {
                        items.push(Ok(Bytes::copy_from_slice(&c[..b - sent])));
                        items.push(Err(TranslatteError::Download("connection reset".into())));
                        break;
                    }
                }
                sent += c.len();
                items.push(Ok(Bytes::copy_from_slice(c)));
            }
            Box::pin(async move { Ok(Box::pin(futures::stream::iter(items)) as ByteStream) })
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        store: ModelStore,
        fake: Fake,
    }

    const BIG: usize = 10_000;

    fn fixture() -> Fixture {
        let big: Vec<u8> = (0..BIG).map(|i| (i * 7 % 251) as u8).collect();
        let dict = paddle_dict(YAML).unwrap();
        let entry = |path: &str, data: &[u8], source_path: &str, derive: bool| {
            format!(
                r#"{{"path": "{path}", "size": {}, "sha256": "{}", "source_path": "{source_path}"{}}}"#,
                data.len(),
                hex::encode(Sha256::digest(data)),
                if derive {
                    r#", "derive": "paddle_dict""#
                } else {
                    ""
                }
            )
        };
        let model = |id: &str, kind: &str, repo: &str, files: Vec<String>| {
            format!(
                r#"{{"id": "{id}", "kind": "{kind}", "display_name": "{id}", "source": {{"repo": "{repo}", "revision": "{REV}"}}, "files": [{}]}}"#,
                files.join(",")
            )
        };
        let json = format!(
            r#"{{"version": 1, "models": [{}, {}, {}], "languages": []}}"#,
            model(
                "det",
                "core",
                "o/det",
                vec![entry("det/inference.onnx", b"det", "inference.onnx", false)]
            ),
            model(
                "translator",
                "core",
                "o/tr",
                vec![entry("translator/model.gguf", &big, "up.gguf", false)]
            ),
            model(
                "rec-x",
                "script_pack",
                "o/rec",
                vec![
                    entry("rec-x/inference.onnx", b"rec", "inference.onnx", false),
                    entry("rec-x/dict.txt", &dict, "inference.yml", true),
                ]
            ),
        );
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(Manifest::from_json(&json).unwrap(), dir.path().into());
        let mut files = HashMap::new();
        files.insert("o/det/inference.onnx".into(), b"det".to_vec());
        files.insert("o/tr/up.gguf".into(), big);
        files.insert("o/rec/inference.onnx".into(), b"rec".to_vec());
        files.insert("o/rec/inference.yml".into(), YAML.as_bytes().to_vec());
        Fixture {
            _dir: dir,
            store,
            fake: Fake {
                files,
                chunk: 1000,
                ..Default::default()
            },
        }
    }

    fn run(fx: &Fixture, id: &str) -> (Result<()>, Vec<DownloadProgress>) {
        run_with(fx, id, &AtomicBool::new(false))
    }

    fn run_with(
        fx: &Fixture,
        id: &str,
        cancel: &AtomicBool,
    ) -> (Result<()>, Vec<DownloadProgress>) {
        let mut d = Downloader::with_source(&fx.store, &fx.fake);
        d.retry_delay = Duration::ZERO;
        d.progress_interval = Duration::ZERO;
        let events = Mutex::new(Vec::new());
        let r = d.download_model_blocking(id, cancel, |p| events.lock().unwrap().push(p));
        (r, events.into_inner().unwrap())
    }

    impl Source for &Fake {
        fn open<'a>(
            &'a self,
            source: &'a ModelSource,
            path: &'a str,
            range: Option<Range<u64>>,
        ) -> BoxFuture<'a, Result<ByteStream>> {
            (**self).open(source, path, range)
        }
    }

    fn requests(fx: &Fixture) -> Vec<(String, Option<Range<u64>>)> {
        std::mem::take(&mut *fx.fake.requests.lock().unwrap())
    }

    fn translator(fx: &Fixture) -> &ModelFile {
        &fx.store.manifest().model("translator").unwrap().files[0]
    }

    #[test]
    fn downloads_and_derives_files_then_is_idempotent() {
        let fx = fixture();
        let (r, events) = run(&fx, "rec-x");
        r.unwrap();
        assert_eq!(fx.store.state("rec-x").unwrap(), InstallState::Installed);
        assert!(fx.store.verify("rec-x").unwrap());
        assert_eq!(
            requests(&fx),
            vec![
                ("o/rec/inference.onnx".into(), Some(0..3)),
                ("o/rec/inference.yml".into(), None),
            ]
        );
        let last = events.last().unwrap();
        assert_eq!(last.file_path, "rec-x/dict.txt");
        assert_eq!(last.model_bytes_done, last.model_bytes_total);

        let (r, _) = run(&fx, "rec-x");
        r.unwrap();
        assert!(requests(&fx).is_empty());
    }

    #[test]
    fn resumes_from_existing_part() {
        let fx = fixture();
        let f = translator(&fx);
        let full = &fx.fake.files["o/tr/up.gguf"];
        std::fs::create_dir_all(fx.store.root().join("translator")).unwrap();
        std::fs::write(fx.store.part_path(f), &full[..4321]).unwrap();
        run(&fx, "translator").0.unwrap();
        assert_eq!(
            requests(&fx),
            vec![("o/tr/up.gguf".into(), Some(4321..BIG as u64))]
        );
        assert!(fx.store.verify("translator").unwrap());
        assert!(!fx.store.part_path(f).exists());
    }

    #[test]
    fn resumes_after_mid_stream_failures() {
        let fx = fixture();
        fx.fake
            .cut_after
            .lock()
            .unwrap()
            .insert("o/tr/up.gguf".into(), vec![2500, 0, 3000]);
        run(&fx, "translator").0.unwrap();
        let ranges: Vec<_> = requests(&fx).into_iter().map(|(_, r)| r.unwrap()).collect();
        assert_eq!(
            ranges,
            vec![0..10_000, 2500..10_000, 2500..10_000, 5500..10_000]
        );
        assert!(fx.store.verify("translator").unwrap());
    }

    #[test]
    fn gives_up_after_repeated_stalls_and_keeps_part() {
        let fx = fixture();
        fx.fake
            .cut_after
            .lock()
            .unwrap()
            .insert("o/tr/up.gguf".into(), vec![1000, 0, 0, 0, 0, 0, 0]);
        let (r, _) = run(&fx, "translator");
        assert!(matches!(r, Err(TranslatteError::Download(_))), "{r:?}");
        assert_eq!(requests(&fx).len(), 1 + MAX_STALLED_ATTEMPTS as usize);
        assert_eq!(
            part_len(&fx.store.part_path(translator(&fx))).unwrap(),
            1000
        );
    }

    #[test]
    fn restarts_once_from_zero_on_checksum_mismatch() {
        let fx = fixture();
        let mut bad = fx.fake.files["o/tr/up.gguf"].clone();
        bad[9000] ^= 0xff;
        fx.fake
            .corrupt_once
            .lock()
            .unwrap()
            .insert("o/tr/up.gguf".into(), bad);
        run(&fx, "translator").0.unwrap();
        let ranges: Vec<_> = requests(&fx).into_iter().map(|(_, r)| r.unwrap()).collect();
        assert_eq!(ranges, vec![0..10_000, 0..10_000]);
        assert!(fx.store.verify("translator").unwrap());
    }

    #[test]
    fn fails_after_second_checksum_mismatch() {
        let mut fx = fixture();
        fx.fake
            .files
            .get_mut("o/tr/up.gguf")
            .unwrap()
            .iter_mut()
            .for_each(|b| *b = !*b);
        let (r, _) = run(&fx, "translator");
        assert!(matches!(r, Err(TranslatteError::ChecksumMismatch { .. })));
        assert_eq!(requests(&fx).len(), 2);
        assert!(!fx.store.part_path(translator(&fx)).exists());
    }

    #[test]
    fn server_ignoring_range_is_caught_and_restarted() {
        let fx = fixture();
        let f = translator(&fx);
        let full = &fx.fake.files["o/tr/up.gguf"];
        std::fs::create_dir_all(fx.store.root().join("translator")).unwrap();
        std::fs::write(fx.store.part_path(f), &full[..100]).unwrap();
        fx.fake
            .ignore_range
            .lock()
            .unwrap()
            .push("o/tr/up.gguf".into());
        run(&fx, "translator").0.unwrap();
        let ranges: Vec<_> = requests(&fx).into_iter().map(|(_, r)| r.unwrap()).collect();
        assert_eq!(ranges, vec![100..10_000, 0..10_000]);
        assert!(fx.store.verify("translator").unwrap());
    }

    #[test]
    fn cancel_returns_cancelled_and_keeps_part() {
        let fx = fixture();
        let cancel = AtomicBool::new(false);
        let mut d = Downloader::with_source(&fx.store, &fx.fake);
        d.retry_delay = Duration::ZERO;
        d.progress_interval = Duration::ZERO;
        let r = d.download_model_blocking("translator", &cancel, |p| {
            if p.bytes_done >= 3000 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert!(matches!(r, Err(TranslatteError::Cancelled)), "{r:?}");
        let have = part_len(&fx.store.part_path(translator(&fx))).unwrap();
        assert!((3000..BIG as u64).contains(&have), "{have}");

        cancel.store(false, Ordering::Relaxed);
        d.download_model_blocking("translator", &cancel, |_| {})
            .unwrap();
        assert!(fx.store.verify("translator").unwrap());
    }

    #[test]
    fn already_cancelled_does_nothing() {
        let fx = fixture();
        let (r, _) = run_with(&fx, "det", &AtomicBool::new(true));
        assert!(matches!(r, Err(TranslatteError::Cancelled)));
        assert!(requests(&fx).is_empty());
    }

    #[test]
    fn redownloads_corrupt_installed_file() {
        let fx = fixture();
        run(&fx, "det").0.unwrap();
        std::fs::write(fx.store.root().join("det/inference.onnx"), "dex").unwrap();
        requests(&fx);
        run(&fx, "det").0.unwrap();
        assert_eq!(requests(&fx).len(), 1);
        assert!(fx.store.verify("det").unwrap());
    }

    #[test]
    fn derived_file_mismatch_is_an_error() {
        let mut fx = fixture();
        fx.fake.files.insert(
            "o/rec/inference.yml".into(),
            b"PostProcess:\n  character_dict: ['x']\n".to_vec(),
        );
        let (r, _) = run(&fx, "rec-x");
        assert!(r.is_err());
        assert!(!fx
            .store
            .part_path(&fx.store.manifest().model("rec-x").unwrap().files[1])
            .exists());
    }

    #[test]
    fn progress_is_monotonic_and_complete() {
        let fx = fixture();
        let (r, events) = run(&fx, "translator");
        r.unwrap();
        assert!(events
            .windows(2)
            .all(|w| w[0].model_bytes_done <= w[1].model_bytes_done));
        let last = events.last().unwrap();
        assert_eq!(last.bytes_done, BIG as u64);
        assert_eq!(last.model_bytes_total, BIG as u64);
    }

    #[test]
    fn download_models_runs_each_model() {
        let fx = fixture();
        let mut d = Downloader::with_source(&fx.store, &fx.fake);
        d.retry_delay = Duration::ZERO;
        d.progress_interval = Duration::ZERO;
        d.download_models_blocking(&["det", "rec-x"], &AtomicBool::new(false), |_| {})
            .unwrap();
        assert!(fx.store.verify("det").unwrap());
        assert!(fx.store.verify("rec-x").unwrap());
        assert!(matches!(
            d.download_model_blocking("nope", &AtomicBool::new(false), |_| {}),
            Err(TranslatteError::Manifest(_))
        ));
    }

    #[test]
    fn download_future_is_send() {
        fn assert_send<T: Send>(_: &T) {}
        let fx = fixture();
        let d = Downloader::with_source(&fx.store, &fx.fake);
        let cancel = AtomicBool::new(false);
        let fut = d.download_model("det", &cancel, |_| {});
        assert_send(&fut);
    }
}
