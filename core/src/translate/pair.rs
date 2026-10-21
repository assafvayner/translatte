use crate::manifest::{Language, TARGET_LANGUAGE};
use crate::translate::Translator;
use crate::{Result, TranslatteError};

/// One loaded pair model: translates a batch of texts from its source language into English.
pub trait PairBackend {
    fn translate(&mut self, texts: &[String]) -> Result<Vec<String>>;
}

/// Loads the pair model that translates from a source language.
pub trait PairLoader {
    type Model: PairBackend;

    fn load(&self, src: &Language) -> Result<Self::Model>;
}

/// [`Translator`] over pair models. Keeps the model of the last source language resident;
/// translating from a language with another model frees it and loads that one.
pub struct PairTranslator<L: PairLoader> {
    loader: L,
    loaded: Option<(String, L::Model)>,
}

impl<L: PairLoader> PairTranslator<L> {
    pub fn new(loader: L) -> Self {
        Self {
            loader,
            loaded: None,
        }
    }

    fn model(&mut self, src: &Language, tgt: &Language) -> Result<&mut L::Model> {
        if tgt.code != TARGET_LANGUAGE {
            return Err(TranslatteError::Translate(format!(
                "can only translate into English, not {}",
                tgt.name
            )));
        }
        let pair = src
            .pair_model
            .as_deref()
            .ok_or_else(|| TranslatteError::NoPairModel(src.code.clone()))?;
        if self.loaded.as_ref().map(|(id, _)| id.as_str()) != Some(pair) {
            self.loaded = None;
            self.loaded = Some((pair.to_string(), self.loader.load(src)?));
        }
        Ok(&mut self.loaded.as_mut().expect("loaded above").1)
    }
}

impl<L: PairLoader> Translator for PairTranslator<L> {
    fn translate(
        &mut self,
        blocks: &[String],
        src: &Language,
        tgt: &Language,
    ) -> Result<Vec<Option<String>>> {
        let texts: Vec<String> = blocks
            .iter()
            .filter(|b| !b.trim().is_empty())
            .cloned()
            .collect();
        let translated: Vec<Option<String>> = if texts.is_empty() {
            Vec::new()
        } else {
            let model = self.model(src, tgt)?;
            match model.translate(&texts) {
                Ok(out) if out.len() == texts.len() => out.into_iter().map(Some).collect(),
                Ok(out) => {
                    return Err(TranslatteError::Translate(format!(
                        "expected {} translations, got {}",
                        texts.len(),
                        out.len()
                    )))
                }
                // Retried one block at a time, so a block the model chokes on doesn't cost the rest.
                Err(e) => {
                    let each: Vec<Option<String>> = texts
                        .iter()
                        .map(|t| {
                            model
                                .translate(std::slice::from_ref(t))
                                .ok()
                                .and_then(|mut v| v.pop())
                        })
                        .collect();
                    if each.iter().all(Option::is_none) {
                        return Err(e);
                    }
                    each
                }
            }
        };
        let mut translated = translated.into_iter();
        Ok(blocks
            .iter()
            .map(|b| {
                if b.trim().is_empty() {
                    Some(b.clone())
                } else {
                    translated.next().flatten()
                }
            })
            .collect())
    }

    /// Loads the model for `src` and translates a dummy text: Marian builds its graph on the first
    /// call (~165ms on a Pixel 8), which this moves off the latency-critical path.
    fn warm(&mut self, src: &Language, tgt: &Language) -> Result<()> {
        self.model(src, tgt)?
            .translate(&[".".to_string()])
            .map(drop)
    }
}

#[cfg(feature = "pair")]
mod store {
    use super::{PairBackend, PairLoader};
    use crate::manifest::Language;
    use crate::model_store::ModelStore;
    use crate::{Result, TranslatteError};

    impl PairBackend for bergamot::PairModel {
        fn translate(&mut self, texts: &[String]) -> Result<Vec<String>> {
            bergamot::PairModel::translate(self, texts)
                .map_err(|e| TranslatteError::Translate(e.to_string()))
        }
    }

    /// Loads pair models from the files installed in a [`ModelStore`].
    pub struct StoreLoader(ModelStore);

    impl StoreLoader {
        pub fn new(store: ModelStore) -> Self {
            Self(store)
        }
    }

    impl PairLoader for StoreLoader {
        type Model = bergamot::PairModel;

        fn load(&self, src: &Language) -> Result<Self::Model> {
            let id = src
                .pair_model
                .as_deref()
                .ok_or_else(|| TranslatteError::NoPairModel(src.code.clone()))?;
            self.0.require_installed(id)?;
            let file = |name: &str| self.0.model_file(id, name);
            let model = file("model.bin")?;
            let lex = file("lex.bin")?;
            let (src_vocab, trg_vocab) = match file("vocab.spm") {
                Ok(v) => (v.clone(), v),
                Err(_) => (file("srcvocab.spm")?, file("trgvocab.spm")?),
            };
            bergamot::PairModel::load(&bergamot::PairFiles {
                model: &model,
                src_vocab: &src_vocab,
                trg_vocab: &trg_vocab,
                shortlist: &lex,
            })
            .map_err(|e| TranslatteError::Translate(format!("loading {id}: {e}")))
        }
    }
}

