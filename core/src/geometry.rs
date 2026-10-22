use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn dist(self, o: Pt) -> f32 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl BBox {
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    pub fn union(self, o: BBox) -> BBox {
        BBox {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }

    /// Intersection over union; 0 when either box is empty.
    pub fn iou(&self, o: &BBox) -> f32 {
        let iw = (self.x1.min(o.x1) - self.x0.max(o.x0)).max(0.0);
        let ih = (self.y1.min(o.y1) - self.y0.max(o.y0)).max(0.0);
        let inter = iw * ih;
        let union = self.width() * self.height() + o.width() * o.height() - inter;
        if union > 0.0 {
            inter / union
        } else {
            0.0
        }
    }
}

/// Which way a text line reads in the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    Horizontal,
    /// Reads downward: CJK columns, or Latin rotated 90° clockwise.
    TopToBottom,
    /// Reads upward: Latin rotated 90° counter-clockwise.
    BottomToTop,
}

/// Crops at least this much taller than wide are read as vertical text, as in PaddleOCR.
pub const TALL_RATIO: f32 = 1.5;

/// Cosine and sine of `angle`, exact at multiples of 90° so axis-aligned quads stay exact.
fn rotation(angle: f32) -> (f32, f32) {
    let snap = |v: f32| {
        if v.abs() < 1e-6 {
            0.0
        } else if (v.abs() - 1.0).abs() < 1e-6 {
            v.signum()
        } else {
            v
        }
    };
    (snap(angle.cos()), snap(angle.sin()))
}

/// Absolute difference of two angles, wrapped to [0, π].
pub fn angle_between(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(std::f32::consts::TAU);
    d.min(std::f32::consts::TAU - d)
}

/// Corners in order: top-left, top-right, bottom-right, bottom-left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quad(pub [Pt; 4]);

impl Quad {
    /// Mirrors PaddleOCR's `get_mini_boxes` ordering so detector output matches upstream behavior.
    pub fn from_unordered(pts: [Pt; 4]) -> Self {
        let mut p = pts;
        p.sort_by(|a, b| a.x.total_cmp(&b.x));
        let (tl, bl) = if p[0].y <= p[1].y {
            (p[0], p[1])
        } else {
            (p[1], p[0])
        };
        let (tr, br) = if p[2].y <= p[3].y {
            (p[2], p[3])
        } else {
            (p[3], p[2])
        };
        Quad([tl, tr, br, bl])
    }

    pub fn from_bbox(b: BBox) -> Self {
        Quad([
            Pt::new(b.x0, b.y0),
            Pt::new(b.x1, b.y0),
            Pt::new(b.x1, b.y1),
            Pt::new(b.x0, b.y1),
        ])
    }

    pub fn width(&self) -> f32 {
        self.0[0].dist(self.0[1]).max(self.0[3].dist(self.0[2]))
    }

    pub fn height(&self) -> f32 {
        self.0[0].dist(self.0[3]).max(self.0[1].dist(self.0[2]))
    }

    pub fn angle(&self) -> f32 {
        let [tl, tr, _, _] = self.0;
        (tr.y - tl.y).atan2(tr.x - tl.x)
    }

    pub fn is_tall(&self) -> bool {
        self.height() >= TALL_RATIO * self.width()
    }

    /// Reorders the corners so TL→TR runs along `direction`'s reading direction.
    pub fn turned(&self, direction: Direction) -> Quad {
        let [tl, tr, br, bl] = self.0;
        match direction {
            Direction::Horizontal => *self,
            Direction::TopToBottom => Quad([tr, br, bl, tl]),
            Direction::BottomToTop => Quad([bl, tl, tr, br]),
        }
    }

    /// The quad relabeled as a tall crop when it may hold vertical text: already tall, or long
    /// along an axis closer to vertical than horizontal.
    pub fn vertical_candidate(&self) -> Option<Quad> {
        if self.is_tall() {
            return Some(*self);
        }
        let angle = self.angle();
        if angle.sin().abs() < std::f32::consts::FRAC_1_SQRT_2 {
            return None;
        }
        let direction = if angle < 0.0 {
            Direction::TopToBottom
        } else {
            Direction::BottomToTop
        };
        Some(self.turned(direction)).filter(Quad::is_tall)
    }

    /// Maps into a frame where a line reading at `angle` reads along +x and following lines are +y.
    pub fn to_reading_frame(&self, angle: f32) -> Quad {
        let (c, s) = rotation(angle);
        Quad(
            self.0
                .map(|p| Pt::new(p.x * c + p.y * s, p.y * c - p.x * s)),
        )
    }

    pub fn from_reading_frame(&self, angle: f32) -> Quad {
        let (c, s) = rotation(angle);
        Quad(
            self.0
                .map(|p| Pt::new(p.x * c - p.y * s, p.x * s + p.y * c)),
        )
    }

