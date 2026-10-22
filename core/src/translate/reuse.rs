use std::collections::VecDeque;

use crate::geometry::{BBox, Quad};

/// Frames whose blocks a live frame's blocks are matched against.
const FRAMES: usize = 3;
/// A block matches one of an earlier frame whose box overlaps it at least this much (IoU). Low,
/// as a hand-held camera moves boxes a lot between frames.
const MIN_IOU: f32 = 0.3;
/// ... and whose text differs by at most this share of characters, ignoring whitespace, with
/// the same numbers (a price or a time that changed is a different text).
const MAX_EDIT_RATIO: f32 = 0.2;

struct Seen {
    bbox: BBox,
    text: String,
    chars: Vec<char>,
    numbers: Vec<String>,
    translation: String,
}

/// A block of an earlier frame that a block matched: the text that was translated, and its
/// translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reuse<'a> {
    pub source: &'a str,
    pub translation: &'a str,
}

/// Translations of the last few live frames' blocks, so a block whose text came out a character
/// or two different from the frame before (OCR jitter) reuses that translation instead of being
/// translated again. Live frames only; photos always translate.
#[derive(Default)]
pub struct FrameReuse {
    frames: VecDeque<Vec<Seen>>,
    source: Option<String>,
}

fn chars(text: &str) -> Vec<char> {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn numbers(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_numeric())
        .filter(|run| !run.is_empty())
        .map(str::to_string)
        .collect()
}

/// Levenshtein distance over characters, so CJK text is compared glyph by glyph.
fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

impl FrameReuse {
    /// Forgets earlier frames when the source language changes.
    pub fn set_source(&mut self, code: &str) {
        if self.source.as_deref() != Some(code) {
            self.frames.clear();
            self.source = Some(code.to_string());
        }
    }

