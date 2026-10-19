use super::prompt::{build_prompt, clean_output, format_numbered, parse_numbered_items, Prompt};
use super::{TextGenerator, Translator};
use crate::manifest::Language;
use crate::Result;

const MAX_TOKENS_CAP: usize = 2048;

pub struct BatchTranslator<G> {
    generator: G,
}

impl<G: TextGenerator> BatchTranslator<G> {
    pub fn new(generator: G) -> Self {
        Self { generator }
    }

    pub fn generator(&self) -> &G {
        &self.generator
    }

    /// Cleaned completion, or `None` if generation hit the token limit.
    fn complete(&mut self, prompt: &Prompt, max_tokens: usize) -> Result<Option<String>> {
        let g = self.generator.generate(prompt, max_tokens)?;
        Ok((!g.truncated).then(|| clean_output(&g.text)))
    }

    fn translate_one(
        &mut self,
        text: &str,
        src: &Language,
        tgt: &Language,
    ) -> Result<Option<String>> {
        Ok(self
            .complete(&build_prompt(src, tgt, text), max_tokens_for(&[text]))?
            .filter(|out| !out.is_empty()))
    }
}

/// Generous budget: target text can be several tokens per source char (e.g. CJK → English).
pub fn max_tokens_for<S: AsRef<str>>(blocks: &[S]) -> usize {
    let chars: usize = blocks.iter().map(|b| b.as_ref().chars().count()).sum();
    (chars * 4 + 32).min(MAX_TOKENS_CAP)
}

