/// CTC class table matching PaddleOCR's CTCLabelDecode: index 0 is blank, then the dict, then a space
/// (PP-OCRv5 models are exported with use_space_char=True).
#[derive(Debug, Clone)]
pub struct Charset {
    classes: Vec<String>,
}

impl Charset {
    pub fn from_dict_str(s: &str) -> Self {
        let body = s.strip_suffix('\n').unwrap_or(s);
        let mut classes = vec![String::new()];
        classes.extend(body.split('\n').map(str::to_string));
        classes.push(" ".to_string());
        Self { classes }
    }

    pub fn len(&self) -> usize {
        self.classes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }

    pub fn get(&self, idx: usize) -> &str {
        &self.classes[idx]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_are_blank_then_dict_then_space() {
        let c = Charset::from_dict_str("a\nb\n　\n");
        assert_eq!(c.len(), 5);
        assert_eq!(c.get(0), "");
        assert_eq!(c.get(1), "a");
        assert_eq!(c.get(3), "　");
        assert_eq!(c.get(4), " ");
    }
}
