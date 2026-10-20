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
const MIN_ROW_OVERLAP: f32 = 0.5;
const MAX_ROW_GAP: f32 = 0.5;
const MAX_STACK_GAP: f32 = 0.5;
const MAX_CENTERED_STACK_GAP: f32 = 0.75;
const MAX_STACK_OVERLAP: f32 = 0.5;
const MIN_FULL_ROW_WIDTH: f32 = 8.0;
const MAX_ALIGN_OFFSET: f32 = 1.0;
const MAX_BLOCK_ROWS: usize = 4;
const MAX_BLOCK_CHARS: usize = 200;
const BAND_TOLERANCE: f32 = 0.5;

fn overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    a1.min(b1) - a0.max(b0)
}

fn heights_compatible(a: f32, b: f32) -> bool {
    let lo = a.min(b);
    lo > 0.0 && a.max(b) / lo <= MAX_HEIGHT_RATIO
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

struct Item {
    line: Line,
    bbox: BBox,
}

struct Row {
    items: Vec<Item>,
    bbox: BBox,
}

impl Row {
    fn new(item: Item) -> Self {
        Self {
            bbox: item.bbox,
            items: vec![item],
        }
    }

    fn push(&mut self, item: Item) {
        self.bbox = self.bbox.union(item.bbox);
        self.items.push(item);
    }

    fn heights(&self) -> impl Iterator<Item = f32> + '_ {
        self.items.iter().map(|i| i.bbox.height())
    }

    fn chars(&self) -> usize {
        self.items.iter().map(|i| i.line.text.chars().count()).sum()
    }

    /// Horizontal gap to the nearest line if `b` belongs on this row.
    fn fit(&self, b: &BBox) -> Option<f32> {
        let (near, gap) = self
            .items
            .iter()
            .map(|i| (&i.bbox, (b.x0 - i.bbox.x1).max(i.bbox.x0 - b.x1)))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        let h = near.height().min(b.height());
        (heights_compatible(near.height(), b.height())
            && overlap(near.y0, near.y1, b.y0, b.y1) >= MIN_ROW_OVERLAP * h
            && gap <= MAX_ROW_GAP * h)
            .then_some(gap)
    }
}

struct Group {
    rows: Vec<Row>,
    chars: usize,
}

impl Group {
    fn line_height(&self) -> f32 {
        median(self.rows.iter().flat_map(Row::heights).collect())
    }

    /// Vertical gap to the last row if `row` can be stacked under this group.
    fn fit(&self, row: &Row) -> Option<f32> {
        if self.rows.len() >= MAX_BLOCK_ROWS || self.chars >= MAX_BLOCK_CHARS {
            return None;
        }
        let last = &self.rows.last()?.bbox;
        let hb = self.line_height();
        let hr = median(row.heights().collect());
        if !heights_compatible(hb, hr) {
            return None;
        }
        let h = hb.min(hr);
        let b = &row.bbox;
        let gap = b.y0 - last.y1;
        if gap < -MAX_STACK_OVERLAP * h {
            return None;
        }
        let tol = MAX_ALIGN_OFFSET * h;
        let centered = ((b.x0 + b.x1) - (last.x0 + last.x1)).abs() / 2.0 <= tol;
        let edge_aligned = (b.x0 - last.x0).abs() <= tol || (b.x1 - last.x1).abs() <= tol;
        // Edge alignment alone also matches menu and price columns; only treat it as continuation
        // when the previous row looks like a wrapped paragraph line.
        let fits = if centered {
            gap <= MAX_CENTERED_STACK_GAP * h
        } else {
            edge_aligned && last.width() >= MIN_FULL_ROW_WIDTH * h && gap <= MAX_STACK_GAP * h
        };
        fits.then_some(gap)
    }
}

