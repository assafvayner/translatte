use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache};
use image::RgbImage;
use translatte_core::pipeline::TranslatedRegion;

pub const MIN_FONT_SIZE: f32 = 8.0;
const LINE_HEIGHT: f32 = 1.15;

const OUTLINE_RGB: image::Rgb<u8> = image::Rgb([255, 0, 0]);
const OUTLINE_THICKNESS: i32 = 2;

/// Marks where OCR found text: draws each region's quad edges in a visible color.
pub fn draw_outlines(img: &mut RgbImage, regions: &[TranslatedRegion]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut put = |x: i32, y: i32| {
        if (0..w).contains(&x) && (0..h).contains(&y) {
            img.put_pixel(x as u32, y as u32, OUTLINE_RGB);
        }
    };
    for r in regions {
        let pts = r.quad.0;
        for i in 0..4 {
            let (a, b) = (pts[i], pts[(i + 1) % 4]);
            let steps = (b.x - a.x).abs().max((b.y - a.y).abs()).ceil().max(1.0) as i32;
            for s in 0..=steps {
                let t = s as f32 / steps as f32;
                let x = (a.x + (b.x - a.x) * t).round() as i32;
                let y = (a.y + (b.y - a.y) * t).round() as i32;
                for dy in 0..OUTLINE_THICKNESS {
                    for dx in 0..OUTLINE_THICKNESS {
                        put(x + dx, y + dy);
                    }
                }
            }
        }
    }
}

/// Dev-only renderer: axis-aligned boxes, no rotation. The Android app does the real overlay.
pub struct Renderer {
    fonts: FontSystem,
    cache: SwashCache,
}

impl Renderer {
    pub fn new() -> Self {
        #[allow(unused_mut)]
        let mut fonts = FontSystem::new();
        // fontdb has no Android system font paths.
        #[cfg(target_os = "android")]
        {
            let db = fonts.db_mut();
            db.load_fonts_dir("/system/fonts");
            db.set_sans_serif_family("Roboto");
        }
        Self {
            fonts,
            cache: SwashCache::new(),
        }
    }

