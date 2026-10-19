pub mod batch;
#[cfg(feature = "llama")]
pub mod llama;
pub mod prompt;

use std::ops::Range;

use prompt::Prompt;

use crate::manifest::Language;
use crate::Result;

/// Max blocks per LLM call; keeps prompt + output well inside a 4096-token context.
pub const CHUNK_SIZE: usize = 40;
/// Max source chars per LLM call, so a chunk of long blocks stays within the output budget.
pub const MAX_CHUNK_CHARS: usize = 600;

pub struct Generation {
    pub text: String,
    /// Stopped by the token budget or context limit rather than end-of-generation.
    pub truncated: bool,
}

/// Raw text generation (one prompt in, completion out). Implemented by the LLM backend.
pub trait TextGenerator {
    fn generate(&mut self, prompt: &Prompt, max_tokens: usize) -> Result<Generation>;
}

/// Splits blocks into consecutive ranges of at most CHUNK_SIZE blocks and MAX_CHUNK_CHARS chars;
/// an over-long block gets a range of its own.
pub fn chunk_ranges(blocks: &[String]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let (mut start, mut chars) = (0, 0);
    for (i, b) in blocks.iter().enumerate() {
        let len = b.chars().count();
        if i > start && (i - start == CHUNK_SIZE || chars + len > MAX_CHUNK_CHARS) {
            ranges.push(start..i);
            (start, chars) = (i, 0);
        }
        chars += len;
    }
    if start < blocks.len() {
        ranges.push(start..blocks.len());
    }
    ranges
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sized(lens: &[usize]) -> Vec<String> {
        lens.iter().map(|n| "x".repeat(*n)).collect()
    }

    #[test]
    fn chunk_ranges_empty() {
        assert!(chunk_ranges(&[]).is_empty());
    }

    #[test]
    fn chunk_ranges_split_by_count() {
        let blocks = sized(&[1; 85]);
        assert_eq!(chunk_ranges(&blocks), vec![0..40, 40..80, 80..85]);
    }

    #[test]
    fn chunk_ranges_split_by_chars() {
        let blocks = sized(&[300, 300, 1, 599, 600]);
        assert_eq!(chunk_ranges(&blocks), vec![0..2, 2..4, 4..5]);
    }

    #[test]
    fn chunk_ranges_isolate_over_long_blocks() {
        let blocks = sized(&[10, 700, 10]);
        assert_eq!(chunk_ranges(&blocks), vec![0..1, 1..2, 2..3]);
    }

    #[test]
    fn chunk_ranges_count_chars_not_bytes() {
        let blocks = vec!["é".repeat(400), "é".repeat(200)];
        assert_eq!(chunk_ranges(&blocks), vec![0..2]);
    }
}
