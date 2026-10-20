use crate::manifest::Language;

/// The user turn of Hy-MT2's chat template split so the backend can tokenize the untrusted `text`
/// without special-token parsing; `prefix + text + suffix` is the full rendered prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub prefix: String,
    pub text: String,
    pub suffix: String,
}

fn prompt_name(l: &Language) -> &str {
    l.prompt_name.as_deref().unwrap_or(&l.name)
}

/// Hy-MT2's default translation instruction (model card), naming the source language too, which
/// makes the model translate short Latin-script signs it otherwise copies. Pairs with a Chinese
/// variety, or a language it only writes when asked in Chinese, get the Chinese instruction with
/// Chinese language names, as the model card prescribes; without a Chinese name for either side
/// the whole instruction stays English rather than mixing languages.
fn instruction(src: &Language, tgt: &Language) -> String {
    let zh_names = src
        .prompt_name_zh
        .as_deref()
        .zip(tgt.prompt_name_zh.as_deref());
    match zh_names {
        Some((s, t)) if src.zh_instruction || tgt.zh_instruction => {
            format!("将以下{s}文本翻译为{t}，注意只需要输出翻译后的结果，不要额外解释：\n\n")
        }
        _ => format!(
            "Translate the following {} text into {}. Note that you should only output the \
             translated result without any additional explanation:\n\n",
            prompt_name(src),
            prompt_name(tgt)
        ),
    }
}

/// Renders Hy-MT2's chat template (the GGUF's `tokenizer.chat_template`) for a single user turn.
/// The vocab doesn't add BOS itself, so the template's BOS is part of the prefix.
pub fn build_prompt(src: &Language, tgt: &Language, text: &str) -> Prompt {
    Prompt {
        prefix: format!(
            "<｜hy_begin▁of▁sentence｜><｜hy_User｜>{}",
            instruction(src, tgt)
        ),
        text: text.trim().to_string(),
        suffix: "<｜hy_Assistant｜>".to_string(),
    }
}