fn build_rows(lines: Vec<Line>, rtl: bool) -> Vec<Row> {
    let mut items: Vec<Item> = lines
        .into_iter()
        .map(|line| Item {
            bbox: line.quad.bbox(),
            line,
        })
        .collect();
    items.sort_by(|a, b| (a.bbox.y0 + a.bbox.y1).total_cmp(&(b.bbox.y0 + b.bbox.y1)));
    let mut rows: Vec<Row> = Vec::new();
    for item in items {
        let best = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.fit(&item.bbox).map(|g| (i, g)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, _)) => rows[i].push(item),
            None => rows.push(Row::new(item)),
        }
    }
    for row in &mut rows {
        row.items.sort_by(|a, b| {
            let ord = a.bbox.x0.total_cmp(&b.bbox.x0);
            if rtl {
                ord.reverse()
            } else {
                ord
            }
        });
    }
    rows
}

pub fn group_lines(lines: Vec<Line>, lang: &Language) -> Vec<Block> {
    let mut rows = build_rows(lines, lang.rtl);
    rows.sort_by(|a, b| a.bbox.y0.total_cmp(&b.bbox.y0));
    let mut groups: Vec<Group> = Vec::new();
    for row in rows {
        let best = groups
            .iter()
            .enumerate()
            .filter_map(|(i, g)| g.fit(&row).map(|gap| (i, gap)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, _)) => {
                groups[i].chars += row.chars();
                groups[i].rows.push(row);
            }
            None => groups.push(Group {
                chars: row.chars(),
                rows: vec![row],
            }),
        }
    }
    order_blocks(groups, lang)
}

