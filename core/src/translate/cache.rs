use std::collections::HashMap;

use crate::manifest::Language;
use crate::translate::Translator;
use crate::Result;

/// Translations kept for one source language: the text of a few dozen camera frames.
pub const CACHE_CAPACITY: usize = 512;

/// [`Translator`] that remembers translations by block text, so text already seen in a photo or
/// camera frame isn't translated again; only misses reach the wrapped translator. It holds one
/// source language and is cleared when the source changes. Blocks that failed to translate aren't
/// cached.
pub struct CachingTranslator<T> {
    inner: T,
    capacity: usize,
    source: Option<String>,
    /// Text -> (translation, last use).
    entries: HashMap<String, (String, u64)>,
    tick: u64,
}

impl<T: Translator> CachingTranslator<T> {
    pub fn new(inner: T) -> Self {
        Self::with_capacity(inner, CACHE_CAPACITY)
    }

    pub fn with_capacity(inner: T, capacity: usize) -> Self {
        Self {
            inner,
            capacity,
            source: None,
            entries: HashMap::new(),
            tick: 0,
        }
    }

    fn lookup(&mut self, text: &str) -> Option<String> {
        self.tick += 1;
        let tick = self.tick;
        self.entries.get_mut(text).map(|(translation, used)| {
            *used = tick;
            translation.clone()
        })
    }

    fn insert(&mut self, text: String, translation: String) {
        self.tick += 1;
        self.entries.insert(text, (translation, self.tick));
        if self.entries.len() > self.capacity {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(text, _)| text.clone());
            if let Some(oldest) = oldest {
                self.entries.remove(&oldest);
            }
        }
    }
}

impl<T: Translator> Translator for CachingTranslator<T> {
    fn translate(
        &mut self,
        blocks: &[String],
        src: &Language,
        tgt: &Language,
    ) -> Result<Vec<Option<String>>> {
        if self.source.as_deref() != Some(src.code.as_str()) {
            self.entries.clear();
            self.source = Some(src.code.clone());
        }
        let mut out: Vec<Option<String>> = blocks.iter().map(|b| self.lookup(b)).collect();
        let mut misses: Vec<String> = Vec::new();
        for (block, cached) in blocks.iter().zip(&out) {
            if cached.is_none() && !misses.contains(block) {
                misses.push(block.clone());
            }
        }
        if misses.is_empty() {
            return Ok(out);
        }
        let translated = self.inner.translate(&misses, src, tgt)?;
        for (text, translation) in misses.into_iter().zip(translated) {
            let Some(translation) = translation else {
                continue;
            };
            for (block, slot) in blocks.iter().zip(out.iter_mut()) {
                if slot.is_none() && *block == text {
                    *slot = Some(translation.clone());
                }
            }
            self.insert(text, translation);
        }
        Ok(out)
    }

    fn warm(&mut self, src: &Language, tgt: &Language) -> Result<()> {
        self.inner.warm(src, tgt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TranslatteError;

    #[derive(Default)]
    struct Recorder {
        batches: Vec<Vec<String>>,
    }

    impl Translator for Recorder {
        fn translate(
            &mut self,
            blocks: &[String],
            _src: &Language,
            _tgt: &Language,
        ) -> Result<Vec<Option<String>>> {
            self.batches.push(blocks.to_vec());
            if blocks.iter().any(|b| b == "boom") {
                return Err(TranslatteError::Translate("boom".into()));
            }
            Ok(blocks
                .iter()
                .map(|b| (b != "bad").then(|| b.to_uppercase()))
                .collect())
        }
    }

    fn lang(code: &str) -> Language {
        Language {
            code: code.into(),
            name: code.into(),
            script_pack: None,
            pair_model: None,
            rtl: false,
            spaced: true,
        }
    }

    fn blocks(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|t| t.to_string()).collect()
    }

    fn some(texts: &[&str]) -> Vec<Option<String>> {
        texts.iter().map(|t| Some(t.to_string())).collect()
    }

    #[test]
    fn only_misses_reach_the_translator() {
        let mut t = CachingTranslator::new(Recorder::default());
        let (fr, en) = (lang("fr"), lang("en"));
        assert_eq!(
            t.translate(&blocks(&["a", "b"]), &fr, &en).unwrap(),
            some(&["A", "B"])
        );
        assert_eq!(
            t.translate(&blocks(&["b", "c"]), &fr, &en).unwrap(),
            some(&["B", "C"])
        );
        assert_eq!(t.inner.batches, vec![blocks(&["a", "b"]), blocks(&["c"])]);
    }

    #[test]
    fn a_repeated_miss_is_translated_once() {
        let mut t = CachingTranslator::new(Recorder::default());
        let out = t
            .translate(&blocks(&["a", "a"]), &lang("fr"), &lang("en"))
            .unwrap();
        assert_eq!(out, some(&["A", "A"]));
        assert_eq!(t.inner.batches, vec![blocks(&["a"])]);
    }

    #[test]
    fn changing_the_source_clears_the_cache() {
        let mut t = CachingTranslator::new(Recorder::default());
        let en = lang("en");
        for src in ["fr", "de", "fr"] {
            t.translate(&blocks(&["a"]), &lang(src), &en).unwrap();
        }
        assert_eq!(t.inner.batches.len(), 3);
    }

    #[test]
    fn untranslated_blocks_are_not_cached() {
        let mut t = CachingTranslator::new(Recorder::default());
        let (fr, en) = (lang("fr"), lang("en"));
        for _ in 0..2 {
            assert_eq!(
                t.translate(&blocks(&["bad"]), &fr, &en).unwrap(),
                vec![None]
            );
        }
        assert_eq!(t.inner.batches.len(), 2);
    }

    #[test]
    fn evicts_the_least_recently_used() {
        let mut t = CachingTranslator::with_capacity(Recorder::default(), 2);
        let (fr, en) = (lang("fr"), lang("en"));
        for text in ["a", "b", "a", "c", "a", "b"] {
            t.translate(&blocks(&[text]), &fr, &en).unwrap();
        }
        assert_eq!(
            t.inner.batches,
            vec![
                blocks(&["a"]),
                blocks(&["b"]),
                blocks(&["c"]),
                blocks(&["b"])
            ]
        );
    }

    #[test]
    fn errors_propagate() {
        let mut t = CachingTranslator::new(Recorder::default());
        assert!(t
            .translate(&blocks(&["boom"]), &lang("fr"), &lang("en"))
            .is_err());
    }
}
