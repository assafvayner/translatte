use std::num::NonZeroU32;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::OnceLock;

use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams};
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::vocab::LlamaVocab;

use super::prompt::Prompt;
use super::{Generation, TextGenerator};
use crate::{Result, TranslatteError};

const N_CTX: u32 = 4096;
/// Hy-MT2's recommended sampling (model card). Seeded so a given input and seed always get the same
/// output; [`LlamaGenerator::set_seed`] changes it (e.g. for a retry).
const TEMPERATURE: f32 = 0.7;
const TOP_K: i32 = 20;
const TOP_P: f32 = 0.6;
const REPEAT_PENALTY: f32 = 1.05;
const PENALTY_LAST_N: i32 = 64;
const DEFAULT_SEED: u32 = 0;

fn err(e: impl std::fmt::Display) -> TranslatteError {
    TranslatteError::Translate(e.to_string())
}

static BACKEND: OnceLock<std::result::Result<LlamaBackend, String>> = OnceLock::new();

/// The llama.cpp backend is process-global and can only be initialized once; it lives for the process.
fn backend() -> Result<&'static LlamaBackend> {
    BACKEND
        .get_or_init(|| {
            llama_cpp_2::send_logs_to_tracing(
                llama_cpp_2::LogOptions::default().with_logs_enabled(false),
            );
            LlamaBackend::init().map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(err)
}

self_cell::self_cell!(
    /// The model and the one context borrowing it, kept together so the KV cache survives across calls.
    struct ModelContext {
        owner: LlamaModel,
        #[covariant]
        dependent: LlamaContext,
    }
);

pub struct LlamaGenerator {
    inner: ModelContext,
    /// Declared after `inner` so it is freed after the context that uses it.
    _pool: Option<ThreadPool>,
    threads: i32,
    /// Sliding-window attention width, if the model uses SWA.
    n_swa: Option<usize>,
    /// Tokens whose KV entries are in the context, at positions `0..cached.len()`.
    cached: Vec<LlamaToken>,
    prefix_cache: bool,
    /// Prompt tokens the last `generate` call took from the cache instead of decoding.
    reused: usize,
    seed: u32,
}

impl LlamaGenerator {
    pub fn load(path: &Path) -> Result<Self> {
        let backend = backend()?;
        let params = LlamaModelParams::default();
        // The iOS Simulator's Metal device lacks features ggml-metal needs, so decode fails there.
        #[cfg(all(target_os = "ios", target_abi = "sim"))]
        let params = params.with_n_gpu_layers(0);
        let model = LlamaModel::load_from_file(backend, path, &params).map_err(err)?;
        let gpu = params.n_gpu_layers() > 0 && backend.supports_gpu_offload();
        let threads = std::env::var("TRANSLATTE_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or_else(|| {
                // On big.LITTLE phones extra threads land on little cores and stall the rest. iPhones
                // have only 2 P-cores; the simulator runs on the Mac's cores.
                let cap = if cfg!(target_os = "android") {
                    5
                } else if cfg!(all(target_os = "ios", not(target_abi = "sim"))) {
                    2
                } else {
                    8
                };
                std::thread::available_parallelism()
                    .map(|n| n.get().min(cap) as i32)
                    .unwrap_or(4)
            });
        let n_swa = model
            .meta_val_str("general.architecture")
            .and_then(|arch| model.meta_val_str(&format!("{arch}.attention.sliding_window")))
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n| n > 0);
        // llama.cpp otherwise sizes the sliding-window layers' KV cache for the full context, and an
        // 8-bit cache halves the rest; both leave more RAM to keep the mmapped weights resident.
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_batch(N_CTX)
            .with_swa_full(false)
            .with_type_k(KvCacheType::Q8_0)
            .with_type_v(KvCacheType::Q8_0)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);
        let mut inner =
            ModelContext::try_new(model, |m| m.new_context(backend, ctx_params)).map_err(err)?;
        let pool = ThreadPool::new(threads, !gpu).and_then(|pool| {
            inner
                .with_dependent_mut(|_, ctx| pool.attach(ctx))
                .then_some(pool)
        });
        let prefix_cache = std::env::var("TRANSLATTE_PREFIX_CACHE").map_or(true, |v| v != "0");
        Ok(Self {
            inner,
            _pool: pool,
            threads,
            n_swa,
            cached: Vec::new(),
            prefix_cache,
            reused: 0,
            seed: DEFAULT_SEED,
        })
    }

    /// Enables or disables reusing the KV entries of the token prefix shared with the previous call.
    pub fn with_prefix_cache(mut self, enabled: bool) -> Self {
        self.prefix_cache = enabled;
        self
    }

    /// Seed for sampling in later `generate` calls; a different seed can give a different
    /// translation of the same text.
    pub fn set_seed(&mut self, seed: u32) {
        self.seed = seed;
    }

    /// Prompt tokens the last `generate` call reused from the KV cache.
    pub fn reused_tokens(&self) -> usize {
        self.reused
    }

    /// Whether ggml runs this context on a persistent threadpool (see [`ThreadPool`]).
    #[doc(hidden)]
    pub fn threadpool_attached(&self) -> bool {
        self._pool.is_some()
    }

    /// Tokens currently held in the KV cache.
    pub fn cached_tokens(&self) -> usize {
        self.cached.len()
    }
}

