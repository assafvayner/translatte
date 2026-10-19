use serde::Serialize;

use crate::geometry::{BBox, Quad};
use crate::manifest::Language;
use crate::ocr::Line;

#[derive(Debug, Clone, Serialize)]
pub struct Block {
    pub quad: Quad,
    pub text: String,
    pub lines: Vec<Line>,
}

const MAX_HEIGHT_RATIO: f32 = 1.5;
const MAX_STACK_GAP: f32 = 0.6;
const MAX_ROW_GAP: f32 = 1.0;
const MIN_ROW_OVERLAP: f32 = 0.5;

fn overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    a1.min(b1) - a0.max(b0)
}

fn should_merge(a: &BBox, b: &BBox) -> bool {
    let (ha, hb) = (a.height(), b.height());
    let h = ha.min(hb);
    if h <= 0.0 || ha.max(hb) / h > MAX_HEIGHT_RATIO {
        return false;
    }
    let v_overlap = overlap(a.y0, a.y1, b.y0, b.y1);
    let h_overlap = overlap(a.x0, a.x1, b.x0, b.x1);
    let same_row = v_overlap >= MIN_ROW_OVERLAP * h && -h_overlap <= MAX_ROW_GAP * h;
    let stacked = h_overlap > 0.0 && -v_overlap <= MAX_STACK_GAP * h;
    same_row || stacked
}

fn find(parent: &mut [usize], i: usize) -> usize {
    if parent[i] != i {
        parent[i] = find(parent, parent[i]);
    }
    parent[i]
}

pub fn group_lines(lines: Vec<Line>, lang: &Language) -> Vec<Block> {
    let boxes: Vec<BBox> = lines.iter().map(|l| l.quad.bbox()).collect();
    let mut parent: Vec<usize> = (0..lines.len()).collect();
    for i in 0..lines.len() {
        for j in i + 1..lines.len() {
            if should_merge(&boxes[i], &boxes[j]) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                parent[ri] = rj;
            }
        }
    }
    let mut groups: Vec<Vec<Line>> = Vec::new();
    let mut root_to_group = std::collections::HashMap::new();
    for (i, l) in lines.into_iter().enumerate() {
        let r = find(&mut parent, i);
        let g = *root_to_group.entry(r).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[g].push(l);
    }
    let mut blocks: Vec<Block> = groups.into_iter().map(|g| build_block(g, lang)).collect();
    blocks.sort_by(|a, b| {
        let (ba, bb) = (a.quad.bbox(), b.quad.bbox());
        let x_key = |b: &BBox| if lang.rtl { -b.x1 } else { b.x0 };
        ba.y0
            .total_cmp(&bb.y0)
            .then(x_key(&ba).total_cmp(&x_key(&bb)))
    });
    blocks
}

fn build_block(mut lines: Vec<Line>, lang: &Language) -> Block {
    let center_y = |l: &Line| {
        let b = l.quad.bbox();
        (b.y0 + b.y1) / 2.0
    };
    lines.sort_by(|a, b| center_y(a).total_cmp(&center_y(b)));
    let mut rows: Vec<Vec<Line>> = Vec::new();
    for l in lines {
        let starts_new_row = match rows.last().and_then(|r| r.last()) {
            Some(prev) => (center_y(&l) - center_y(prev)).abs() > 0.5 * prev.quad.bbox().height(),
            None => true,
        };
        if starts_new_row {
            rows.push(vec![l]);
        } else {
            rows.last_mut().unwrap().push(l);
        }
    }
    for row in &mut rows {
        row.sort_by(|a, b| {
            let (xa, xb) = (a.quad.bbox().x0, b.quad.bbox().x0);
            if lang.rtl {
                xb.total_cmp(&xa)
            } else {
                xa.total_cmp(&xb)
            }
        });
    }
    let sep = if lang.spaced { " " } else { "" };
    let lines: Vec<Line> = rows.into_iter().flatten().collect();
    let text = lines
        .iter()
        .map(|l| l.text.trim())
        .collect::<Vec<_>>()
        .join(sep);
    let quad = if lines.len() == 1 {
        lines[0].quad
    } else {
        Quad::from_bbox(
            lines
                .iter()
                .map(|l| l.quad.bbox())
                .reduce(BBox::union)
                .unwrap(),
        )
    };
    Block { quad, text, lines }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{BBox, Pt};

    fn line(x0: f32, y0: f32, x1: f32, y1: f32, text: &str) -> Line {
        Line {
            quad: Quad::from_bbox(BBox { x0, y0, x1, y1 }),
            text: text.into(),
            confidence: 0.9,
        }
    }

    fn lang(rtl: bool, spaced: bool) -> Language {
        Language {
            code: "xx".into(),
            name: "X".into(),
            script_pack: None,
            rtl,
            spaced,
        }
    }

    fn texts(blocks: &[Block]) -> Vec<&str> {
        blocks.iter().map(|b| b.text.as_str()).collect()
    }

    #[test]
    fn stacked_close_lines_merge_into_one_block() {
        let lines = vec![
            line(10.0, 40.0, 200.0, 60.0, "world"),
            line(10.0, 10.0, 180.0, 30.0, "hello"),
        ];
        let blocks = group_lines(lines, &lang(false, true));
        assert_eq!(texts(&blocks), vec!["hello world"]);
        assert_eq!(
            blocks[0].quad.bbox(),
            BBox {
                x0: 10.0,
                y0: 10.0,
                x1: 200.0,
                y1: 60.0
            }
        );
    }

    #[test]
    fn distant_lines_stay_separate_and_are_ordered_top_to_bottom() {
        let lines = vec![
            line(10.0, 300.0, 200.0, 320.0, "bottom"),
            line(10.0, 10.0, 200.0, 30.0, "top"),
        ];
        assert_eq!(
            texts(&group_lines(lines, &lang(false, true))),
            vec!["top", "bottom"]
        );
    }

    #[test]
    fn same_row_words_merge_in_reading_order() {
        let lines = vec![
            line(120.0, 10.0, 200.0, 30.0, "24h"),
            line(10.0, 10.0, 100.0, 30.0, "OPEN"),
        ];
        assert_eq!(
            texts(&group_lines(lines.clone(), &lang(false, true))),
            vec!["OPEN 24h"]
        );
        assert_eq!(
            texts(&group_lines(lines, &lang(true, true))),
            vec!["24h OPEN"]
        );
    }

    #[test]
    fn unspaced_scripts_join_without_spaces() {
        let lines = vec![
            line(10.0, 10.0, 200.0, 30.0, "本日の"),
            line(10.0, 36.0, 200.0, 56.0, "おすすめ"),
        ];
        assert_eq!(
            texts(&group_lines(lines, &lang(false, false))),
            vec!["本日のおすすめ"]
        );
    }

    #[test]
    fn very_different_heights_do_not_merge() {
        let lines = vec![
            line(10.0, 10.0, 300.0, 70.0, "BIG"),
            line(10.0, 75.0, 300.0, 90.0, "small print"),
        ];
        assert_eq!(group_lines(lines, &lang(false, true)).len(), 2);
    }

    #[test]
    fn single_line_block_keeps_its_rotated_quad() {
        let rotated = Line {
            quad: Quad([
                Pt::new(0.0, 10.0),
                Pt::new(100.0, 0.0),
                Pt::new(102.0, 20.0),
                Pt::new(2.0, 30.0),
            ]),
            text: "tilted".into(),
            confidence: 0.9,
        };
        let blocks = group_lines(vec![rotated.clone()], &lang(false, true));
        assert_eq!(blocks[0].quad, rotated.quad);
    }
}
