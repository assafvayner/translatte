use crate::manifest::Language;

/// TranslateGemma's chat template split so the backend can tokenize the untrusted `text` without
/// special-token parsing; `prefix + text + suffix` is the full rendered prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub prefix: String,
    pub text: String,
    pub suffix: String,
}

fn prompt_name(l: &Language) -> &str {
    l.prompt_name.as_deref().unwrap_or(&l.name)
}

/// Renders TranslateGemma's chat template (see the model's chat_template.jinja). BOS is added by the tokenizer.
pub fn build_prompt(src: &Language, tgt: &Language, text: &str) -> Prompt {
    let (s, sc, t, tc) = (prompt_name(src), &src.code, prompt_name(tgt), &tgt.code);
    Prompt {
        prefix: format!(
            "<start_of_turn>user\nYou are a professional {s} ({sc}) to {t} ({tc}) translator. Your goal is to \
             accurately convey the meaning and nuances of the original {s} text while adhering to {t} grammar, \
             vocabulary, and cultural sensitivities.\nProduce only the {t} translation, without any additional \
             explanations or commentary. Please translate the following {s} text into {t}:\n\n\n"
        ),
        text: text.trim().to_string(),
        suffix: "<end_of_turn>\n<start_of_turn>model\n".to_string(),
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

fn digit(c: char) -> Option<usize> {
    match c {
        '0'..='9' => Some(c as usize - '0' as usize),
        '０'..='９' => Some(c as usize - '０' as usize),
        _ => None,
    }
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
        // A dot followed by a digit is a decimal ("2.50"), not a list marker.
        '.' | '．' => {
            let after = chars.as_str().trim_start_matches('*');
            after
                .chars()
                .next()
                .and_then(digit)
                .is_none()
                .then_some(after)?
        }
        ')' | '）' | '、' | ':' | '：' => chars.as_str().trim_start_matches('*'),
        c if c.is_whitespace() => {
            let after = rest.trim_start().strip_prefix('-')?;
            starts_with_space_or_empty(after).then_some(after)?
        }
        _ => return None,
    };
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

pub fn parse_numbered(output: &str, n: usize) -> Option<Vec<String>> {
    parse_numbered_items(output, n).filter(|items| items.iter().all(|i| !i.is_empty()))
}

pub fn clean_output(s: &str) -> String {
    s.split("<end_of_turn>")
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang(code: &str, name: &str) -> Language {
        Language {
            code: code.into(),
            name: name.into(),
            script_pack: None,
            rtl: false,
            spaced: true,
            prompt_name: None,
        }
    }

    #[test]
    fn prompt_matches_translategemma_template() {
        let p = build_prompt(
            &lang("ja", "Japanese"),
            &lang("en", "English"),
            "  こんにちは \n",
        );
        assert_eq!(p.text, "こんにちは");
        assert_eq!(
            format!("{}{}{}", p.prefix, p.text, p.suffix),
            "<start_of_turn>user\nYou are a professional Japanese (ja) to English (en) translator. Your goal is to \
             accurately convey the meaning and nuances of the original Japanese text while adhering to English \
             grammar, vocabulary, and cultural sensitivities.\nProduce only the English translation, without any \
             additional explanations or commentary. Please translate the following Japanese text into English:\n\n\n\
             こんにちは<end_of_turn>\n<start_of_turn>model\n"
        );
    }

    #[test]
    fn prompt_uses_translategemma_language_name_when_set() {
        let src = Language {
            prompt_name: Some("Chinese".into()),
            ..lang("zh-TW", "Chinese (Traditional)")
        };
        let p = build_prompt(&src, &lang("en", "English"), "出口");
        assert!(p
            .prefix
            .contains("professional Chinese (zh-TW) to English (en)"));
        assert!(!p.prefix.contains("Traditional"));
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
    fn clean_output_strips_turn_marker() {
        assert_eq!(clean_output("  Hello<end_of_turn>\n"), "Hello");
    }

    #[test]
    fn clean_output_drops_everything_after_turn_marker() {
        assert_eq!(
            clean_output(" Hello <end_of_turn>\n<start_of_turn>user\nmore"),
            "Hello"
        );
    }
}
