//! Resumable model downloads straight from the pinned upstream Hugging Face repos.
//!
//! Files are streamed into `<file>.part` (appending from its current length via a byte-range
//! request), then `ModelStore::commit_download` checks the size and renames it into place.
//! Content integrity is left to the transport (TLS, and hf-xet's chunk hashes for Xet-backed files).

use std::fs::OpenOptions;
use std::future::Future;
use std::io::{self, Write};
use std::ops::Range;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bytes::Bytes;
use futures::future::BoxFuture;
use futures::{Stream, StreamExt};
use serde::Deserialize;

use crate::manifest::{Derive, ModelEntry, ModelFile, ModelSource};
use crate::model_store::ModelStore;
use crate::{Result, TranslatteError};

#[cfg(not(any(feature = "tls-rustls", feature = "tls-native")))]
compile_error!("`download` needs a TLS backend: enable `tls-rustls` or `tls-native`");
#[cfg(all(feature = "tls-rustls", feature = "tls-native"))]
compile_error!("`tls-rustls` and `tls-native` are mutually exclusive");

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>;

pub struct Opened {
    pub stream: ByteStream,
    /// Length of the body, when known. For a ranged request, the full file length means the
    /// server ignored the range and is sending the file from the start.
    pub len: Option<u64>,
}

/// Where file bytes come from: [`HubSource`] in production, fakes in tests.
pub trait Source: Send + Sync {
    /// Streams `path` from the pinned `source` repo, limited to `range` when given.
    fn open<'a>(
        &'a self,
        source: &'a ModelSource,
        path: &'a str,
        range: Option<Range<u64>>,
    ) -> BoxFuture<'a, Result<Opened>>;
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
    ) -> BoxFuture<'a, Result<Opened>> {
        Box::pin(async move {
            let (owner, name) = hf_hub::split_id(&source.repo);
            let (len, stream) = self
                .client
                .model(owner, name)
                .download_file_stream()
                .filename(path)
                .revision(source.revision.as_str())
                .maybe_range(range)
                .send()
                .await
                .map_err(download_err)?;
            Ok(Opened {
                stream: Box::pin(stream.map(|r| r.map_err(download_err))),
                len,
            })
        })
    }
}

fn download_err(e: impl std::fmt::Display) -> TranslatteError {
    TranslatteError::Download(e.to_string())
}

/// `bytes_done` (and with it `model_bytes_done`) drops back to 0 when a file restarts from
/// scratch because the server ignored the resume range.
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
/// Xet-backed files arrive in order while many byte ranges download in parallel, so the first chunk
/// can take minutes on a slow link even though data is flowing; restarting then throws that away.
const FIRST_CHUNK_TIMEOUT: Duration = Duration::from_secs(600);
const CANCEL_POLL: Duration = Duration::from_millis(200);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
/// Upper bound on a source file that is parsed rather than stored (e.g. `inference.yml`).
const MAX_DERIVE_SOURCE_BYTES: usize = 16 << 20;

