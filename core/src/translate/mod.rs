pub mod cache;
pub mod pair;
pub mod reuse;

use crate::manifest::Language;
use crate::Result;

/// Translates the text blocks of one image. `None` marks a block that could not be translated.
pub trait Translator {
    fn translate(
        &mut self,
        blocks: &[String],
        src: &Language,
        tgt: &Language,
    ) -> Result<Vec<Option<String>>>;

    /// Prepares for translating from `src` to `tgt`, off the latency-critical path.
    fn warm(&mut self, _src: &Language, _tgt: &Language) -> Result<()> {
        Ok(())
    }
}
