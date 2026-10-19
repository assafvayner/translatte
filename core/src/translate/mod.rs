pub mod batch;
pub mod prompt;

use crate::manifest::Language;
use crate::Result;

/// Max blocks per LLM call; keeps prompt + output well inside a 4096-token context.
pub const CHUNK_SIZE: usize = 40;

/// Raw text generation (one prompt in, completion out). Implemented by the LLM backend.
pub trait TextGenerator {
    fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String>;
}

/// Translates a chunk of at most CHUNK_SIZE blocks. `None` marks a block that could not be translated.
pub trait Translator {
    fn translate(
        &mut self,
        blocks: &[String],
        src: &Language,
        tgt: &Language,
    ) -> Result<Vec<Option<String>>>;
}