pub struct Downloader<'a> {
    store: &'a ModelStore,
    source: Box<dyn Source + 'a>,
    retry_delay: Duration,
    idle_timeout: Duration,
    first_chunk_timeout: Duration,
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
            first_chunk_timeout: FIRST_CHUNK_TIMEOUT,
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
            if !self.store.is_file_installed(f)? {
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

    /// [`Self::download_model`] on a private current-thread runtime. Panics if called from
    /// inside a tokio runtime; use the async version there.
    pub fn download_model_blocking(
        &self,
        id: &str,
        cancel: &AtomicBool,
        progress: impl Fn(DownloadProgress) + Send + Sync,
    ) -> Result<()> {
        block_on(self.download_model(id, cancel, progress))
    }

    /// [`Self::download_models`] on a private current-thread runtime. Panics if called from
    /// inside a tokio runtime; use the async version there.
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
        loop {
            check_cancel(cancel)?;
            let before = part_len(&part)?;
            let failure = if before < f.size {
                match self
                    .transfer(entry, f, &part, before, cancel, reporter)
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
                    Err(e @ TranslatteError::SizeMismatch { .. }) => e,
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
            self.backoff(stalled, cancel).await?;
        }
    }

    /// Requests `have..size` and appends it to the `.part`, or rewrites the `.part` from the
    /// start when the server ignores the range.
    async fn transfer(
        &self,
        entry: &ModelEntry,
        f: &ModelFile,
        part: &Path,
        mut have: u64,
        cancel: &AtomicBool,
        reporter: &mut Reporter<'_>,
    ) -> Result<()> {
        let opened = self
            .watch(
                self.source
                    .open(&entry.source, &f.source_path, Some(have..f.size)),
                cancel,
            )
            .await??;
        let mut stream = opened.stream;
        let mut out = if have > 0 && opened.len == Some(f.size) {
            have = 0;
            reporter.report(f, 0, true);
            OpenOptions::new().write(true).truncate(true).open(part)?
        } else {
            OpenOptions::new().create(true).append(true).open(part)?
        };
        let mut timeout = self.first_chunk_timeout;
        while have < f.size {
            let Some(chunk) = self.watch_for(stream.next(), timeout, cancel).await? else {
                break;
            };
            timeout = self.idle_timeout;
            let chunk = chunk?;
            // Bytes past the expected size are never written.
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
        let part = self.store.part_path(f);
        let mut stalled = 0;
        loop {
            check_cancel(cancel)?;
            let failure = match self.derive_once(entry, f, kind, cancel).await {
                Ok(()) => match self.store.commit_download(f) {
                    Ok(()) => return Ok(()),
                    Err(TranslatteError::Incomplete { path, have, want }) => {
                        std::fs::remove_file(&part)?;
                        TranslatteError::Download(format!(
                            "derived {path} has {have} bytes, expected {want}"
                        ))
                    }
                    Err(TranslatteError::SizeMismatch { path, have, want }) => {
                        TranslatteError::Download(format!(
                            "derived {path} has {have} bytes, expected {want}"
                        ))
                    }
                    Err(e) => return Err(e),
                },
                Err(e @ TranslatteError::Download(_)) => e,
                Err(e) => return Err(e),
            };
            stalled += 1;
            if stalled >= MAX_STALLED_ATTEMPTS {
                return Err(failure);
            }
            self.backoff(stalled, cancel).await?;
        }
    }

    async fn derive_once(
        &self,
        entry: &ModelEntry,
        f: &ModelFile,
        kind: Derive,
        cancel: &AtomicBool,
    ) -> Result<()> {
        let opened = self
            .watch(
                self.source.open(&entry.source, &f.source_path, None),
                cancel,
            )
            .await??;
        let raw = self
            .collect(opened.stream, MAX_DERIVE_SOURCE_BYTES, cancel)
            .await?;
        let data = match kind {
            Derive::PaddleDict => {
                let text = std::str::from_utf8(&raw).map_err(|_| {
                    TranslatteError::Download(format!("{} is not UTF-8", f.source_path))
                })?;
                paddle_dict(text)?
            }
        };
        std::fs::write(self.store.part_path(f), data)?;
        Ok(())
    }

    async fn collect(
        &self,
        mut stream: ByteStream,
        limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        while let Some(chunk) = self.watch(stream.next(), cancel).await? {
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

    /// Awaits `fut`, checking `cancel` every [`CANCEL_POLL`] and failing with a `Download`
    /// error once it has made no progress for the idle timeout.
    async fn watch<F: Future>(&self, fut: F, cancel: &AtomicBool) -> Result<F::Output> {
        self.watch_for(fut, self.idle_timeout, cancel).await
    }

    async fn watch_for<F: Future>(
        &self,
        fut: F,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<F::Output> {
        let mut fut = std::pin::pin!(fut);
        let mut idle = Duration::ZERO;
        loop {
            check_cancel(cancel)?;
            match tokio::time::timeout(CANCEL_POLL, fut.as_mut()).await {
                Ok(out) => return Ok(out),
                Err(_) => {
                    idle += CANCEL_POLL;
                    if idle >= timeout {
                        return Err(TranslatteError::Download(format!(
                            "no response for {}s",
                            idle.as_secs()
                        )));
                    }
                }
            }
        }
    }

    async fn backoff(&self, stalled: u32, cancel: &AtomicBool) -> Result<()> {
        let end = tokio::time::Instant::now() + self.retry_delay * stalled;
        loop {
            check_cancel(cancel)?;
            let now = tokio::time::Instant::now();
            if now >= end {
                return Ok(());
            }
            tokio::time::sleep((end - now).min(CANCEL_POLL)).await;
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

struct Reporter<'p> {
    progress: &'p (dyn Fn(DownloadProgress) + Send + Sync),
    model_id: &'p str,
    model_bytes_total: u64,
    model_bytes_before: u64,
    interval: Duration,
    last: Option<std::time::Instant>,
}

impl Reporter<'_> {
    fn report(&mut self, f: &ModelFile, bytes_done: u64, force: bool) {
        let now = std::time::Instant::now();
        // The final update for a file is always sent forced, once the file is committed.
        if !force && (bytes_done >= f.size || self.last.is_some_and(|t| now - t < self.interval)) {
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
/// sizes.
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
    use std::sync::{Arc, Mutex};

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
        /// Number of upcoming requests per path whose `open` fails.
        open_errors: Mutex<HashMap<String, u32>>,
        /// Every `open` never resolves, after setting this flag if given.
        hang_open: Option<Option<Arc<AtomicBool>>>,
        /// Streams wait this long before their first chunk, or forever when `Duration::MAX`.
        first_chunk_delay: Option<Duration>,
        /// Streams hang after their first chunk.
        stall_after_first_chunk: bool,
        requests: Mutex<Vec<(String, Option<Range<u64>>)>>,
    }

    impl Source for Fake {
        fn open<'a>(
            &'a self,
            source: &'a ModelSource,
            path: &'a str,
            range: Option<Range<u64>>,
        ) -> BoxFuture<'a, Result<Opened>> {
            assert_eq!(source.revision, REV);
            let key = format!("{}/{path}", source.repo);
            self.requests
                .lock()
                .unwrap()
                .push((key.clone(), range.clone()));
            if let Some(flag) = &self.hang_open {
                if let Some(flag) = flag {
                    flag.store(true, Ordering::Relaxed);
                }
                return Box::pin(futures::future::pending());
            }
            if let Some(n) = self.open_errors.lock().unwrap().get_mut(&key) {
                if *n > 0 {
                    *n -= 1;
                    return Box::pin(async { Err(TranslatteError::Download("503".into())) });
                }
            }
            let full = self.files.get(&key).cloned().unwrap_or_default();
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
            let len = Some(data.len() as u64);
            let delay = self.first_chunk_delay;
            let stall = self.stall_after_first_chunk;
            Box::pin(async move {
                let items = futures::stream::iter(items);
                let stream: ByteStream = match delay {
                    None if stall => Box::pin(items.take(1).chain(futures::stream::pending())),
                    None => Box::pin(items),
                    Some(d) if d == Duration::MAX => Box::pin(futures::stream::pending()),
                    Some(d) => Box::pin(
                        futures::stream::once(tokio::time::sleep(d))
                            .filter_map(|_| async { None })
                            .chain(items),
                    ),
                };
                Ok(Opened { stream, len })
            })
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
                r#"{{"path": "{path}", "size": {}, "source_path": "{source_path}"{}}}"#,
                data.len(),
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
        ) -> BoxFuture<'a, Result<Opened>> {
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
        assert_eq!(fx.store.state("rec-x").unwrap(), InstallState::Installed);
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
        assert_eq!(
            fx.store.state("translator").unwrap(),
            InstallState::Installed
        );
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
        assert_eq!(
            fx.store.state("translator").unwrap(),
            InstallState::Installed
        );
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
    fn oversize_part_is_discarded_and_redownloaded() {
        let fx = fixture();
        let f = translator(&fx);
        std::fs::create_dir_all(fx.store.part_path(f).parent().unwrap()).unwrap();
        std::fs::write(fx.store.part_path(f), vec![0u8; BIG + 5]).unwrap();
        run(&fx, "translator").0.unwrap();
        assert_eq!(
            fx.store.state("translator").unwrap(),
            InstallState::Installed
        );
        assert!(!fx.store.part_path(f).exists());
    }

    #[test]
    fn server_ignoring_range_rewrites_part_from_start() {
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
        let (r, events) = run(&fx, "translator");
        r.unwrap();
        let ranges: Vec<_> = requests(&fx).into_iter().map(|(_, r)| r.unwrap()).collect();
        assert_eq!(ranges, vec![100..10_000]);
        assert_eq!(
            fx.store.state("translator").unwrap(),
            InstallState::Installed
        );
        assert_eq!(events[0].bytes_done, 0);
    }

    #[test]
    fn open_errors_are_retried_then_give_up() {
        let fx = fixture();
        fx.fake
            .open_errors
            .lock()
            .unwrap()
            .insert("o/tr/up.gguf".into(), 2);
        run(&fx, "translator").0.unwrap();
        assert_eq!(requests(&fx).len(), 3);
        assert_eq!(
            fx.store.state("translator").unwrap(),
            InstallState::Installed
        );

        fx.fake
            .open_errors
            .lock()
            .unwrap()
            .insert("o/det/inference.onnx".into(), 100);
        let (r, _) = run(&fx, "det");
        assert!(matches!(r, Err(TranslatteError::Download(_))), "{r:?}");
        assert_eq!(requests(&fx).len(), MAX_STALLED_ATTEMPTS as usize);
    }

    #[test]
    fn derive_retries_failed_source_fetch() {
        let fx = fixture();
        fx.fake
            .open_errors
            .lock()
            .unwrap()
            .insert("o/rec/inference.yml".into(), 2);
        run(&fx, "rec-x").0.unwrap();
        assert_eq!(fx.store.state("rec-x").unwrap(), InstallState::Installed);
        let yml = requests(&fx)
            .into_iter()
            .filter(|(k, _)| k.ends_with(".yml"))
            .count();
        assert_eq!(yml, 3);
    }

    /// So that a `rec-x` download starts with the derived dict.
    fn install_rec_onnx(fx: &Fixture) {
        std::fs::create_dir_all(fx.store.root().join("rec-x")).unwrap();
        std::fs::write(fx.store.root().join("rec-x/inference.onnx"), "rec").unwrap();
    }

    fn paused_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap()
    }

    #[test]
    fn hanging_open_times_out_into_bounded_retries() {
        for id in ["translator", "rec-x"] {
            let mut fx = fixture();
            fx.fake.hang_open = Some(None);
            install_rec_onnx(&fx);
            let d = Downloader::with_source(&fx.store, &fx.fake);
            let rt = paused_runtime();
            let start = rt.block_on(async { tokio::time::Instant::now() });
            let r = rt.block_on(d.download_model(id, &AtomicBool::new(false), |_| {}));
            assert!(matches!(r, Err(TranslatteError::Download(_))), "{r:?}");
            assert_eq!(requests(&fx).len(), MAX_STALLED_ATTEMPTS as usize);
            let waited = rt.block_on(async move { start.elapsed() });
            assert!(waited >= IDLE_TIMEOUT * MAX_STALLED_ATTEMPTS, "{waited:?}");
        }
    }

    #[test]
    fn waits_past_idle_timeout_for_first_chunk() {
        let mut fx = fixture();
        fx.fake.first_chunk_delay = Some(IDLE_TIMEOUT * 3);
        install_rec_onnx(&fx);
        let d = Downloader::with_source(&fx.store, &fx.fake);
        let rt = paused_runtime();
        rt.block_on(d.download_model("translator", &AtomicBool::new(false), |_| {}))
            .unwrap();
        assert_eq!(requests(&fx).len(), 1);
        assert_eq!(
            fx.store.state("translator").unwrap(),
            InstallState::Installed
        );
    }

    #[test]
    fn stall_after_first_chunk_times_out_at_idle_timeout() {
        let mut fx = fixture();
        fx.fake.stall_after_first_chunk = true;
        let d = Downloader::with_source(&fx.store, &fx.fake);
        let rt = paused_runtime();
        let start = rt.block_on(async { tokio::time::Instant::now() });
        rt.block_on(d.download_model("translator", &AtomicBool::new(false), |_| {}))
            .unwrap();
        // One chunk per request, each followed by an idle timeout, until the file is complete.
        let n = requests(&fx).len() as u32;
        assert_eq!(n as usize, BIG / fx.fake.chunk);
        let waited = rt.block_on(async move { start.elapsed() });
        assert!(waited >= IDLE_TIMEOUT * (n - 1), "{waited:?}");
        assert!(waited < (IDLE_TIMEOUT + RETRY_DELAY) * n, "{waited:?}");
    }

    #[test]
    fn missing_first_chunk_times_out_into_bounded_retries() {
        let mut fx = fixture();
        fx.fake.first_chunk_delay = Some(Duration::MAX);
        let d = Downloader::with_source(&fx.store, &fx.fake);
        let rt = paused_runtime();
        let start = rt.block_on(async { tokio::time::Instant::now() });
        let r = rt.block_on(d.download_model("translator", &AtomicBool::new(false), |_| {}));
        assert!(matches!(r, Err(TranslatteError::Download(_))), "{r:?}");
        assert_eq!(requests(&fx).len(), MAX_STALLED_ATTEMPTS as usize);
        let waited = rt.block_on(async move { start.elapsed() });
        assert!(
            waited >= FIRST_CHUNK_TIMEOUT * MAX_STALLED_ATTEMPTS,
            "{waited:?}"
        );
    }

    #[test]
    fn hanging_open_responds_to_cancel() {
        for id in ["translator", "rec-x"] {
            let cancel = Arc::new(AtomicBool::new(false));
            let mut fx = fixture();
            fx.fake.hang_open = Some(Some(cancel.clone()));
            install_rec_onnx(&fx);
            let d = Downloader::with_source(&fx.store, &fx.fake);
            let rt = paused_runtime();
            let start = rt.block_on(async { tokio::time::Instant::now() });
            let r = rt.block_on(d.download_model(id, &cancel, |_| {}));
            assert!(matches!(r, Err(TranslatteError::Cancelled)), "{r:?}");
            assert_eq!(requests(&fx).len(), 1);
            let waited = rt.block_on(async move { start.elapsed() });
            assert!(waited <= CANCEL_POLL * 2, "{waited:?}");
        }
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
        assert_eq!(
            fx.store.state("translator").unwrap(),
            InstallState::Installed
        );
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
        std::fs::write(fx.store.root().join("det/inference.onnx"), "de").unwrap();
        requests(&fx);
        run(&fx, "det").0.unwrap();
        assert_eq!(requests(&fx).len(), 1);
        assert_eq!(fx.store.state("det").unwrap(), InstallState::Installed);
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
        let done: Vec<_> = events
            .iter()
            .filter(|e| e.bytes_done == e.bytes_total)
            .collect();
        assert_eq!(done.len(), 1);
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
        assert_eq!(fx.store.state("det").unwrap(), InstallState::Installed);
        assert_eq!(fx.store.state("rec-x").unwrap(), InstallState::Installed);
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