/// ggml worker threads that persist across decode calls. Without one, ggml creates and joins a fresh
/// set of threads for every `decode`, i.e. for every generated token, and on phones the scheduler
/// starts those new threads on slow cores at low clocks.
struct ThreadPool(NonNull<llama_cpp_sys_2::ggml_threadpool>);

// SAFETY: the pool is only handed to llama.cpp, which drives it from the thread calling `decode`.
unsafe impl Send for ThreadPool {}

impl ThreadPool {
    /// With `poll`, idle workers spin briefly before sleeping, which keeps the per-token handoff
    /// fast when the CPU runs the whole graph. When the graph runs on the GPU the workers have
    /// little to do and spinning would only burn power, so they sleep right away.
    fn new(threads: i32, poll: bool) -> Option<Self> {
        // SAFETY: the params are initialized by ggml and only read by `ggml_threadpool_new`.
        unsafe {
            let mut params = llama_cpp_sys_2::ggml_threadpool_params_default(threads);
            if !poll {
                params.poll = 0;
            }
            NonNull::new(llama_cpp_sys_2::ggml_threadpool_new(&mut params)).map(Self)
        }
    }

    /// Makes `ctx` run its graphs on this pool. Returns false if the context pointer is unavailable.
    fn attach(&self, ctx: &mut LlamaContext) -> bool {
        // llama-cpp-2 exposes no raw context pointer or threadpool API; its Debug output prints the
        // pointer. The crate is pinned (core/Cargo.toml) and `pointer_in` accepts only that exact format.
        let Some(raw) = pointer_in(&format!("{ctx:?}")) else {
            return false;
        };
        // SAFETY: `raw` is the live context `ctx` wraps, borrowed mutably here; the pool outlives it.
        unsafe {
            llama_cpp_sys_2::llama_attach_threadpool(
                raw as *mut llama_cpp_sys_2::llama_context,
                self.0.as_ptr(),
                self.0.as_ptr(),
            )
        };
        true
    }
}

/// The pointer in llama-cpp-2 0.1.160's `LlamaContext` Debug output, `LlamaContext { context: 0x… }`;
/// `None` for anything else.
fn pointer_in(debug: &str) -> Option<usize> {
    let hex = debug
        .strip_prefix("LlamaContext { context: 0x")?
        .strip_suffix(" }")?;
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    usize::from_str_radix(hex, 16).ok().filter(|&p| p != 0)
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        // SAFETY: created by `ggml_threadpool_new`; the context using it was dropped first.
        unsafe { llama_cpp_sys_2::ggml_threadpool_free(self.0.as_ptr()) }
    }
}