#[cfg(feature = "pair")]
pub use store::StoreLoader;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Default)]
    struct Log {
        loads: Vec<String>,
        batches: Vec<Vec<String>>,
    }

    struct FakeLoader(Rc<RefCell<Log>>);

    struct FakeModel {
        pair: String,
        log: Rc<RefCell<Log>>,
    }

    impl PairBackend for FakeModel {
        fn translate(&mut self, texts: &[String]) -> Result<Vec<String>> {
            self.log.borrow_mut().batches.push(texts.to_vec());
            if texts.iter().any(|t| t.contains("bad")) {
                return Err(TranslatteError::Translate("bad input".into()));
            }
            Ok(texts
                .iter()
                .map(|t| format!("{}:{}", self.pair, t.to_uppercase()))
                .collect())
        }
    }

    impl PairLoader for FakeLoader {
        type Model = FakeModel;

        fn load(&self, src: &Language) -> Result<FakeModel> {
            let pair = src.pair_model.clone().unwrap();
            self.0.borrow_mut().loads.push(pair.clone());
            Ok(FakeModel {
                pair,
                log: self.0.clone(),
            })
        }
    }

    fn translator() -> (Rc<RefCell<Log>>, PairTranslator<FakeLoader>) {
        let log = Rc::new(RefCell::new(Log::default()));
        (log.clone(), PairTranslator::new(FakeLoader(log)))
    }

    fn lang(code: &str, pair: Option<&str>) -> Language {
        Language {
            code: code.into(),
            name: code.into(),
            script_pack: Some("rec-latin".into()),
            pair_model: pair.map(Into::into),
            rtl: false,
            spaced: true,
        }
    }

    fn en() -> Language {
        Language {
            code: "en".into(),
            name: "English".into(),
            script_pack: None,
            pair_model: None,
            rtl: false,
            spaced: true,
        }
    }

    fn blocks(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn blank_blocks_pass_through_and_the_rest_go_in_one_batch() {
        let (log, mut t) = translator();
        let fr = lang("fr", Some("pair-fr-en"));
        let out = t
            .translate(&blocks(&["sortie", "  ", "entrée"]), &fr, &en())
            .unwrap();
        assert_eq!(
            out,
            vec![
                Some("pair-fr-en:SORTIE".into()),
                Some("  ".into()),
                Some("pair-fr-en:ENTRÉE".into())
            ]
        );
        assert_eq!(log.borrow().batches, vec![blocks(&["sortie", "entrée"])]);
    }

    #[test]
    fn only_blank_blocks_load_nothing() {
        let (log, mut t) = translator();
        let out = t
            .translate(&blocks(&[" "]), &lang("fr", Some("pair-fr-en")), &en())
            .unwrap();
        assert_eq!(out, vec![Some(" ".into())]);
        assert!(log.borrow().loads.is_empty());
    }

    #[test]
    fn a_failed_batch_is_retried_block_by_block() {
        let (log, mut t) = translator();
        let fr = lang("fr", Some("pair-fr-en"));
        let out = t
            .translate(&blocks(&["sortie", "bad", "entrée"]), &fr, &en())
            .unwrap();
        assert_eq!(
            out,
            vec![
                Some("pair-fr-en:SORTIE".into()),
                None,
                Some("pair-fr-en:ENTRÉE".into())
            ]
        );
        assert_eq!(log.borrow().batches.len(), 4);
    }

    #[test]
    fn every_block_failing_is_an_error() {
        let (_log, mut t) = translator();
        let err = t
            .translate(
                &blocks(&["bad", "bad too"]),
                &lang("fr", Some("pair-fr-en")),
                &en(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("bad input"), "{err}");
    }

    #[test]
    fn keeps_one_model_and_swaps_it_when_the_pair_changes() {
        let (log, mut t) = translator();
        let fr = lang("fr", Some("pair-fr-en"));
        let de = lang("de", Some("pair-de-en"));
        let zh_tw = lang("zh-TW", Some("pair-zh-Hant-en"));
        let yue = lang("yue", Some("pair-zh-Hant-en"));
        for (src, text) in [
            (&fr, "a"),
            (&fr, "b"),
            (&de, "c"),
            (&zh_tw, "d"),
            (&yue, "e"),
        ] {
            t.translate(&blocks(&[text]), src, &en()).unwrap();
        }
        assert_eq!(
            log.borrow().loads,
            vec!["pair-fr-en", "pair-de-en", "pair-zh-Hant-en"]
        );
    }

    #[test]
    fn only_translates_into_english() {
        let (log, mut t) = translator();
        let fr = lang("fr", Some("pair-fr-en"));
        let err = t
            .translate(&blocks(&["a"]), &fr, &lang("de", Some("pair-de-en")))
            .unwrap_err();
        assert!(err.to_string().contains("English"), "{err}");
        assert!(log.borrow().loads.is_empty());
    }

    #[test]
    fn a_language_without_a_pair_model_is_an_error() {
        let (_log, mut t) = translator();
        assert!(matches!(
            t.translate(&blocks(&["a"]), &lang("xx", None), &en()),
            Err(TranslatteError::NoPairModel(code)) if code == "xx"
        ));
    }

    #[test]
    fn warm_loads_the_model_and_runs_it_once() {
        let (log, mut t) = translator();
        t.warm(&lang("fr", Some("pair-fr-en")), &en()).unwrap();
        assert_eq!(log.borrow().loads, vec!["pair-fr-en"]);
        assert_eq!(log.borrow().batches, vec![blocks(&["."])]);
    }
}