pub fn format_numbered(blocks: &[String]) -> String {
    blocks
        .iter()
        .enumerate()
        .map(|(i, b)| format!("{}. {}", i + 1, b))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Zero of each decimal digit block the model may number items with (e.g. Burmese or Bengali
/// targets get native digits).
const DIGIT_ZEROS: [char; 17] = [
    '0', '\u{0660}', '\u{06F0}', '\u{0966}', '\u{09E6}', '\u{0A66}', '\u{0AE6}', '\u{0B66}',
    '\u{0BE6}', '\u{0C66}', '\u{0D66}', '\u{0E50}', '\u{0F20}', '\u{1040}', '\u{17E0}', '\u{1810}',
    '０',
];

fn digit(c: char) -> Option<usize> {
    DIGIT_ZEROS.iter().find_map(|&z| {
        let d = (c as u32).checked_sub(z as u32)?;
        (d < 10).then_some(d as usize)
    })
}

fn starts_with_space_or_empty(s: &str) -> bool {
    s.chars().next().is_none_or(char::is_whitespace)
}

fn split_marker(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start_matches(|c: char| c == '*' || c == '#' || c.is_whitespace());
    let mut num = 0usize;
    let mut end = 0;
    for (i, c) in t.char_indices() {
        let Some(d) = digit(c) else { break };
        num = num.checked_mul(10)?.checked_add(d)?;
        end = i + c.len_utf8();
    }
    if end == 0 {
        return None;
    }
    let rest = t[end..].trim_start_matches('*');
    let mut chars = rest.chars();
    let body = match chars.next()? {
        '.' | '．' | ')' | '）' | '、' | ':' | '：' => chars.as_str().trim_start_matches('*'),
        c if c.is_whitespace() => {
            let after = rest.trim_start().strip_prefix('-')?;
            if !starts_with_space_or_empty(after) {
                return None;
            }
            after.trim_start()
        }
        _ => return None,
    };
    // A separator followed by a digit is a decimal, time or range ("2.50", "2:30", "2 - 5"),
    // not a list marker.
    if body.chars().next().and_then(digit).is_some() {
        return None;
    }
    Some((num, body.trim()))
}

/// Exactly `n` items numbered 1..=n (some possibly empty), or `None` if the numbering is broken.
pub fn parse_numbered_items(output: &str, n: usize) -> Option<Vec<String>> {
    let mut items: Vec<String> = Vec::with_capacity(n);
    for line in output.lines() {
        match split_marker(line) {
            Some((num, _)) if num == n + 1 && items.len() == n => break,
            Some((num, rest)) if num == items.len() + 1 => items.push(rest.to_string()),
            _ => {
                let line = line.trim();
                let Some(last) = items.last_mut() else {
                    continue;
                };
                if line.is_empty() {
                    continue;
                }
                if !last.is_empty() {
                    last.push(' ');
                }
                last.push_str(line);
            }
        }
    }
    (items.len() == n).then_some(items)
}

#[cfg(test)]
pub fn parse_numbered(output: &str, n: usize) -> Option<Vec<String>> {
    parse_numbered_items(output, n).filter(|items| items.iter().all(|i| !i.is_empty()))
}

/// Hy-MT2's end-of-turn token; generation normally stops on it as end-of-generation.
const END_OF_TURN: &str = "<｜hy_place▁holder▁no▁2｜>";

pub fn clean_output(s: &str) -> String {
    s.split(END_OF_TURN).next().unwrap_or("").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang(code: &str, name: &str) -> Language {
        Language {
            code: code.into(),
            name: name.into(),
            script_pack: None,
            pair_model: None,
            rtl: false,
            spaced: true,
            prompt_name: None,
            prompt_name_zh: None,
            zh_instruction: false,
        }
    }

    fn zh_lang(code: &str, name: &str, zh: &str) -> Language {
        Language {
            prompt_name_zh: Some(zh.into()),
            zh_instruction: true,
            ..lang(code, name)
        }
    }

    fn full(p: &Prompt) -> String {
        format!("{}{}{}", p.prefix, p.text, p.suffix)
    }

    #[test]
    fn prompt_matches_hy_mt2_template() {
        let p = build_prompt(
            &lang("ja", "Japanese"),
            &lang("en", "English"),
            "  こんにちは \n",
        );
        assert_eq!(p.text, "こんにちは");
        assert_eq!(
            full(&p),
            "<｜hy_begin▁of▁sentence｜><｜hy_User｜>Translate the following Japanese text into \
             English. Note that you should only output the translated result without any \
             additional explanation:\n\nこんにちは<｜hy_Assistant｜>"
        );
    }

    #[test]
    fn prompt_uses_prompt_name_when_set() {
        let tgt = Language {
            prompt_name: Some("Traditional Chinese".into()),
            ..lang("zh-TW", "Chinese (Traditional)")
        };
        let p = build_prompt(&lang("en", "English"), &tgt, "Exit");
        assert!(p.prefix.contains("into Traditional Chinese."));
        assert!(!p.prefix.contains("(Traditional)"));
    }

    #[test]
    fn chinese_pairs_get_the_chinese_instruction() {
        let zh = zh_lang("zh", "Chinese", "中文");
        let en = Language {
            prompt_name_zh: Some("英语".into()),
            ..lang("en", "English")
        };
        assert_eq!(
            full(&build_prompt(&zh, &en, "出口")),
            "<｜hy_begin▁of▁sentence｜><｜hy_User｜>将以下中文文本翻译为英语，\
             注意只需要输出翻译后的结果，不要额外解释：\n\n出口<｜hy_Assistant｜>"
        );
        let to_bo = build_prompt(&en, &zh_lang("bo", "Tibetan", "藏语"), "Exit");
        assert!(to_bo.prefix.contains("将以下英语文本翻译为藏语"));
    }

    #[test]
    fn missing_chinese_name_keeps_the_instruction_english() {
        let p = build_prompt(
            &lang("fr", "French"),
            &zh_lang("zh", "Chinese", "中文"),
            "Sortie",
        );
        assert!(p
            .prefix
            .ends_with("Translate the following French text into Chinese. Note that you should only output the translated result without any additional explanation:\n\n"));
    }

    #[test]
    fn numbered_round_trip() {
        let blocks = vec!["a".to_string(), "b c".to_string()];
        assert_eq!(format_numbered(&blocks), "1. a\n2. b c");
        assert_eq!(
            parse_numbered("1. A\n2. B C", 2),
            Some(vec!["A".into(), "B C".into()])
        );
    }

    #[test]
    fn parse_accepts_paren_and_fullwidth_markers_and_continuations() {
        let out = "1) Today's special\n2．Soy sauce ramen\n   850 yen\n3、Cash only";
        assert_eq!(
            parse_numbered(out, 3),
            Some(vec![
                "Today's special".into(),
                "Soy sauce ramen 850 yen".into(),
                "Cash only".into()
            ])
        );
    }

    #[test]
    fn parse_treats_out_of_sequence_numbers_as_text() {
        assert_eq!(
            parse_numbered("1. Founded in\n2024. a great year\n2. Next", 2),
            Some(vec!["Founded in 2024. a great year".into(), "Next".into()])
        );
    }

    #[test]
    fn parse_rejects_wrong_count_or_empty_items() {
        assert_eq!(parse_numbered("1. A", 2), None);
        assert_eq!(parse_numbered("1. A\n2. ", 2), None);
        assert_eq!(parse_numbered("Here you go:", 1), None);
    }

    fn ab() -> Option<Vec<String>> {
        Some(vec!["A".into(), "B".into()])
    }

    #[test]
    fn parse_items_keeps_empty_items_but_requires_count() {
        assert_eq!(
            parse_numbered_items("1. A\n2.\n3. C", 3),
            Some(vec!["A".into(), String::new(), "C".into()])
        );
        assert_eq!(parse_numbered_items("1. A", 2), None);
    }

    #[test]
    fn parse_ignores_preamble_before_first_item() {
        assert_eq!(parse_numbered("Here you go:\n\n1. A\n2. B", 2), ab());
    }

    #[test]
    fn parse_ignores_items_beyond_n() {
        assert_eq!(parse_numbered("1. A\n2. B\n3. C\nmore", 2), ab());
    }

    #[test]
    fn dot_marker_followed_by_digit_is_text() {
        assert_eq!(
            parse_numbered("1. Price list\n2.50 euro\n2. Water", 2),
            Some(vec!["Price list 2.50 euro".into(), "Water".into()])
        );
        assert_eq!(parse_numbered("1.\nA\n2．５０", 2), None);
        assert_eq!(parse_numbered("1.\nA\n2．B", 2), ab());
        assert_eq!(
            parse_numbered("1.Hello\n2.World", 2),
            Some(vec!["Hello".into(), "World".into()])
        );
        assert_eq!(parse_numbered("1.\nA\n2． B", 2), ab());
    }

    #[test]
    fn paren_comma_and_colon_markers_accept_any_following_char() {
        assert_eq!(parse_numbered("1)A\n2:B", 2), ab());
        assert_eq!(parse_numbered("1、A\n2: B", 2), ab());
    }

    #[test]
    fn markers_followed_by_digits_are_text_for_all_separators() {
        assert_eq!(
            parse_numbered("1. Open\n2:30 pm\n2. Closed", 2),
            Some(vec!["Open 2:30 pm".into(), "Closed".into()])
        );
        assert_eq!(
            parse_numbered("1) A\n2)5 B\n2）３ C\n2、4\n2：1\n2 - 5 people\n2) D", 2),
            Some(vec![
                "A 2)5 B 2）３ C 2、4 2：1 2 - 5 people".into(),
                "D".into()
            ])
        );
    }

    #[test]
    fn parse_accepts_dash_markers() {
        assert_eq!(parse_numbered("1 - A\n2  -  B", 2), ab());
        assert_eq!(
            parse_numbered("1 - A\n2 -5 degrees\n2 - B", 2),
            Some(vec!["A 2 -5 degrees".into(), "B".into()])
        );
    }

    #[test]
    fn parse_normalizes_fullwidth_digits() {
        assert_eq!(parse_numbered("１． A\n２）B", 2), ab());
    }

    #[test]
    fn parse_strips_markdown_around_numbers() {
        assert_eq!(parse_numbered("**1.** A\n## 2. B", 2), ab());
        assert_eq!(parse_numbered("**1**. A\n*2)* B", 2), ab());
    }

    #[test]
    fn parse_accepts_native_digits() {
        assert_eq!(parse_numbered("1. A  \n2. B  ", 2), ab());
        assert_eq!(parse_numbered("၁. A\n၂. B", 2), ab());
        assert_eq!(parse_numbered("১. A\n২. B", 2), ab());
        assert_eq!(parse_numbered("١. A\n٢. B", 2), ab());
        assert_eq!(digit('٩'), Some(9));
        assert_eq!(digit('a'), None);
        assert_eq!(digit('/'), None);
    }

    #[test]
    fn clean_output_strips_turn_marker() {
        assert_eq!(
            clean_output("  Hello  \n<｜hy_place▁holder▁no▁2｜>"),
            "Hello"
        );
    }

    #[test]
    fn clean_output_drops_everything_after_turn_marker() {
        assert_eq!(
            clean_output(" Hello <｜hy_place▁holder▁no▁2｜><｜hy_User｜>more"),
            "Hello"
        );
    }
}