/// How many leading tokens can keep their KV entries from the previous call. Once a sequence
/// reaches the sliding attention window, llama.cpp may overwrite its early cells, so nothing is
/// reused.
fn shared_prefix(cached: &[LlamaToken], tokens: &[LlamaToken], n_swa: Option<usize>) -> usize {
    if n_swa.is_some_and(|w| cached.len() >= w) {
        return 0;
    }
    cached
        .iter()
        .zip(tokens)
        .take_while(|(a, b)| a == b)
        .count()
}

/// Like [`shared_prefix`], but always leaves a prompt token to decode so the last position gets
/// fresh logits.
fn reusable_prefix(cached: &[LlamaToken], tokens: &[LlamaToken], n_swa: Option<usize>) -> usize {
    shared_prefix(cached, tokens, n_swa).min(tokens.len().saturating_sub(1))
}

/// Drops the KV entries at positions `n_keep..`.
fn truncate_cache(
    ctx: &mut LlamaContext,
    cached: &mut Vec<LlamaToken>,
    n_keep: usize,
) -> Result<()> {
    if n_keep == 0 {
        ctx.clear_kv_cache();
    } else {
        ctx.kv_cache_seq_rm(0, Some(n_keep as u32), None)
            .map_err(err)?;
    }
    cached.truncate(n_keep);
    Ok(())
}

impl TextGenerator for LlamaGenerator {
    fn generate(&mut self, prompt: &Prompt, max_tokens: usize) -> Result<Generation> {
        let Self {
            inner,
            threads,
            n_swa,
            cached,
            prefix_cache,
            reused,
            seed,
            _pool: pool,
        } = self;
        inner.with_dependent_mut(|model, ctx| {
            let vocab = model.vocab();
            // The user text is OCR output: tokenizing it without special parsing keeps a literal
            // control-token string in an image from becoming a control token. The prompt carries
            // the template's BOS, so the tokenizer adds none.
            let mut tokens = vocab.tokenize(prompt.prefix.as_bytes(), false, true);
            tokens.extend(vocab.tokenize(prompt.text.as_bytes(), false, false));
            tokens.extend(vocab.tokenize(prompt.suffix.as_bytes(), false, true));
            let budget = max_tokens.min((N_CTX as usize).saturating_sub(tokens.len()));
            if budget == 0 {
                return Err(TranslatteError::Translate(format!(
                    "prompt ({} tokens) leaves no room for output in context {N_CTX}",
                    tokens.len()
                )));
            }
            let n_keep = if *prefix_cache {
                reusable_prefix(cached, &tokens, *n_swa)
            } else {
                0
            };
            let perf_tag = format!(
                "threads={threads} pool={}",
                if pool.is_some() { "on" } else { "off" }
            );
            let result = generate_from(
                ctx, &vocab, &tokens, n_keep, cached, budget, &perf_tag, *seed,
            );
            match &result {
                Ok(_) => *reused = n_keep,
                Err(_) => {
                    ctx.clear_kv_cache();
                    cached.clear();
                }
            }
            result
        })
    }

    fn warm(&mut self, prefix: &str) -> Result<()> {
        if !self.prefix_cache {
            return Ok(());
        }
        let Self {
            inner,
            threads,
            n_swa,
            cached,
            ..
        } = self;
        inner.with_dependent_mut(|model, ctx| {
            let tokens = model.vocab().tokenize(prefix.as_bytes(), false, true);
            let n_keep = shared_prefix(cached, &tokens, *n_swa);
            let t0 = std::time::Instant::now();
            let result = warm_from(ctx, &tokens, n_keep, cached);
            match &result {
                Ok(()) if std::env::var_os("TRANSLATTE_PERF").is_some() => eprintln!(
                    "llama: threads={threads} warm {} tok ({n_keep} reused) in {:.2}s",
                    tokens.len(),
                    t0.elapsed().as_secs_f64(),
                ),
                Ok(()) => {}
                Err(_) => {
                    ctx.clear_kv_cache();
                    cached.clear();
                }
            }
            result
        })
    }
}