    pub fn bbox(&self) -> BBox {
        let xs = self.0.map(|p| p.x);
        let ys = self.0.map(|p| p.y);
        BBox {
            x0: xs.iter().copied().fold(f32::INFINITY, f32::min),
            y0: ys.iter().copied().fold(f32::INFINITY, f32::min),
            x1: xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            y1: ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        }
    }

    pub fn scale(&self, sx: f32, sy: f32) -> Quad {
        Quad(self.0.map(|p| Pt::new(p.x * sx, p.y * sy)))
    }

    pub fn clamp(&self, max_x: f32, max_y: f32) -> Quad {
        let (max_x, max_y) = (max_x.max(0.0), max_y.max(0.0));
        Quad(
            self.0
                .map(|p| Pt::new(p.x.clamp(0.0, max_x), p.y.clamp(0.0, max_y))),
        )
    }

    /// Assumes a convex quad with corners in order.
    pub fn contains(&self, p: Pt) -> bool {
        let mut sign = 0.0f32;
        for i in 0..4 {
            let a = self.0[i];
            let b = self.0[(i + 1) % 4];
            let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
            if cross != 0.0 {
                if sign != 0.0 && cross.signum() != sign {
                    return false;
                }
                sign = cross.signum();
            }
        }
        sign != 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Quad {
        Quad::from_bbox(BBox { x0, y0, x1, y1 })
    }

    #[test]
    fn iou_of_boxes() {
        let a = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 10.0,
            y1: 10.0,
        };
        let half = BBox { x0: 5.0, ..a };
        let apart = BBox {
            x0: 20.0,
            x1: 30.0,
            ..a
        };
        let empty = BBox { x1: 0.0, ..a };
        assert_eq!(a.iou(&a), 1.0);
        assert_eq!(a.iou(&half), 0.5);
        assert_eq!(a.iou(&apart), 0.0);
        assert_eq!(empty.iou(&empty), 0.0);
    }

    #[test]
    fn from_unordered_orders_corners_clockwise_from_top_left() {
        let q = Quad::from_unordered([
            Pt::new(10.0, 20.0),
            Pt::new(0.0, 0.0),
            Pt::new(0.0, 20.0),
            Pt::new(10.0, 0.0),
        ]);
        assert_eq!(q, rect(0.0, 0.0, 10.0, 20.0));
    }

    #[test]
    fn width_height_and_angle_of_axis_aligned_rect() {
        let q = rect(0.0, 0.0, 30.0, 10.0);
        assert_eq!(q.width(), 30.0);
        assert_eq!(q.height(), 10.0);
        assert_eq!(q.angle(), 0.0);
    }

