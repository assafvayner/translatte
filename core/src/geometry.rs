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
}