/// Decodes `tokens[n_keep..]` into the context without computing logits.
fn warm_from(
    ctx: &mut LlamaContext,
    tokens: &[LlamaToken],
    n_keep: usize,
    cached: &mut Vec<LlamaToken>,
) -> Result<()> {
    truncate_cache(ctx, cached, n_keep)?;
    if n_keep == tokens.len() {
        return Ok(());
    }
    let mut batch = LlamaBatch::new(N_CTX as usize, 1);
    for (i, &t) in (n_keep as i32..).zip(&tokens[n_keep..]) {
        batch.add(t, i, &[0], false).map_err(err)?;
    }
    ctx.decode(&mut batch).map_err(err)?;
    cached.extend_from_slice(&tokens[n_keep..]);
    Ok(())
}

/// Decodes `tokens[n_keep..]` on top of the first `n_keep` cached entries, then samples,
/// recording every token decoded into the context in `cached`.
#[allow(clippy::too_many_arguments)]
fn generate_from(
    ctx: &mut LlamaContext,
    vocab: &LlamaVocab,
    tokens: &[LlamaToken],
    n_keep: usize,
    cached: &mut Vec<LlamaToken>,
    budget: usize,
    perf_tag: &str,
    seed: u32,
) -> Result<Generation> {
    truncate_cache(ctx, cached, n_keep)?;

    let mut batch = LlamaBatch::new(N_CTX as usize, 1);
    let last = tokens.len() as i32 - 1;
    for (i, &t) in (n_keep as i32..).zip(&tokens[n_keep..]) {
        batch.add(t, i, &[0], i == last).map_err(err)?;
    }
    let t0 = std::time::Instant::now();
    let n_prefill = batch.n_tokens();
    ctx.decode(&mut batch).map_err(err)?;
    cached.extend_from_slice(&tokens[n_keep..]);
    let prefill = t0.elapsed();

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(vocab.n_tokens(), PENALTY_LAST_N, REPEAT_PENALTY, 0.0, 0.0),
        LlamaSampler::top_k(TOP_K),
        LlamaSampler::top_p(TOP_P, 1),
        LlamaSampler::temp(TEMPERATURE),
        LlamaSampler::dist(seed),
    ]);
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut out = String::new();
    let mut truncated = true;
    let mut n_gen = 0;
    for pos in (tokens.len() as i32..).take(budget) {
        // `sample` also accepts the token into the sampler (for the repetition penalty).
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        n_gen += 1;
        if vocab.is_eog(token) {
            truncated = false;
            break;
        }
        let bytes = vocab.token_to_piece(token, true, None);
        out.reserve(
            decoder
                .max_utf8_buffer_length(bytes.len())
                .unwrap_or(bytes.len() * 4),
        );
        let _ = decoder.decode_to_string(&bytes, &mut out, false);
        batch.clear();
        batch.add(token, pos, &[0], true).map_err(err)?;
        ctx.decode(&mut batch).map_err(err)?;
        cached.push(token);
    }
    out.reserve(decoder.max_utf8_buffer_length(0).unwrap_or(4));
    let _ = decoder.decode_to_string(&[], &mut out, true);
    if std::env::var_os("TRANSLATTE_PERF").is_some() {
        let decode = t0.elapsed() - prefill;
        eprintln!(
            "llama: {perf_tag} prompt {} tok ({n_keep} reused), prefill {n_prefill} tok in {:.2}s ({:.1} tok/s), decode {n_gen} tok in {:.2}s ({:.1} tok/s){}",
            tokens.len(),
            prefill.as_secs_f64(),
            n_prefill as f64 / prefill.as_secs_f64(),
            decode.as_secs_f64(),
            n_gen as f64 / decode.as_secs_f64(),
            mem_status(),
        );
    }
    Ok(Generation {
        text: out,
        truncated,
    })
}

