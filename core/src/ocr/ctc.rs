use ndarray::ArrayView2;

use super::charset::Charset;

/// Greedy CTC decode over a [timesteps, classes] probability matrix. Returns text and mean char confidence.
pub fn ctc_decode(probs: ArrayView2<f32>, charset: &Charset) -> (String, f32) {
    let mut text = String::new();
    let mut confs = Vec::new();
    let mut prev = 0usize;
    for row in probs.rows() {
        let (idx, p) =
            row.iter()
                .copied()
                .enumerate()
                .fold((0, f32::NEG_INFINITY), |best, (i, v)| {
                    if v > best.1 {
                        (i, v)
                    } else {
                        best
                    }
                });
        if idx != 0 && idx != prev {
            text.push_str(charset.get(idx));
            confs.push(p);
        }
        prev = idx;
    }
    let conf = if confs.is_empty() {
        0.0
    } else {
        confs.iter().sum::<f32>() / confs.len() as f32
    };
    (text, conf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn collapses_repeats_and_drops_blanks() {
        let cs = Charset::from_dict_str("a\nb\n");
        // classes: 0=blank 1=a 2=b 3=space
        let probs = array![
            [0.1, 0.8, 0.05, 0.05],
            [0.1, 0.9, 0.0, 0.0],
            [0.9, 0.1, 0.0, 0.0],
            [0.1, 0.7, 0.1, 0.1],
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.6, 0.4],
        ];
        let (text, conf) = ctc_decode(probs.view(), &cs);
        assert_eq!(text, "aa b");
        assert!((conf - (0.8 + 0.7 + 1.0 + 0.6) / 4.0).abs() < 1e-6);
    }

    #[test]
    fn all_blank_is_empty_with_zero_confidence() {
        let cs = Charset::from_dict_str("a\n");
        let probs = array![[1.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert_eq!(ctc_decode(probs.view(), &cs), (String::new(), 0.0));
    }
}
