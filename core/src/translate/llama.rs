use std::num::NonZeroU32;
use std::path::Path;
use std::sync::OnceLock;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;

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

pub struct LlamaGenerator {
    model: LlamaModel,
    backend: &'static LlamaBackend,
    threads: i32,
}

impl LlamaGenerator {
    pub fn load(path: &Path) -> Result<Self> {
        let backend = backend()?;
        let model =
            LlamaModel::load_from_file(backend, path, &LlamaModelParams::default()).map_err(err)?;
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8) as i32)
            .unwrap_or(4);
        Ok(Self {
            model,
            backend,
            threads,
        })
    }
}

impl TextGenerator for LlamaGenerator {
    fn generate(&mut self, prompt: &Prompt, max_tokens: usize) -> Result<Generation> {
        let vocab = self.model.vocab();
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
        // A fresh context per call keeps the struct free of self-references; KV alloc is small next to decode time.
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_batch(N_CTX)
            .with_n_threads(self.threads)
            .with_n_threads_batch(self.threads);
        let mut ctx = self
            .model
            .new_context(self.backend, ctx_params)
            .map_err(err)?;

        let mut batch = LlamaBatch::new(N_CTX as usize, 1);
        let last = tokens.len() as i32 - 1;
        for (i, t) in (0_i32..).zip(tokens) {
            batch.add(t, i, &[0], i == last).map_err(err)?;
        }
        ctx.decode(&mut batch).map_err(err)?;

        let mut sampler = LlamaSampler::greedy();
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut out = String::new();
        let mut truncated = true;
        let start = batch.n_tokens();
        for pos in (start..).take(budget) {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
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
        }
        out.reserve(decoder.max_utf8_buffer_length(0).unwrap_or(4));
        let _ = decoder.decode_to_string(&[], &mut out, true);
        Ok(Generation {
            text: out,
            truncated,
        })
    }
}