/// Peak and current RSS and major page faults, on Linux/Android, for the perf log.
fn mem_status() -> String {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return String::new();
    };
    let mut out: String = status
        .lines()
        .filter(|l| {
            ["VmHWM", "RssAnon", "RssFile", "VmSwap"]
                .iter()
                .any(|k| l.starts_with(k))
        })
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            Some(format!(" {}{}", f.next()?, f.next()?))
        })
        .collect();
    // Field 12 of /proc/self/stat (the 10th after the parenthesized command name).
    if let Some(majflt) = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|s| Some(s.rsplit(')').next()?.split_whitespace().nth(9)?.to_owned()))
    {
        out.push_str(&format!(" majflt:{majflt}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(ids: &[i32]) -> Vec<LlamaToken> {
        ids.iter().copied().map(LlamaToken).collect()
    }

    #[test]
    fn finds_context_pointer_in_debug_output() {
        assert_eq!(
            pointer_in("LlamaContext { context: 0x7b1c2a3000 }"),
            Some(0x7b1c2a3000)
        );
        assert_eq!(pointer_in("LlamaContext { context: 0x0 }"), None);
        assert_eq!(pointer_in("LlamaContext { .. }"), None);
        assert_eq!(pointer_in("LlamaContext { context: 0x }"), None);
        assert_eq!(pointer_in("LlamaContext { context: 0x7b1c, n: 0x1 }"), None);
        assert_eq!(pointer_in("Other { context: 0x7b1c }"), None);
        assert_eq!(pointer_in("LlamaContext { context: 0x7b1c } "), None);
    }

    #[test]
    fn reuses_longest_common_prefix() {
        let cached = toks(&[1, 2, 3, 4, 5, 6]);
        assert_eq!(reusable_prefix(&cached, &toks(&[1, 2, 3, 9, 9]), None), 3);
        assert_eq!(reusable_prefix(&cached, &toks(&[7, 2, 3]), None), 0);
        assert_eq!(reusable_prefix(&[], &toks(&[1, 2]), None), 0);
    }

    #[test]
    fn always_leaves_a_token_to_decode() {
        let cached = toks(&[1, 2, 3, 4]);
        assert_eq!(reusable_prefix(&cached, &toks(&[1, 2, 3, 4]), None), 3);
        assert_eq!(reusable_prefix(&cached, &toks(&[1, 2]), None), 1);
        assert_eq!(reusable_prefix(&cached, &toks(&[1]), None), 0);
    }

    #[test]
    fn extends_into_previous_output() {
        let cached = toks(&[1, 2, 3, 4]);
        assert_eq!(
            reusable_prefix(&cached, &toks(&[1, 2, 3, 4, 5, 6]), None),
            4
        );
    }

    #[test]
    fn shared_prefix_may_cover_all_tokens() {
        let cached = toks(&[1, 2, 3, 4]);
        assert_eq!(shared_prefix(&cached, &toks(&[1, 2, 3]), None), 3);
        assert_eq!(shared_prefix(&cached, &toks(&[1, 2, 3, 4, 5]), None), 4);
        assert_eq!(shared_prefix(&cached, &toks(&[1, 2, 3]), Some(4)), 0);
    }

    #[test]
    fn nothing_reused_once_sliding_window_was_reached() {
        let cached = toks(&[1, 2, 3, 4]);
        let tokens = toks(&[1, 2, 3, 9]);
        assert_eq!(reusable_prefix(&cached, &tokens, Some(5)), 3);
        assert_eq!(reusable_prefix(&cached, &tokens, Some(4)), 0);
    }
}
