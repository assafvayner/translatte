use super::prompt::{build_prompt, clean_output, format_numbered, parse_numbered};
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

    fn translate_one(
        &mut self,
        text: &str,
        src: &Language,
        tgt: &Language,
    ) -> Result<Option<String>> {
        let out = self
            .generator
            .generate(&build_prompt(src, tgt, text), max_tokens_for(&[text]))?;
        let out = clean_output(&out);
        Ok((!out.is_empty()).then_some(out))
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
        match blocks.len() {
            0 => Ok(Vec::new()),
            1 => Ok(vec![self.translate_one(&blocks[0], src, tgt)?]),
            n => {
                let prompt = build_prompt(src, tgt, &format_numbered(blocks));
                let out = self.generator.generate(&prompt, max_tokens_for(blocks))?;
                if let Some(items) = parse_numbered(&clean_output(&out), n) {
                    return Ok(items.into_iter().map(Some).collect());
                }
                blocks
                    .iter()
                    .map(|b| self.translate_one(b, src, tgt))
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    struct FakeGen {
        responses: VecDeque<String>,
        prompts: Vec<String>,
    }

    impl FakeGen {
        fn new(responses: &[&str]) -> Self {
            Self {
                responses: responses.iter().map(|s| s.to_string()).collect(),
                prompts: Vec::new(),
            }
        }
    }

    impl TextGenerator for FakeGen {
        fn generate(&mut self, prompt: &str, _max_tokens: usize) -> Result<String> {
            self.prompts.push(prompt.to_string());
            Ok(self
                .responses
                .pop_front()
                .expect("unexpected generate call"))
        }
    }

    fn lang(code: &str) -> Language {
        Language {
            code: code.into(),
            name: code.into(),
            script_pack: None,
            rtl: false,
            spaced: true,
        }
    }

    fn blocks(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_input_makes_no_calls() {
        let mut t = BatchTranslator::new(FakeGen::new(&[]));
        assert!(t
            .translate(&[], &lang("fr"), &lang("en"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn single_block_is_sent_unnumbered() {
        let mut t = BatchTranslator::new(FakeGen::new(&["Hello<end_of_turn>"]));
        let out = t
            .translate(&blocks(&["Bonjour"]), &lang("fr"), &lang("en"))
            .unwrap();
        assert_eq!(out, vec![Some("Hello".to_string())]);
        assert!(t.generator().prompts[0].contains("\n\n\nBonjour<end_of_turn>"));
    }

    #[test]
    fn multiple_blocks_use_one_numbered_call() {
        let mut t = BatchTranslator::new(FakeGen::new(&["1. Hello\n2. Exit"]));
        let out = t
            .translate(&blocks(&["Bonjour", "Sortie"]), &lang("fr"), &lang("en"))
            .unwrap();
        assert_eq!(out, vec![Some("Hello".into()), Some("Exit".into())]);
        assert_eq!(t.generator().prompts.len(), 1);
        assert!(t.generator().prompts[0].contains("1. Bonjour\n2. Sortie"));
    }

    #[test]
    fn falls_back_to_per_block_when_numbering_breaks() {
        let mut t = BatchTranslator::new(FakeGen::new(&["Hello, Exit", "Hello", ""]));
        let out = t
            .translate(&blocks(&["Bonjour", "Sortie"]), &lang("fr"), &lang("en"))
            .unwrap();
        assert_eq!(out, vec![Some("Hello".into()), None]);
        assert_eq!(t.generator().prompts.len(), 3);
    }

    #[test]
    fn max_tokens_scales_with_input_and_is_capped() {
        assert_eq!(max_tokens_for(&blocks(&["abcd"])), 48);
        assert_eq!(max_tokens_for(&blocks(&[&"x".repeat(10_000)])), 2048);
    }
}
