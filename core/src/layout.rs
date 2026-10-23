use serde::Serialize;

use crate::geometry::{angle_between, BBox, Quad};
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
/// Lines whose reading angles differ by up to this many degrees share a reading frame.
const MAX_ANGLE_SPREAD: f32 = 8.0;
/// Blocks sharing more than this fraction of the smaller one's area read the same text twice.
const MAX_SHARED_AREA: f32 = 0.5;
/// Blocks at different angles sharing more than this fraction of the smaller one's area can't
/// both be shown.
const MAX_CROSSING_AREA: f32 = 0.25;
/// Overlapping blocks of one reading frame are trimmed apart unless either would keep less than
/// this fraction of its height (or width).
const MIN_KEPT_EXTENT: f32 = 0.5;

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

fn build_rows(lines: Vec<Line>, angle: f32, rtl: bool) -> Vec<Row> {
    let mut items: Vec<Item> = lines
        .into_iter()
        .map(|line| Item {
            bbox: line.quad.to_reading_frame(angle).bbox(),
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

/// Lines reading at about the same angle.
#[derive(Default)]
struct Cluster {
    cos: f32,
    sin: f32,
    lines: Vec<Line>,
}

impl Cluster {
    /// Mean reading angle, weighted by line length since short lines' angles are noisy.
    fn angle(&self) -> f32 {
        self.sin.atan2(self.cos)
    }

    fn push(&mut self, line: Line) {
        let (a, w) = (line.quad.angle(), line.quad.width());
        self.cos += w * a.cos();
        self.sin += w * a.sin();
        self.lines.push(line);
    }
}

/// Clusters lines by reading angle, taking the longest lines first; the longest line's cluster
/// comes first.
fn cluster_by_angle(mut lines: Vec<Line>) -> Vec<Cluster> {
    lines.sort_by(|a, b| b.quad.width().total_cmp(&a.quad.width()));
    let spread = MAX_ANGLE_SPREAD.to_radians();
    let mut clusters: Vec<Cluster> = Vec::new();
    for line in lines {
        let a = line.quad.angle();
        let best = clusters
            .iter()
            .enumerate()
            .map(|(i, c)| (i, angle_between(c.angle(), a)))
            .filter(|&(_, d)| d <= spread)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, _)) => clusters[i].push(line),
            None => {
                let mut c = Cluster::default();
                c.push(line);
                clusters.push(c);
            }
        }
    }
    clusters
}

/// Groups lines in the reading frame of their angle, so tilted paragraphs and vertical columns
/// stack like horizontal rows; blocks keep their lines' rotation.
pub fn group_lines(lines: Vec<Line>, lang: &Language) -> Vec<Block> {
    drop_crossing(drop_overlapping(
        cluster_by_angle(lines)
            .into_iter()
            .flat_map(|c| {
                let angle = c.angle();
                group_frame(c.lines, angle, lang)
            })
            .collect(),
    ))
}

impl Block {
    /// Mean line confidence, weighted by characters.
    fn confidence(&self) -> f32 {
        let (sum, n) = self.lines.iter().fold((0.0, 0.0), |(sum, n), l| {
            let c = l.text.chars().count().max(1) as f32;
            (sum + c * l.confidence, n + c)
        });
        if n > 0.0 {
            sum / n
        } else {
            0.0
        }
    }
}

/// Of blocks covering mostly the same area (e.g. one text read at two angles), keeps the more
/// confident one, preserving reading order.
fn drop_overlapping(blocks: Vec<Block>) -> Vec<Block> {
    let mut by_confidence: Vec<usize> = (0..blocks.len()).collect();
    let confidence: Vec<f32> = blocks.iter().map(Block::confidence).collect();
    by_confidence.sort_by(|&a, &b| confidence[b].total_cmp(&confidence[a]));
    let mut kept: Vec<usize> = Vec::new();
    for i in by_confidence {
        let q = &blocks[i].quad;
        let clashes = kept.iter().any(|&k| {
            let other = &blocks[k].quad;
            q.intersection_area(other) > MAX_SHARED_AREA * q.area().min(other.area())
        });
        if !clashes {
            kept.push(i);
        }
    }
    kept.sort_unstable();
    blocks
        .into_iter()
        .enumerate()
        .filter(|(i, _)| kept.binary_search(i).is_ok())
        .map(|(_, b)| b)
        .collect()
}