impl<G: TextGenerator> Translator for BatchTranslator<G> {
    fn translate(
        &mut self,
        blocks: &[String],
        src: &Language,
        tgt: &Language,
    ) -> Result<Vec<Option<String>>> {
        let n = blocks.len();
        let mut batch_failed = false;
        let items = match n {
            0 => return Ok(Vec::new()),
            1 => vec![String::new()],
            _ => {
                let prompt = build_prompt(src, tgt, &format_numbered(blocks));
                let parsed = match self.complete(&prompt, max_tokens_for(blocks)) {
                    Ok(out) => out.and_then(|out| parse_numbered_items(&out, n)),
                    Err(_) => {
                        batch_failed = true;
                        None
                    }
                };
                parsed.unwrap_or_else(|| vec![String::new(); n])
            }
        };
        let mut out = Vec::with_capacity(n);
        for (item, block) in items.into_iter().zip(blocks) {
            if !item.is_empty() {
                out.push(Some(item));
                continue;
            }
            match self.translate_one(block, src, tgt) {
                // Batch and first retry both failing points at the backend itself (e.g. context
                // allocation), not at the input, so surface it instead of returning all-None.
                Err(e) if batch_failed => return Err(e),
                Err(_) => out.push(None),
                Ok(t) => out.push(t),
            }
            batch_failed = false;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::translate::Generation;
    use crate::TranslatteError;

    enum Resp {
        Done(&'static str),
        Truncated(&'static str),
        Fail,
    }
    use Resp::*;

    struct FakeGen {
        responses: VecDeque<Resp>,
        prompts: Vec<String>,
    }

    impl FakeGen {
        fn new(responses: Vec<Resp>) -> Self {
            Self {
                responses: responses.into(),
                prompts: Vec::new(),
            }
        }
    }

    impl TextGenerator for FakeGen {
        fn generate(&mut self, prompt: &Prompt, _max_tokens: usize) -> Result<Generation> {
            self.prompts
                .push(format!("{}{}{}", prompt.prefix, prompt.text, prompt.suffix));
            let (text, truncated) = match self
                .responses
                .pop_front()
                .expect("unexpected generate call")
            {
                Done(t) => (t, false),
                Truncated(t) => (t, true),
                Fail => return Err(TranslatteError::Translate("boom".into())),
            };
            Ok(Generation {
                text: text.to_string(),
                truncated,
            })
        }
    }

    fn lang(code: &str) -> Language {
        Language {
            code: code.into(),
            name: code.into(),
            script_pack: None,
            rtl: false,
            spaced: true,
            prompt_name: None,
        }
    }

    fn blocks(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn run(responses: Vec<Resp>, input: &[&str]) -> (Vec<Option<String>>, Vec<String>) {
        let mut t = BatchTranslator::new(FakeGen::new(responses));
        let out = t
            .translate(&blocks(input), &lang("fr"), &lang("en"))
            .unwrap();
        let g = t.generator;
        assert!(g.responses.is_empty(), "unused responses");
        (out, g.prompts)
    }

    fn some(v: &[&str]) -> Vec<Option<String>> {
        v.iter().map(|s| Some(s.to_string())).collect()
    }

    #[test]
    fn empty_input_makes_no_calls() {
        let (out, prompts) = run(vec![], &[]);
        assert!(out.is_empty());
        assert!(prompts.is_empty());
    }

    #[test]
    fn single_block_is_sent_unnumbered() {
        let (out, prompts) = run(vec![Done("Hello<end_of_turn>")], &["Bonjour"]);
        assert_eq!(out, some(&["Hello"]));
        assert!(prompts[0].contains("\n\n\nBonjour<end_of_turn>"));
    }

    #[test]
    fn multiple_blocks_use_one_numbered_call() {
        let (out, prompts) = run(vec![Done("1. Hello\n2. Exit")], &["Bonjour", "Sortie"]);
        assert_eq!(out, some(&["Hello", "Exit"]));
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("1. Bonjour\n2. Sortie"));
    }

    #[test]
    fn falls_back_to_per_block_when_numbering_breaks() {
        let (out, prompts) = run(
            vec![Done("Hello, Exit"), Done("Hello"), Done("")],
            &["Bonjour", "Sortie"],
        );
        assert_eq!(out, vec![Some("Hello".into()), None]);
        assert_eq!(prompts.len(), 3);
    }

    #[test]
    fn truncated_batch_falls_back_to_per_block() {
        let (out, _) = run(
            vec![Truncated("1. Hello\n2. Ex"), Done("Hello"), Done("Exit")],
            &["Bonjour", "Sortie"],
        );
        assert_eq!(out, some(&["Hello", "Exit"]));
    }

    #[test]
    fn batch_error_falls_back_to_per_block() {
        let (out, _) = run(
            vec![Fail, Done("Hello"), Done("Exit")],
            &["Bonjour", "Sortie"],
        );
        assert_eq!(out, some(&["Hello", "Exit"]));
    }

    #[test]
    fn systemic_failure_is_propagated() {
        let mut t = BatchTranslator::new(FakeGen::new(vec![Fail, Fail, Fail]));
        let res = t.translate(
            &blocks(&["Bonjour", "Sortie", "Eau"]),
            &lang("fr"),
            &lang("en"),
        );
        assert!(res.is_err());
        assert_eq!(t.generator.prompts.len(), 2);
    }

    #[test]
    fn later_per_block_errors_after_batch_error_become_none() {
        let (out, _) = run(vec![Fail, Done("Hello"), Fail], &["Bonjour", "Sortie"]);
        assert_eq!(out, vec![Some("Hello".into()), None]);
    }

    #[test]
    fn only_empty_items_are_retried() {
        let (out, prompts) = run(
            vec![Done("1. Hello\n2.\n3. Water"), Done("Exit")],
            &["Bonjour", "Sortie", "Eau"],
        );
        assert_eq!(out, some(&["Hello", "Exit", "Water"]));
        assert_eq!(prompts.len(), 2);
        assert!(prompts[1].contains("\n\n\nSortie<end_of_turn>"));
    }

    #[test]
    fn per_block_errors_and_truncations_become_none() {
        let (out, _) = run(
            vec![Done("garbage"), Fail, Truncated("Exi"), Done("Water")],
            &["Bonjour", "Sortie", "Eau"],
        );
        assert_eq!(out, vec![None, None, Some("Water".into())]);
    }

    #[test]
    fn single_block_error_or_truncation_is_none() {
        assert_eq!(run(vec![Fail], &["Bonjour"]).0, vec![None]);
        assert_eq!(run(vec![Truncated("Hel")], &["Bonjour"]).0, vec![None]);
    }

    #[test]
    fn max_tokens_scales_with_input_and_is_capped() {
        assert_eq!(max_tokens_for(&blocks(&["abcd"])), 48);
        assert_eq!(max_tokens_for(&blocks(&[&"x".repeat(10_000)])), 2048);
    }
}