    fn layout(&mut self, text: &str, size: f32, width: f32) -> Buffer {
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, size * LINE_HEIGHT));
        buffer.set_size(&mut self.fonts, Some(width), None);
        buffer.set_text(&mut self.fonts, text, Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.fonts, false);
        buffer
    }

    pub fn fit_font_size(&mut self, text: &str, width: f32, height: f32) -> f32 {
        let mut size = (height / LINE_HEIGHT).max(MIN_FONT_SIZE);
        while size > MIN_FONT_SIZE {
            let buffer = self.layout(text, size, width);
            let runs: Vec<_> = buffer.layout_runs().collect();
            let fits_h = runs.len() as f32 * size * LINE_HEIGHT <= height;
            let fits_w = runs.iter().all(|r| r.line_w <= width + 0.5);
            if fits_h && fits_w {
                return size;
            }
            size *= 0.9;
        }
        MIN_FONT_SIZE
    }

    pub fn draw_regions(&mut self, img: &mut RgbImage, regions: &[TranslatedRegion]) {
        if img.width() == 0 || img.height() == 0 {
            return;
        }
        for r in regions {
            let Some(text) = &r.translated_text else {
                continue;
            };
            let b = r.quad.bbox();
            let (x0, y0) = (
                (b.x0.max(0.0) as u32).min(img.width() - 1),
                (b.y0.max(0.0) as u32).min(img.height() - 1),
            );
            let (x1, y1) = (
                (b.x1.max(0.0) as u32).min(img.width() - 1),
                (b.y1.max(0.0) as u32).min(img.height() - 1),
            );
            if x0 > x1
                || y0 > y1
                || b.x1 < 0.0
                || b.y1 < 0.0
                || b.x0 >= img.width() as f32
                || b.y0 >= img.height() as f32
            {
                continue;
            }
            for y in y0..=y1 {
                for x in x0..=x1 {
                    img.put_pixel(x, y, image::Rgb(r.bg_rgb));
                }
            }
            let (w, h) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
            let size = self.fit_font_size(text, w, h);
            let buffer = self.layout(text, size, w);
            let [fr, fg, fb] = r.fg_rgb;
            buffer.draw(
                &mut self.fonts,
                &mut self.cache,
                Color::rgb(fr, fg, fb),
                |gx, gy, gw, gh, color| {
                    let a = color.a() as f32 / 255.0;
                    if a == 0.0 {
                        return;
                    }
                    for dy in 0..gh as i32 {
                        for dx in 0..gw as i32 {
                            let (px, py) = (x0 as i32 + gx + dx, y0 as i32 + gy + dy);
                            if px < x0 as i32 || py < y0 as i32 || px > x1 as i32 || py > y1 as i32
                            {
                                continue;
                            }
                            let p = img.get_pixel_mut(px as u32, py as u32);
                            let src = [color.r(), color.g(), color.b()];
                            for (dst, s) in p.0.iter_mut().zip(src) {
                                *dst = (s as f32 * a + *dst as f32 * (1.0 - a)).round() as u8;
                            }
                        }
                    }
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;
    use translatte_core::geometry::{BBox, Quad};

    fn region(text: Option<&str>) -> TranslatedRegion {
        TranslatedRegion {
            quad: Quad::from_bbox(BBox {
                x0: 10.0,
                y0: 10.0,
                x1: 190.0,
                y1: 50.0,
            }),
            source_text: "src".into(),
            translated_text: text.map(str::to_string),
            bg_rgb: [255, 255, 0],
            fg_rgb: [0, 0, 0],
            angle: 0.0,
            line_height: 0.0,
        }
    }

    #[test]
    fn paints_background_and_some_text_pixels() {
        let mut img = RgbImage::from_pixel(200, 60, Rgb([0, 0, 255]));
        Renderer::new().draw_regions(&mut img, &[region(Some("Emergency exit"))]);
        assert_eq!(*img.get_pixel(12, 12), Rgb([255, 255, 0]));
        assert_eq!(*img.get_pixel(5, 5), Rgb([0, 0, 255]));
        let dark = img.pixels().filter(|p| p.0 == [0, 0, 0]).count();
        assert!(dark > 20, "expected glyph pixels, got {dark}");
    }

    #[test]
    fn regions_outside_or_straddling_the_image_do_not_panic() {
        let mut img = RgbImage::from_pixel(100, 50, Rgb([0, 0, 255]));
        let at = |x0: f32, y0: f32, x1: f32, y1: f32| {
            let mut r = region(Some("Emergency exit"));
            r.quad = Quad::from_bbox(BBox { x0, y0, x1, y1 });
            r
        };
        Renderer::new().draw_regions(
            &mut img,
            &[
                at(-30.0, -10.0, 40.0, 20.0),
                at(80.0, 30.0, 400.0, 300.0),
                at(500.0, 500.0, 600.0, 540.0),
                at(-90.0, -90.0, -10.0, -10.0),
            ],
        );
        assert_eq!(*img.get_pixel(0, 0), Rgb([255, 255, 0]));
        assert_eq!(*img.get_pixel(99, 49), Rgb([255, 255, 0]));
        assert_eq!(*img.get_pixel(60, 10), Rgb([0, 0, 255]));
    }

    #[test]
    fn outlines_trace_quad_edges_only() {
        let mut img = RgbImage::from_pixel(200, 60, Rgb([0, 0, 255]));
        draw_outlines(&mut img, &[region(None)]);
        assert_eq!(*img.get_pixel(100, 10), OUTLINE_RGB);
        assert_eq!(*img.get_pixel(10, 30), OUTLINE_RGB);
        assert_eq!(*img.get_pixel(100, 30), Rgb([0, 0, 255]));
        assert_eq!(*img.get_pixel(5, 5), Rgb([0, 0, 255]));
    }

    #[test]
    fn outlines_clip_to_the_image() {
        let mut img = RgbImage::from_pixel(50, 50, Rgb([0, 0, 255]));
        let mut r = region(None);
        r.quad = Quad::from_bbox(BBox {
            x0: -20.0,
            y0: -20.0,
            x1: 400.0,
            y1: 30.0,
        });
        draw_outlines(&mut img, &[r]);
        assert_eq!(*img.get_pixel(25, 30), OUTLINE_RGB);
    }

    #[test]
    fn zero_size_image_is_a_no_op() {
        let mut img = RgbImage::new(0, 10);
        Renderer::new().draw_regions(&mut img, &[region(Some("x"))]);
        assert_eq!(img.dimensions(), (0, 10));
    }

    #[test]
    fn untranslated_regions_are_left_untouched() {
        let mut img = RgbImage::from_pixel(200, 60, Rgb([0, 0, 255]));
        Renderer::new().draw_regions(&mut img, &[region(None)]);
        assert!(img.pixels().all(|p| p.0 == [0, 0, 255]));
    }

    #[test]
    fn long_text_shrinks_to_fit_box() {
        let mut r = Renderer::new();
        let size = r.fit_font_size(
            "a very long translated sentence that needs wrapping",
            180.0,
            40.0,
        );
        assert!((MIN_FONT_SIZE..40.0).contains(&size), "size {size}");
    }
}