fn order_blocks(groups: Vec<Group>, lang: &Language) -> Vec<Block> {
    let mut keyed: Vec<(BBox, f32, Block)> = groups
        .into_iter()
        .map(|g| {
            let h = g.line_height();
            let block = build_block(g, lang);
            (block.quad.bbox(), h, block)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.y0.total_cmp(&b.0.y0));
    let x_key = |b: &BBox| if lang.rtl { -b.x1 } else { b.x0 };
    let mut out = Vec::with_capacity(keyed.len());
    let mut rest = keyed.into_iter().peekable();
    while let Some(first) = rest.next() {
        let limit = first.0.y0 + BAND_TOLERANCE * first.1;
        let mut band = vec![first];
        while let Some(next) = rest.next_if(|n| n.0.y0 < limit) {
            band.push(next);
        }
        band.sort_by(|a, b| x_key(&a.0).total_cmp(&x_key(&b.0)));
        out.extend(band.into_iter().map(|(_, _, b)| b));
    }
    out
}

fn build_block(group: Group, lang: &Language) -> Block {
    let sep = if lang.spaced { " " } else { "" };
    let lines: Vec<Line> = group
        .rows
        .into_iter()
        .flat_map(|r| r.items.into_iter().map(|i| i.line))
        .collect();
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
            pair_model: None,
            rtl,
            spaced,
            prompt_name: None,
            prompt_name_zh: None,
            zh_instruction: false,
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
            line(108.0, 10.0, 200.0, 30.0, "24h"),
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

    fn assert_no_mix(blocks: &[Block], a: &str, b: &str) {
        for blk in blocks {
            let has = |p: &str| blk.lines.iter().any(|l| l.text.starts_with(p));
            assert!(!(has(a) && has(b)), "mixed block: {:?}", blk.text);
        }
    }

    #[test]
    fn menu_items_and_prices_stay_apart_and_blocks_are_bounded() {
        let mut lines = Vec::new();
        for i in 0..6 {
            let y = 10.0 + i as f32 * 30.0;
            lines.push(line(
                10.0,
                y,
                100.0 + i as f32 * 10.0,
                y + 20.0,
                &format!("item{i}"),
            ));
            lines.push(line(400.0, y, 460.0, y + 20.0, &format!("${i}.50")));
        }
        let blocks = group_lines(lines, &lang(false, true));
        assert_no_mix(&blocks, "item", "$");
        assert!(
            blocks.iter().all(|b| b.lines.len() <= 4),
            "{:?}",
            texts(&blocks)
        );
    }

    #[test]
    fn columns_separated_by_narrow_gutter_do_not_mix() {
        let mut lines = Vec::new();
        for i in 0..5 {
            let y = 10.0 + i as f32 * 28.0;
            lines.push(line(10.0, y, 200.0, y + 20.0, &format!("L{i}")));
            lines.push(line(215.0, y, 400.0, y + 20.0, &format!("R{i}")));
        }
        assert_no_mix(&group_lines(lines, &lang(false, true)), "L", "R");
    }

    #[test]
    fn long_paragraph_is_split_into_ordered_small_blocks() {
        let lines: Vec<Line> = (0..30)
            .map(|i| {
                let y = 10.0 + i as f32 * 28.0;
                line(
                    10.0,
                    y,
                    300.0 - (i % 3) as f32 * 20.0,
                    y + 20.0,
                    &format!("l{i}"),
                )
            })
            .collect();
        let blocks = group_lines(lines, &lang(false, true));
        assert!(blocks.iter().all(|b| b.lines.len() <= 4));
        let order: Vec<String> = blocks
            .iter()
            .flat_map(|b| b.lines.iter().map(|l| l.text.clone()))
            .collect();
        let expected: Vec<String> = (0..30).map(|i| format!("l{i}")).collect();
        assert_eq!(order, expected);
    }

    #[test]
    fn blocks_on_same_band_are_ordered_left_to_right() {
        let lines = vec![
            line(300.0, 10.0, 400.0, 30.0, "right"),
            line(10.0, 12.0, 100.0, 32.0, "left"),
        ];
        assert_eq!(
            texts(&group_lines(lines, &lang(false, true))),
            vec!["left", "right"]
        );
    }

    #[test]
    fn tightly_spaced_menu_keeps_one_block_per_item_and_price() {
        let names = [
            ("Ramen", 60.0),
            ("Gyoza", 120.0),
            ("Edamame", 70.0),
            ("Beer", 140.0),
        ];
        let mut lines = Vec::new();
        for (i, (name, w)) in names.iter().enumerate() {
            let y = 10.0 + i as f32 * 26.0;
            lines.push(line(10.0, y, 10.0 + w, y + 20.0, name));
            let pw = if i % 2 == 0 { 40.0 } else { 90.0 };
            lines.push(line(460.0 - pw, y, 460.0, y + 20.0, &format!("${i}")));
        }
        let blocks = group_lines(lines, &lang(false, true));
        assert_eq!(blocks.len(), 8, "{:?}", texts(&blocks));
        assert!(blocks.iter().all(|b| b.lines.len() == 1));
    }

    #[test]
    fn centered_sign_with_loose_leading_merges() {
        let lines = vec![
            line(80.0, 10.0, 120.0, 30.0, "NO"),
            line(40.0, 42.0, 160.0, 62.0, "SMOKING"),
        ];
        assert_eq!(
            texts(&group_lines(lines, &lang(false, true))),
            vec!["NO SMOKING"]
        );
    }

    #[test]
    fn wrapped_paragraph_lines_merge_up_to_four_rows() {
        let widths = [400.0, 300.0, 380.0, 250.0, 400.0];
        let lines: Vec<Line> = widths
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let y = 10.0 + i as f32 * 28.0;
                line(10.0, y, 10.0 + w, y + 20.0, &format!("p{i}"))
            })
            .collect();
        assert_eq!(
            texts(&group_lines(lines, &lang(false, true))),
            vec!["p0 p1 p2 p3", "p4"]
        );
    }

    #[test]
    fn heavily_overlapping_rows_do_not_stack() {
        let row = |y0: f32| {
            Row::new(Item {
                line: line(10.0, y0, 200.0, y0 + 20.0, "x"),
                bbox: BBox {
                    x0: 10.0,
                    y0,
                    x1: 200.0,
                    y1: y0 + 20.0,
                },
            })
        };
        let group = Group {
            rows: vec![row(10.0)],
            chars: 1,
        };
        assert!(group.fit(&row(18.0)).is_none());
        assert!(group.fit(&row(25.0)).is_some());
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
