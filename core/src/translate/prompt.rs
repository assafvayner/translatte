use crate::manifest::Language;

/// Renders TranslateGemma's chat template (see the model's chat_template.jinja). BOS is added by the tokenizer.
pub fn build_prompt(src: &Language, tgt: &Language, text: &str) -> String {
    let (s, sc, t, tc) = (&src.name, &src.code, &tgt.name, &tgt.code);
    format!(
        "<start_of_turn>user\nYou are a professional {s} ({sc}) to {t} ({tc}) translator. Your goal is to \
         accurately convey the meaning and nuances of the original {s} text while adhering to {t} grammar, \
         vocabulary, and cultural sensitivities.\nProduce only the {t} translation, without any additional \
         explanations or commentary. Please translate the following {s} text into {t}:\n\n\n{}<end_of_turn>\n\
         <start_of_turn>model\n",
        text.trim()
    )
}

pub fn format_numbered(blocks: &[String]) -> String {
    blocks
        .iter()
        .enumerate()
        .map(|(i, b)| format!("{}. {}", i + 1, b))
        .collect::<Vec<_>>()
        .join("\n")
}

fn split_marker(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let num: usize = t[..digits].parse().ok()?;
    let rest = &t[digits..];
    let marker = rest.chars().next()?;
    if !matches!(marker, '.' | ')' | '．' | '、') {
        return None;
    }
    Some((num, rest[marker.len_utf8()..].trim()))
}

pub fn parse_numbered(output: &str, n: usize) -> Option<Vec<String>> {
    let mut items: Vec<String> = Vec::with_capacity(n);
    for line in output.lines() {
        match split_marker(line) {
            Some((num, rest)) if num == items.len() + 1 => items.push(rest.to_string()),
            _ => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let last = items.last_mut()?;
                if !last.is_empty() {
                    last.push(' ');
                }
                last.push_str(line);
            }
        }
    }
    (items.len() == n && items.iter().all(|i| !i.is_empty())).then_some(items)
}

pub fn clean_output(s: &str) -> String {
    s.replace("<end_of_turn>", "").trim().to_string()
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
        }
    }

    #[test]
    fn prompt_matches_translategemma_template() {
        let p = build_prompt(
            &lang("ja", "Japanese"),
            &lang("en", "English"),
            "  こんにちは \n",
        );
        assert_eq!(
            p,
            "<start_of_turn>user\nYou are a professional Japanese (ja) to English (en) translator. Your goal is to \
             accurately convey the meaning and nuances of the original Japanese text while adhering to English \
             grammar, vocabulary, and cultural sensitivities.\nProduce only the English translation, without any \
             additional explanations or commentary. Please translate the following Japanese text into English:\n\n\n\
             こんにちは<end_of_turn>\n<start_of_turn>model\n"
        );
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
    fn parse_rejects_wrong_count_or_empty_items_or_preamble() {
        assert_eq!(parse_numbered("1. A", 2), None);
        assert_eq!(parse_numbered("1. A\n2. ", 2), None);
        assert_eq!(parse_numbered("Here you go:\n1. A\n2. B", 2), None);
    }

    #[test]
    fn clean_output_strips_turn_marker() {
        assert_eq!(clean_output("  Hello<end_of_turn>\n"), "Hello");
    }
}