    #[test]
    fn angle_of_rotated_quad() {
        let q = Quad([
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(5.0, 15.0),
            Pt::new(-5.0, 5.0),
        ]);
        assert!((q.angle() - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    }

    #[test]
    fn contains_inside_and_outside_points() {
        let q = rect(0.0, 0.0, 10.0, 10.0);
        assert!(q.contains(Pt::new(5.0, 5.0)));
        assert!(q.contains(Pt::new(0.0, 0.0)));
        assert!(!q.contains(Pt::new(11.0, 5.0)));
    }

    #[test]
    fn degenerate_quad_contains_nothing() {
        let p = Pt::new(3.0, 3.0);
        let q = Quad([p, p, p, p]);
        assert!(!q.contains(p));
        assert!(!q.contains(Pt::new(100.0, 0.0)));
    }

    #[test]
    fn rotated_quad_contains() {
        let q = Quad([
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(5.0, 15.0),
            Pt::new(-5.0, 5.0),
        ]);
        assert!(q.contains(Pt::new(5.0, 7.5)));
        assert!(!q.contains(Pt::new(10.0, 0.0)));
    }

    #[test]
    fn clamp_with_negative_max_does_not_panic() {
        let c = rect(1.0, 1.0, 5.0, 5.0).clamp(-1.0, -2.0);
        assert_eq!(
            c.bbox(),
            BBox {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0
            }
        );
    }

    #[test]
    fn bbox_scale_and_clamp() {
        let q = rect(10.0, 10.0, 20.0, 30.0).scale(2.0, 0.5);
        assert_eq!(
            q.bbox(),
            BBox {
                x0: 20.0,
                y0: 5.0,
                x1: 40.0,
                y1: 15.0
            }
        );
        let c = rect(-5.0, -5.0, 50.0, 50.0).clamp(40.0, 30.0);
        assert_eq!(
            c.bbox(),
            BBox {
                x0: 0.0,
                y0: 0.0,
                x1: 40.0,
                y1: 30.0
            }
        );
    }

    #[test]
    fn bbox_union() {
        let a = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 5.0,
            y1: 5.0,
        };
        let b = BBox {
            x0: 3.0,
            y0: -1.0,
            x1: 8.0,
            y1: 4.0,
        };
        assert_eq!(
            a.union(b),
            BBox {
                x0: 0.0,
                y0: -1.0,
                x1: 8.0,
                y1: 5.0
            }
        );
    }

    #[test]
    fn turning_a_tall_quad_puts_the_reading_edge_on_top() {
        let tall = rect(10.0, 10.0, 30.0, 110.0);
        let down = tall.turned(Direction::TopToBottom);
        assert_eq!(
            down.0,
            [
                Pt::new(30.0, 10.0),
                Pt::new(30.0, 110.0),
                Pt::new(10.0, 110.0),
                Pt::new(10.0, 10.0)
            ]
        );
        assert!((down.angle() - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!((down.width(), down.height()), (100.0, 20.0));

        let up = tall.turned(Direction::BottomToTop);
        assert_eq!(up.0[0], Pt::new(10.0, 110.0));
        assert!((up.angle() + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(up.bbox(), tall.bbox());

        assert_eq!(tall.turned(Direction::Horizontal), tall);
    }

    #[test]
    fn only_clearly_tall_quads_are_vertical_candidates() {
        assert!(rect(0.0, 0.0, 20.0, 30.0).is_tall());
        assert!(!rect(0.0, 0.0, 20.0, 29.0).is_tall());
        assert!(!rect(0.0, 0.0, 100.0, 20.0).is_tall());
    }

    #[test]
    fn reading_frame_round_trips_and_levels_turned_quads() {
        let tall = rect(10.0, 10.0, 30.0, 110.0);
        for d in [
            Direction::Horizontal,
            Direction::TopToBottom,
            Direction::BottomToTop,
        ] {
            let angle = tall.turned(d).angle();
            assert_eq!(tall.to_reading_frame(angle).from_reading_frame(angle), tall);
            assert!(tall.turned(d).to_reading_frame(angle).angle().abs() < 1e-6);
        }
    }

    fn rotated(q: Quad, degrees: f32) -> Quad {
        q.from_reading_frame(degrees.to_radians())
    }

    fn close(a: Quad, b: Quad) -> bool {
        a.0.iter().zip(b.0).all(|(p, q)| p.dist(q) < 1e-3)
    }

    #[test]
    fn reading_frame_levels_and_round_trips_any_angle() {
        let line = rect(100.0, 200.0, 400.0, 240.0);
        for degrees in [
            -60.0f32, -45.0, -30.0, -15.0, 7.0, 15.0, 30.0, 45.0, 60.0, 135.0,
        ] {
            let angle = degrees.to_radians();
            let tilted = rotated(line, degrees);
            assert!(angle_between(tilted.angle(), angle) < 1e-4, "{degrees}");
            let level = tilted.to_reading_frame(angle);
            assert!(close(level, line), "{degrees}: {level:?}");
            assert!(close(level.from_reading_frame(angle), tilted), "{degrees}");
        }
    }

    #[test]
    fn angles_compare_across_the_wrap() {
        let deg = |d: f32| d.to_radians();
        assert!((angle_between(deg(175.0), deg(-175.0)) - deg(10.0)).abs() < 1e-5);
        assert!((angle_between(deg(-8.0), deg(8.0)) - deg(16.0)).abs() < 1e-5);
        assert_eq!(angle_between(1.0, 1.0), 0.0);
    }

    #[test]
    fn steep_wide_quads_are_vertical_candidates() {
        let column = rect(0.0, 0.0, 200.0, 30.0);
        for degrees in [-84.0, -60.0, 60.0, 84.0, 96.0, -96.0] {
            let q = rotated(column, degrees);
            let tall = q
                .vertical_candidate()
                .unwrap_or_else(|| panic!("{degrees}"));
            assert!(tall.is_tall(), "{degrees}: {tall:?}");
            assert!(
                tall.angle().abs() < std::f32::consts::FRAC_PI_4,
                "{degrees}"
            );
            assert!(
                [Direction::TopToBottom, Direction::BottomToTop]
                    .iter()
                    .any(|&d| tall.turned(d) == q),
                "{degrees}: one reading is the quad as detected"
            );
        }
        for degrees in [0.0, 15.0, -30.0, -44.0] {
            assert!(rotated(column, degrees).vertical_candidate().is_none());
        }
        assert!(rotated(rect(0.0, 0.0, 40.0, 30.0), 80.0)
            .vertical_candidate()
            .is_none());
        let tall = rect(0.0, 0.0, 20.0, 100.0);
        assert_eq!(tall.vertical_candidate(), Some(tall));
    }
}