    /// The closest-matching block of the last frames, if one matches.
    pub fn lookup(&self, quad: &Quad, text: &str) -> Option<Reuse<'_>> {
        let bbox = quad.bbox();
        let want = chars(text);
        if want.is_empty() {
            return None;
        }
        let want_numbers = numbers(text);
        let mut best: Option<(f32, &Seen)> = None;
        for seen in self.frames.iter().flatten() {
            if bbox.iou(&seen.bbox) < MIN_IOU || seen.numbers != want_numbers {
                continue;
            }
            let longest = want.len().max(seen.chars.len());
            let ratio = edit_distance(&want, &seen.chars) as f32 / longest as f32;
            if ratio <= MAX_EDIT_RATIO && best.is_none_or(|(r, _)| ratio < r) {
                best = Some((ratio, seen));
            }
        }
        best.map(|(_, seen)| Reuse {
            source: &seen.text,
            translation: &seen.translation,
        })
    }

    /// Records a frame's translated blocks as (place, translated text, translation), dropping the
    /// oldest frame past [`FRAMES`]. A block that reused a translation passes the text that was
    /// translated ([`Reuse::source`]), not its own, so matches don't drift frame by frame.
    pub fn push<'a>(&mut self, blocks: impl IntoIterator<Item = (&'a Quad, &'a str, &'a str)>) {
        let frame = blocks
            .into_iter()
            .map(|(quad, text, translation)| Seen {
                bbox: quad.bbox(),
                text: text.to_string(),
                chars: chars(text),
                numbers: numbers(text),
                translation: translation.to_string(),
            })
            .collect();
        self.frames.push_back(frame);
        while self.frames.len() > FRAMES {
            self.frames.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(x: f32, y: f32) -> Quad {
        Quad::from_bbox(BBox {
            x0: x,
            y0: y,
            x1: x + 100.0,
            y1: y + 20.0,
        })
    }

    fn translation(r: &FrameReuse, quad: &Quad, text: &str) -> Option<String> {
        r.lookup(quad, text).map(|hit| hit.translation.to_string())
    }

    fn reuse_with(frames: &[&[(Quad, &str, &str)]]) -> FrameReuse {
        let mut r = FrameReuse::default();
        r.set_source("ja");
        for f in frames {
            r.push(f.iter().map(|(q, s, t)| (q, *s, *t)));
        }
        r
    }

    #[test]
    fn edit_distance_counts_characters() {
        let d = |a: &str, b: &str| edit_distance(&chars(a), &chars(b));
        assert_eq!(d("安曇野天然水", "安曇野の天然水"), 1);
        assert_eq!(d("kitten", "sitting"), 3);
        assert_eq!(d("", "abc"), 3);
        assert_eq!(d("a b", "ab"), 0);
    }

    #[test]
    fn a_jittered_block_in_the_same_place_reuses_the_translation() {
        let r = reuse_with(&[&[(quad(0.0, 0.0), "安曇野の天然水", "Azumino natural water")]]);
        assert_eq!(
            translation(&r, &quad(5.0, 2.0), "安曇野天然水"),
            Some("Azumino natural water".into())
        );
        assert_eq!(
            translation(&r, &quad(0.0, 0.0), "Natural Mioeral Water"),
            None,
            "different text"
        );
    }

    #[test]
    fn a_similar_text_elsewhere_is_not_reused() {
        let r = reuse_with(&[&[(quad(0.0, 0.0), "Natural Mineral Water", "NMW")]]);
        assert_eq!(
            translation(&r, &quad(0.0, 300.0), "Natural Mioeral Water"),
            None
        );
        assert_eq!(
            translation(&r, &quad(10.0, 5.0), "Natural Mioeral Water"),
            Some("NMW".into())
        );
    }

    #[test]
    fn a_changed_number_is_a_different_text() {
        let r = reuse_with(&[&[
            (quad(0.0, 0.0), "Tea 3.00", "Thé 3.00"),
            (quad(0.0, 100.0), "€12.50", "12.50 €"),
            (quad(0.0, 200.0), "12:00", "midi"),
        ]]);
        assert_eq!(translation(&r, &quad(0.0, 0.0), "Tea 4.00"), None);
        assert_eq!(translation(&r, &quad(0.0, 0.0), "Tea 3.0"), None);
        assert_eq!(translation(&r, &quad(0.0, 100.0), "€13.50"), None);
        assert_eq!(translation(&r, &quad(0.0, 200.0), "13:00"), None);
        assert_eq!(
            translation(&r, &quad(0.0, 0.0), "Tca 3.00"),
            Some("Thé 3.00".into())
        );
        assert_eq!(
            translation(&r, &quad(0.0, 100.0), "€12.5O"),
            None,
            "a misread digit changes the numbers too"
        );
    }

    #[test]
    fn a_match_names_the_text_that_was_translated() {
        let r = reuse_with(&[&[(quad(0.0, 0.0), "安曇野の天然水", "Azumino natural water")]]);
        assert_eq!(
            r.lookup(&quad(0.0, 0.0), "安曇野天然水"),
            Some(Reuse {
                source: "安曇野の天然水",
                translation: "Azumino natural water"
            })
        );
    }

    #[test]
    fn short_texts_need_an_exact_match() {
        let r = reuse_with(&[&[(quad(0.0, 0.0), "出口", "Exit")]]);
        assert_eq!(translation(&r, &quad(0.0, 0.0), "入口"), None);
        assert_eq!(
            translation(&r, &quad(0.0, 0.0), "出口"),
            Some("Exit".into())
        );
        assert_eq!(translation(&r, &quad(0.0, 0.0), " "), None);
    }

    #[test]
    fn the_closest_text_wins() {
        let r = reuse_with(&[
            &[(quad(0.0, 0.0), "abcdefghij", "far")],
            &[(quad(0.0, 0.0), "abcdefghiX", "near")],
        ]);
        assert_eq!(
            translation(&r, &quad(0.0, 0.0), "abcdefghiX"),
            Some("near".into())
        );
        assert_eq!(
            translation(&r, &quad(0.0, 0.0), "abcdefghij"),
            Some("far".into())
        );
    }

    #[test]
    fn only_the_last_frames_are_kept() {
        let mut r = reuse_with(&[&[(quad(0.0, 0.0), "abcdefghij", "old")]]);
        for _ in 0..FRAMES {
            r.push(std::iter::empty());
        }
        assert_eq!(translation(&r, &quad(0.0, 0.0), "abcdefghij"), None);
    }

    #[test]
    fn a_new_source_language_forgets_earlier_frames() {
        let mut r = reuse_with(&[&[(quad(0.0, 0.0), "abcdefghij", "T")]]);
        r.set_source("ja");
        assert_eq!(
            translation(&r, &quad(0.0, 0.0), "abcdefghij"),
            Some("T".into())
        );
        r.set_source("zh");
        assert_eq!(translation(&r, &quad(0.0, 0.0), "abcdefghij"), None);
    }
}
