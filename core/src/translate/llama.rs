use std::num::NonZeroU32;
use std::path::Path;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;

use super::TextGenerator;
use crate::{Result, TranslatteError};

const N_CTX: u32 = 4096;

fn err(e: impl std::fmt::Display) -> TranslatteError {
    TranslatteError::Translate(e.to_string())
}

pub struct LlamaGenerator {
    backend: LlamaBackend,
    model: LlamaModel,
    threads: i32,
}

impl LlamaGenerator {
    pub fn load(path: &Path) -> Result<Self> {
        let mut backend = LlamaBackend::init().map_err(err)?;
        backend.void_logs();
        let model = LlamaModel::load_from_file(&backend, path, &LlamaModelParams::default())
            .map_err(err)?;
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8) as i32)
            .unwrap_or(4);
        Ok(Self {
            backend,
            model,
            threads,
        })
    }
}

impl TextGenerator for LlamaGenerator {
    fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String> {
        let vocab = self.model.vocab();
        let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
        if tokens.len() + max_tokens > N_CTX as usize {
            return Err(TranslatteError::Translate(format!(
                "prompt ({} tokens) + output budget ({max_tokens}) exceeds context {N_CTX}",
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
            .new_context(&self.backend, ctx_params)
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
        let start = batch.n_tokens();
        for pos in (start..).take(max_tokens) {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if vocab.is_eog(token) {
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
                break;
            }
            batch.clear();
            batch.add(token, pos, &[0], true).map_err(err)?;
            ctx.decode(&mut batch).map_err(err)?;
        }
        Ok(out)
    }
}