/// Of blocks at different angles that still cross over more than `MAX_CROSSING_AREA` of the
/// smaller one (a fragment read at a stray angle, or a tilted sticker over a paragraph), keeps the
/// one with more text, preserving reading order. Blocks of one reading frame were already
/// separated.
fn drop_crossing(blocks: Vec<Block>) -> Vec<Block> {
    let chars: Vec<usize> = blocks.iter().map(|b| b.text.chars().count()).collect();
    let mut by_chars: Vec<usize> = (0..blocks.len()).collect();
    by_chars.sort_by(|&a, &b| chars[b].cmp(&chars[a]));
    let mut kept: Vec<usize> = Vec::new();
    for i in by_chars {
        let q = &blocks[i].quad;
        let crosses = kept.iter().any(|&k| {
            let other = &blocks[k].quad;
            q.intersection_area(other) > MAX_CROSSING_AREA * q.area().min(other.area())
        });
        if !crosses {
            kept.push(i);
        }
    }
    kept.sort_unstable();
    blocks
        .into_iter()
        .enumerate()
        .filter(|(i, _)| kept.binary_search(i).is_ok())
        .map(|(_, b)| b)
        .collect()
}

fn group_frame(lines: Vec<Line>, angle: f32, lang: &Language) -> Vec<Block> {
    let mut rows = build_rows(lines, angle, lang.rtl);
    rows.sort_by(|a, b| a.bbox.y0.total_cmp(&b.bbox.y0));
    let mut groups: Vec<Group> = Vec::new();
    let mut placed: Vec<BBox> = Vec::new();
    for row in rows {
        // A row can't continue a group across another row lying between them.
        let skips_a_row = |g: &Group| {
            let last = &g.rows.last().expect("groups have rows").bbox;
            placed.iter().any(|m| {
                m.y0 > last.y0
                    && overlap(m.x0, m.x1, last.x0, last.x1) > 0.0
                    && overlap(m.x0, m.x1, row.bbox.x0, row.bbox.x1) > 0.0
            })
        };
        let best = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| !skips_a_row(g))
            .filter_map(|(i, g)| g.fit(&row).map(|gap| (i, gap)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        placed.push(row.bbox);
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
    order_blocks(groups, angle, lang)
}

fn order_blocks(groups: Vec<Group>, angle: f32, lang: &Language) -> Vec<Block> {
    let mut keyed: Vec<(BBox, f32, Block)> = groups
        .into_iter()
        .map(|g| {
            let h = g.line_height();
            let (block, framed) = build_block(g, angle, lang);
            (framed, h, block)
        })
        .collect();
    separate(&mut keyed, angle);
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

/// Blocks of one reading frame whose boxes cross (detected lines are padded, so tightly set
/// neighbors' boxes overlap) meet halfway across the shared band, along the axis that cuts less of
/// them. When that would leave either with less than `MIN_KEPT_EXTENT` of its extent, it mostly
/// sits inside the other and the less confident of the two goes.
fn separate(keyed: &mut Vec<(BBox, f32, Block)>, angle: f32) {
    let mut gone = vec![false; keyed.len()];
    let mut trimmed = vec![false; keyed.len()];
    for i in 0..keyed.len() {
        for j in i + 1..keyed.len() {
            if gone[i] || gone[j] {
                continue;
            }
            let (a, b) = (keyed[i].0, keyed[j].0);
            let (ox, oy) = (
                overlap(a.x0, a.x1, b.x0, b.x1),
                overlap(a.y0, a.y1, b.y0, b.y1),
            );
            if ox <= 0.0 || oy <= 0.0 {
                continue;
            }
            let along_y = oy / a.height().min(b.height()) <= ox / a.width().min(b.width());
            let (lo, hi) = if along_y {
                (a.y0 + a.y1, b.y0 + b.y1)
            } else {
                (a.x0 + a.x1, b.x0 + b.x1)
            };
            let (first, second) = if lo <= hi { (i, j) } else { (j, i) };
            let (mut f, mut s) = (keyed[first].0, keyed[second].0);
            if along_y {
                let mid = (f.y1 + s.y0) / 2.0;
                (f.y1, s.y0) = (mid, mid);
            } else {
                let mid = (f.x1 + s.x0) / 2.0;
                (f.x1, s.x0) = (mid, mid);
            }
            let kept = |new: BBox, old: BBox| {
                if along_y {
                    new.height() / old.height()
                } else {
                    new.width() / old.width()
                }
            };
            if kept(f, keyed[first].0) < MIN_KEPT_EXTENT
                || kept(s, keyed[second].0) < MIN_KEPT_EXTENT
            {
                let weaker = if keyed[i].2.confidence() < keyed[j].2.confidence() {
                    i
                } else {
                    j
                };
                gone[weaker] = true;
                continue;
            }
            keyed[first].0 = f;
            keyed[second].0 = s;
            trimmed[first] = true;
            trimmed[second] = true;
        }
    }
    let mut k = 0;
    keyed.retain_mut(|(framed, _, block)| {
        let i = k;
        k += 1;
        if trimmed[i] {
            block.quad = Quad::from_bbox(*framed).from_reading_frame(angle);
        }
        !gone[i]
    });
}

/// The block and its bounding box in the reading frame. A multi-line block's quad is that box
/// turned back into the image, corners in reading order.
fn build_block(group: Group, angle: f32, lang: &Language) -> (Block, BBox) {
    let sep = if lang.spaced { " " } else { "" };
    let items: Vec<Item> = group.rows.into_iter().flat_map(|r| r.items).collect();
    let framed = items
        .iter()
        .map(|i| i.bbox)
        .reduce(BBox::union)
        .expect("a group has lines");
    let lines: Vec<Line> = items.into_iter().map(|i| i.line).collect();
    let text = lines
        .iter()
        .map(|l| l.text.trim())
        .collect::<Vec<_>>()
        .join(sep);
    let quad = if lines.len() == 1 {
        lines[0].quad
    } else {
        Quad::from_bbox(framed).from_reading_frame(angle)
    };
    (Block { quad, text, lines }, framed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{BBox, Direction, Pt};

    fn line(x0: f32, y0: f32, x1: f32, y1: f32, text: &str) -> Line {
        Line {
            quad: Quad::from_bbox(BBox { x0, y0, x1, y1 }),
            text: text.into(),
            confidence: 0.9,
            direction: Direction::Horizontal,
        }
    }

    fn vline(x0: f32, y0: f32, x1: f32, y1: f32, text: &str, direction: Direction) -> Line {
        Line {
            quad: Quad::from_bbox(BBox { x0, y0, x1, y1 }).turned(direction),
            text: text.into(),
            confidence: 0.9,
            direction,
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
            direction: Direction::Horizontal,
        };
        let blocks = group_lines(vec![rotated.clone()], &lang(false, true));
        assert_eq!(blocks[0].quad, rotated.quad);
    }

    #[test]
    fn cjk_columns_merge_right_to_left() {
        let d = Direction::TopToBottom;
        let lines = vec![
            vline(10.0, 10.0, 40.0, 300.0, "左", d),
            vline(60.0, 10.0, 90.0, 300.0, "中", d),
            vline(110.0, 10.0, 140.0, 300.0, "右", d),
        ];
        let blocks = group_lines(lines, &lang(false, false));
        assert_eq!(texts(&blocks), vec!["右中左"]);
        let q = blocks[0].quad;
        assert!((q.angle() - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert_eq!(
            q.bbox(),
            BBox {
                x0: 10.0,
                y0: 10.0,
                x1: 140.0,
                y1: 300.0
            }
        );
        assert_eq!(q.0[0], Pt::new(140.0, 10.0));
    }

    #[test]
    fn upward_lines_merge_left_to_right() {
        let d = Direction::BottomToTop;
        let lines = vec![
            vline(40.0, 10.0, 60.0, 300.0, "second", d),
            vline(10.0, 10.0, 30.0, 300.0, "first", d),
        ];
        let blocks = group_lines(lines, &lang(false, true));
        assert_eq!(texts(&blocks), vec!["first second"]);
        assert!((blocks[0].quad.angle() + std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }

    #[test]
    fn vertical_and_horizontal_lines_never_merge() {
        let lines = vec![
            line(50.0, 10.0, 200.0, 30.0, "row"),
            vline(10.0, 10.0, 30.0, 200.0, "col", Direction::TopToBottom),
        ];
        let blocks = group_lines(lines, &lang(false, true));
        let mut got = texts(&blocks);
        got.sort();
        assert_eq!(got, vec!["col", "row"]);
    }

    #[test]
    fn single_vertical_line_keeps_its_turned_quad() {
        let l = vline(10.0, 10.0, 30.0, 200.0, "col", Direction::BottomToTop);
        let blocks = group_lines(vec![l.clone()], &lang(false, true));
        assert_eq!(blocks[0].quad, l.quad);
        assert_eq!(blocks[0].lines[0].quad, l.quad);
    }

    /// Rotates `lines` by `degrees` about the origin, each turned by a further `jitter[i]`
    /// degrees about its own center, as a detector reports a tilted page.
    fn tilt(lines: Vec<Line>, degrees: f32, jitter: &[f32]) -> Vec<Line> {
        lines
            .into_iter()
            .enumerate()
            .map(|(i, mut l)| {
                let b = l.quad.bbox();
                let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
                let local = Quad(l.quad.0.map(|p| Pt::new(p.x - cx, p.y - cy)))
                    .from_reading_frame(jitter.get(i).copied().unwrap_or(0.0).to_radians());
                l.quad = Quad(local.0.map(|p| Pt::new(p.x + cx, p.y + cy)))
                    .from_reading_frame(degrees.to_radians());
                l
            })
            .collect()
    }

    fn paragraph() -> Vec<Line> {
        [400.0, 300.0, 380.0, 250.0, 400.0]
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let y = 10.0 + i as f32 * 28.0;
                line(10.0, y, 10.0 + w, y + 20.0, &format!("p{i}"))
            })
            .collect()
    }

    fn assert_angle(q: &Quad, degrees: f32) {
        let got = q.angle().to_degrees();
        assert!(
            crate::geometry::angle_between(got.to_radians(), degrees.to_radians()) < 0.02,
            "angle {got} want {degrees}"
        );
    }

    #[test]
    fn tilted_paragraphs_keep_line_order_and_their_rotation() {
        let jitter = [1.5, -1.0, 0.5, -1.5, 1.0];
        for degrees in [15.0, 30.0, 45.0, 60.0, -15.0, -30.0, -45.0, -60.0] {
            let mut lines = tilt(paragraph(), degrees, &jitter);
            lines.reverse();
            let blocks = group_lines(lines, &lang(false, true));
            assert_eq!(texts(&blocks), vec!["p0 p1 p2 p3", "p4"], "{degrees}");
            assert_angle(&blocks[0].quad, degrees);
            let level = blocks[0].quad.to_reading_frame(degrees.to_radians());
            let b = level.bbox();
            assert!((b.width() - 400.0).abs() < 12.0, "{degrees}: {b:?}");
            assert!((b.height() - 104.0).abs() < 12.0, "{degrees}: {b:?}");
            for l in &blocks[0].lines {
                let [a, _, c, _] = l.quad.0;
                let center = Pt::new((a.x + c.x) / 2.0, (a.y + c.y) / 2.0);
                assert!(blocks[0].quad.contains(center), "{degrees}: {l:?}");
            }
        }
    }

    #[test]
    fn tilted_block_corners_run_in_reading_order() {
        let blocks = group_lines(tilt(paragraph(), 30.0, &[]), &lang(false, true));
        let [tl, tr, br, bl] = blocks[0].quad.0;
        let first = &blocks[0].lines[0].quad.0;
        assert!(tl.dist(first[0]) < 1.0, "{tl:?} {first:?}");
        assert_angle(&Quad([tl, tr, br, bl]), 30.0);
        assert!(tl.dist(bl) < tl.dist(tr));
    }

    #[test]
    fn tilted_menu_items_and_prices_stay_apart() {
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
        for degrees in [15.0, 30.0, 45.0, 60.0] {
            let blocks = group_lines(tilt(lines.clone(), degrees, &[]), &lang(false, true));
            assert_eq!(
                texts(&blocks),
                ["Ramen", "$0", "Gyoza", "$1", "Edamame", "$2", "Beer", "$3"],
                "{degrees}"
            );
        }
    }

    #[test]
    fn paragraphs_at_different_angles_group_separately() {
        let mut lines = tilt(paragraph(), 15.0, &[]);
        let shifted: Vec<Line> = tilt(paragraph(), 45.0, &[])
            .into_iter()
            .map(|mut l| {
                l.text = l.text.replace('p', "q");
                l.quad = Quad(l.quad.0.map(|p| Pt::new(p.x + 200.0, p.y + 150.0)));
                l
            })
            .collect();
        lines.extend(shifted);
        let blocks = group_lines(lines, &lang(false, true));
        let mut got = texts(&blocks);
        got.sort();
        assert_eq!(got, vec!["p0 p1 p2 p3", "p4", "q0 q1 q2 q3", "q4"]);
        for b in &blocks {
            let want = if b.text.starts_with('p') { 15.0 } else { 45.0 };
            assert_angle(&b.quad, want);
        }
    }

    fn at(degrees: f32, width: f32) -> Line {
        tilt(vec![line(0.0, 0.0, width, 20.0, "x")], degrees, &[]).remove(0)
    }

    #[test]
    fn clusters_lines_by_reading_angle() {
        let lines = vec![
            at(7.0, 300.0),
            at(6.0, 250.0),
            at(8.5, 200.0),
            at(0.0, 40.0),
            at(-0.5, 60.0),
            at(30.0, 300.0),
            at(33.0, 100.0),
            at(-84.0, 200.0),
            at(179.0, 100.0),
            at(-175.0, 100.0),
        ];
        let clusters = cluster_by_angle(lines);
        let mut got: Vec<(i32, usize)> = clusters
            .iter()
            .map(|c| (c.angle().to_degrees().round() as i32, c.lines.len()))
            .collect();
        got.sort();
        assert_eq!(got, vec![(-178, 2), (-84, 1), (6, 5), (31, 2)]);
    }

    fn with_confidence(mut l: Line, confidence: f32) -> Line {
        l.confidence = confidence;
        l
    }

    #[test]
    fn heavily_overlapping_blocks_keep_the_more_confident_one() {
        let row = with_confidence(line(10.0, 100.0, 310.0, 130.0, "row"), 0.9);
        let crossing = with_confidence(
            tilt(
                vec![line(60.0, 100.0, 250.0, 130.0, "crossing")],
                0.0,
                &[12.0],
            )
            .remove(0),
            0.6,
        );
        let elsewhere = line(10.0, 300.0, 200.0, 330.0, "elsewhere");
        let blocks = group_lines(
            vec![crossing.clone(), row.clone(), elsewhere.clone()],
            &lang(false, true),
        );
        assert_eq!(texts(&blocks), vec!["row", "elsewhere"]);

        let blocks = group_lines(
            vec![with_confidence(crossing, 0.95), row, elsewhere],
            &lang(false, true),
        );
        let mut got = texts(&blocks);
        got.sort();
        assert_eq!(got, vec!["crossing", "elsewhere"]);
    }

    #[test]
    fn rotated_neighbors_whose_boxes_overlap_both_stay() {
        let lines = tilt(
            vec![
                line(0.0, 0.0, 400.0, 60.0, "TITLE"),
                line(0.0, 66.0, 400.0, 80.0, "small print"),
            ],
            45.0,
            &[],
        );
        let (a, b) = (lines[0].quad.bbox(), lines[1].quad.bbox());
        assert!(a.iou(&b) > 0.5, "the axis-aligned boxes overlap");
        let blocks = group_lines(lines, &lang(false, true));
        let mut got = texts(&blocks);
        got.sort();
        assert_eq!(got, vec!["TITLE", "small print"]);
    }

    #[test]
    fn a_slight_overlap_keeps_both_blocks() {
        let lines = vec![
            with_confidence(line(10.0, 10.0, 210.0, 40.0, "left"), 0.9),
            with_confidence(
                tilt(vec![line(190.0, 10.0, 390.0, 40.0, "right")], 0.0, &[30.0]).remove(0),
                0.6,
            ),
        ];
        assert_eq!(group_lines(lines, &lang(false, true)).len(), 2);
    }

    #[test]
    fn rows_do_not_stack_across_a_row_between_them() {
        let lines = vec![
            line(10.0, 10.0, 210.0, 40.0, "r1"),
            line(10.0, 35.5, 130.0, 65.5, "r2"),
            line(10.0, 61.0, 220.0, 91.0, "r3"),
        ];
        assert_eq!(
            texts(&group_lines(lines, &lang(false, true))),
            vec!["r1", "r2", "r3"]
        );
    }

    fn assert_apart(a: &Quad, b: &Quad) {
        assert!(a.intersection_area(b) < 1.0, "{a:?} {b:?}");
    }

    #[test]
    fn tightly_set_neighbors_are_trimmed_apart() {
        for degrees in [0.0, 30.0] {
            let lines = tilt(
                vec![
                    line(10.0, 10.0, 410.0, 70.0, "TITLE"),
                    line(10.0, 60.0, 300.0, 80.0, "small print"),
                ],
                degrees,
                &[],
            );
            let blocks = group_lines(lines, &lang(false, true));
            assert_eq!(texts(&blocks), vec!["TITLE", "small print"], "{degrees}");
            assert_apart(&blocks[0].quad, &blocks[1].quad);
            assert_angle(&blocks[1].quad, degrees);
            let small = blocks[1].quad.to_reading_frame(degrees.to_radians()).bbox();
            assert!((small.height() - 15.0).abs() < 0.5, "{degrees}: {small:?}");
            assert!((small.width() - 290.0).abs() < 0.5, "{degrees}: {small:?}");
        }
    }

    #[test]
    fn side_by_side_neighbors_are_trimmed_across() {
        let blocks = group_lines(
            vec![
                line(10.0, 10.0, 210.0, 70.0, "BIG"),
                line(200.0, 30.0, 300.0, 50.0, "side"),
            ],
            &lang(false, true),
        );
        assert_eq!(blocks.len(), 2);
        assert_apart(&blocks[0].quad, &blocks[1].quad);
        let big = blocks.iter().find(|b| b.text == "BIG").unwrap().quad.bbox();
        assert_eq!((big.x1, big.y1), (205.0, 70.0));
    }

    #[test]
    fn a_fragment_mostly_inside_a_neighbor_goes_by_confidence() {
        let lines = |fragment: f32| {
            vec![
                with_confidence(line(10.0, 10.0, 410.0, 70.0, "TITLE"), 0.8),
                with_confidence(line(100.0, 40.0, 160.0, 60.0, "x"), fragment),
            ]
        };
        assert_eq!(
            texts(&group_lines(lines(0.6), &lang(false, true))),
            vec!["TITLE"]
        );
        assert_eq!(
            texts(&group_lines(lines(0.9), &lang(false, true))),
            vec!["x"]
        );
    }

    #[test]
    fn a_block_at_a_stray_angle_crossing_a_longer_one_goes() {
        let paragraph =
            with_confidence(line(10.0, 100.0, 410.0, 140.0, "a long line of text"), 0.7);
        let stray = with_confidence(
            tilt(vec![line(150.0, 125.0, 230.0, 165.0, "te")], 0.0, &[25.0]).remove(0),
            0.95,
        );
        let far = tilt(vec![line(150.0, 300.0, 230.0, 340.0, "far")], 0.0, &[25.0]).remove(0);
        let blocks = group_lines(vec![paragraph, stray, far], &lang(false, true));
        assert_eq!(texts(&blocks), vec!["a long line of text", "far"]);
    }
}
