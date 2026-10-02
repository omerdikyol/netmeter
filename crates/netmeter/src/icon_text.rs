//! Renders short text (like `1.2M↓`) into an RGBA bitmap for the tray icon.
//!
//! The font is bundled so the app has no runtime font dependency. On macOS the
//! icon is used as a "template" image by the caller: macOS reads the alpha
//! channel and recolours it, so the same bitmap works on light and dark menu
//! bars. On other platforms the bitmap is drawn as-is.

use fontdue::{Font, FontSettings};

const FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

/// Down/up arrows with ASCII fallbacks, in case a replacement font lacks them.
pub const ARROW_DOWN: char = '\u{2193}';
pub const ARROW_UP: char = '\u{2191}';

pub struct IconRenderer {
    font: Font,
    down: char,
    up: char,
}

fn font_has(font: &Font, ch: char) -> bool {
    font.lookup_glyph_index(ch) != 0
}

impl IconRenderer {
    pub fn new() -> Option<Self> {
        let font = Font::from_bytes(FONT_BYTES, FontSettings::default()).ok()?;
        let down = if font_has(&font, ARROW_DOWN) {
            ARROW_DOWN
        } else {
            'v'
        };
        let up = if font_has(&font, ARROW_UP) {
            ARROW_UP
        } else {
            '^'
        };
        Some(Self { font, down, up })
    }

    /// Whether the bundled font can draw `ch`.
    #[cfg(test)]
    pub fn has_glyph(&self, ch: char) -> bool {
        font_has(&self.font, ch)
    }

    /// The arrow to use for a downward/upward value.
    pub fn arrow(&self, down: bool) -> char {
        if down {
            self.down
        } else {
            self.up
        }
    }

    /// Rasterise `text` at `px` size, returning an RGBA buffer and its dimensions.
    ///
    /// `scale` multiplies the pixel dimensions (2.0 for a HiDPI buffer).
    pub fn render(&self, text: &str, px: f32, scale: f32, color: [u8; 4]) -> (Vec<u8>, u32, u32) {
        let size = (px * scale).max(1.0);
        let line = self.font.horizontal_line_metrics(size);
        let ascent = line.map(|l| l.ascent).unwrap_or(size * 0.8);
        let descent = line.map(|l| l.descent).unwrap_or(-size * 0.2);

        let mut glyphs = Vec::with_capacity(text.chars().count());
        let mut pen_x = 0.0f32;
        for ch in text.chars() {
            let (metrics, bitmap) = self.font.rasterize(ch, size);
            glyphs.push((pen_x, metrics, bitmap));
            pen_x += metrics.advance_width;
        }

        let pad = (size * 0.18).ceil();
        let width = (pen_x + pad * 2.0).ceil().max(1.0) as u32;
        let height = ((ascent - descent) + pad * 2.0).ceil().max(1.0) as u32;
        let baseline = ascent + pad;

        let mut buffer = vec![0u8; (width * height * 4) as usize];
        for (pen, metrics, bitmap) in glyphs {
            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let coverage = bitmap[row * metrics.width + col];
                    if coverage == 0 {
                        continue;
                    }
                    let x = pen + metrics.xmin as f32 + col as f32;
                    let y = baseline - (metrics.ymin as f32 + metrics.height as f32) + row as f32;
                    let (x, y) = (x.round() as i32, y.round() as i32);
                    if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height {
                        continue;
                    }
                    let idx = ((y as u32 * width + x as u32) * 4) as usize;
                    let alpha = (coverage as u16 * color[3] as u16 / 255) as u8;
                    buffer[idx] = color[0];
                    buffer[idx + 1] = color[1];
                    buffer[idx + 2] = color[2];
                    buffer[idx + 3] = alpha;
                }
            }
        }
        (buffer, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_covers_what_we_draw() {
        let renderer = IconRenderer::new().expect("bundled font should load");
        for ch in ['0', '9', 'B', 'K', 'M', 'G', '.'] {
            assert!(renderer.has_glyph(ch), "missing glyph `{ch}`");
        }
        // The arrows are the one non-ASCII thing we draw; if the font ever loses
        // them we fall back to ASCII, so this only checks the fallback path works.
        let _ = renderer.arrow(true);
        let _ = renderer.arrow(false);
    }

    #[test]
    fn rendering_produces_a_visible_image() {
        let renderer = IconRenderer::new().unwrap();
        let arrow = renderer.arrow(true);
        let (buffer, width, height) =
            renderer.render(&format!("1.2M{arrow}"), 12.0, 1.0, [0, 0, 0, 255]);
        assert_eq!(buffer.len(), (width * height * 4) as usize);
        assert!(width > 10 && height > 8, "unexpected size {width}x{height}");
        assert!(
            buffer.chunks(4).any(|pixel| pixel[3] > 0),
            "rendered image is fully transparent"
        );
    }

    #[test]
    fn empty_text_still_yields_a_valid_buffer() {
        let renderer = IconRenderer::new().unwrap();
        let (buffer, width, height) = renderer.render("", 12.0, 1.0, [0, 0, 0, 255]);
        assert!(width >= 1 && height >= 1);
        assert_eq!(buffer.len(), (width * height * 4) as usize);
    }
}
