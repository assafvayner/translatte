use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache};
use image::RgbImage;
use translatte_core::pipeline::TranslatedRegion;

pub const MIN_FONT_SIZE: f32 = 8.0;
const LINE_HEIGHT: f32 = 1.15;

/// Dev-only renderer: axis-aligned boxes, no rotation. The Android app does the real overlay.
pub struct Renderer {
    fonts: FontSystem,
    cache: SwashCache,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            fonts: FontSystem::new(),
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
        for r in regions {
            let Some(text) = &r.translated_text else {
                continue;
            };
            let b = r.quad.bbox();
            let (x0, y0) = (b.x0.max(0.0) as u32, b.y0.max(0.0) as u32);
            let (x1, y1) = (
                (b.x1 as u32).min(img.width() - 1),
                (b.y1 as u32).min(img.height() - 1),
            );
            for y in y0..=y1 {
                for x in x0..=x1 {
                    img.put_pixel(x, y, image::Rgb(r.bg_rgb));
                }
            }
            let (w, h) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
            let size = self.fit_font_size(text, w, h);
            let buffer = self.layout(text, size, w);
            let [fr, fg, fb] = r.fg_rgb;
            let (img_w, img_h) = (img.width() as i32, img.height() as i32);
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
                            if px < 0 || py < 0 || px >= img_w || py >= img_h || py > y1 as i32 {
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
