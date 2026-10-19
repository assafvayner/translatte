use std::num::NonZeroU32;
use std::path::Path;
use std::sync::OnceLock;

use llama_cpp_2::context::params::LlamaContextParams;
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
    threads: i32,
    /// Sliding-window attention width, if the model uses SWA.
    n_swa: Option<usize>,
    /// Tokens whose KV entries are in the context, at positions `0..cached.len()`.
    cached: Vec<LlamaToken>,
    prefix_cache: bool,
    /// Prompt tokens the last `generate` call took from the cache instead of decoding.
    reused: usize,
}

impl LlamaGenerator {
    pub fn load(path: &Path) -> Result<Self> {
        let backend = backend()?;
        let params = LlamaModelParams::default();
        // The iOS Simulator's Metal device lacks features ggml-metal needs, so decode fails there.
        #[cfg(all(target_os = "ios", target_abi = "sim"))]
        let params = params.with_n_gpu_layers(0);
        let model = LlamaModel::load_from_file(backend, path, &params).map_err(err)?;
        let threads = std::env::var("TRANSLATTE_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| {
                // On big.LITTLE phones extra threads land on little cores and stall the rest.
                let cap = if cfg!(target_os = "android") { 5 } else { 8 };
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
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_batch(N_CTX)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);
        let inner =
            ModelContext::try_new(model, |m| m.new_context(backend, ctx_params)).map_err(err)?;
        let prefix_cache = std::env::var("TRANSLATTE_PREFIX_CACHE").map_or(true, |v| v != "0");
        Ok(Self {
            inner,
            threads,
            n_swa,
            cached: Vec::new(),
            prefix_cache,
            reused: 0,
        })
    }

    /// Enables or disables reusing the KV entries of the token prefix shared with the previous call.
    pub fn with_prefix_cache(mut self, enabled: bool) -> Self {
        self.prefix_cache = enabled;
        self
    }

    /// Prompt tokens the last `generate` call reused from the KV cache.
    pub fn reused_tokens(&self) -> usize {
        self.reused
    }

    /// Tokens currently held in the KV cache.
    pub fn cached_tokens(&self) -> usize {
        self.cached.len()
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
        } = self;
        inner.with_dependent_mut(|model, ctx| {
            let vocab = model.vocab();
            // The user text is OCR output: tokenizing it without special parsing keeps a literal
            // "<end_of_turn>" in an image from becoming a control token.
            let mut tokens = vocab.tokenize(prompt.prefix.as_bytes(), true, true);
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
            let result = generate_from(ctx, &vocab, &tokens, n_keep, cached, budget, *threads);
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
            let tokens = model.vocab().tokenize(prefix.as_bytes(), true, true);
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

/// Decodes `tokens[n_keep..]` on top of the first `n_keep` cached entries, then samples greedily,
/// recording every token decoded into the context in `cached`.
fn generate_from(
    ctx: &mut LlamaContext,
    vocab: &LlamaVocab,
    tokens: &[LlamaToken],
    n_keep: usize,
    cached: &mut Vec<LlamaToken>,
    budget: usize,
    threads: i32,
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

    let mut sampler = LlamaSampler::greedy();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut out = String::new();
    let mut truncated = true;
    let mut n_gen = 0;
    for pos in (tokens.len() as i32..).take(budget) {
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        n_gen += 1;
        sampler.accept(token);
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
        if out.ends_with("<end_of_turn>") {
            truncated = false;
            break;
        }
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
            "llama: threads={threads} prompt {} tok ({n_keep} reused), prefill {n_prefill} tok in {:.2}s ({:.1} tok/s), decode {n_gen} tok in {:.2}s ({:.1} tok/s)",
            tokens.len(),
            prefill.as_secs_f64(),
            n_prefill as f64 / prefill.as_secs_f64(),
            decode.as_secs_f64(),
            n_gen as f64 / decode.as_secs_f64(),
        );
    }
    Ok(Generation {
        text: out,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(ids: &[i32]) -> Vec<LlamaToken> {
        ids.iter().copied().map(LlamaToken).collect()
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
