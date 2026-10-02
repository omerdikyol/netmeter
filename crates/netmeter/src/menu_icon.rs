//! The icon-only menu bar glyph.
//!
//! Two opposing arrows (down for received, up for transmitted) read as network
//! throughput at a glance. Drawn by hand so it needs no font and stays crisp:
//! the canvas is 2x so macOS can scale it down to the bar height and still be
//! pixel-perfect on Retina.

use tray_icon::Icon;

/// Logical size of the glyph, in menu bar points.
const SIZE: u32 = 20;

#[cfg(target_os = "macos")]
const INK: [u8; 4] = [0, 0, 0, 255];
#[cfg(not(target_os = "macos"))]
const INK: [u8; 4] = [255, 255, 255, 255];

/// Vertical extent shared by both arrows.
const TOP: f64 = 3.0;
const BOTTOM: f64 = 36.0;
/// Height of an arrow head.
const HEAD: f64 = 15.0;
const HALF_HEAD: f64 = 7.0;
const HALF_STEM: f64 = 2.0;

struct Canvas {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            pixels: vec![0; (width * height * 4) as usize],
            width,
            height,
        }
    }

    fn plot(&mut self, x: u32, y: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.pixels[idx..idx + 4].copy_from_slice(&INK);
    }

    /// Fill `[x0, x1) x [y0, y1)`.
    fn rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        for y in (y0.round().max(0.0) as u32)..(y1.round().max(0.0) as u32) {
            for x in (x0.round().max(0.0) as u32)..(x1.round().max(0.0) as u32) {
                self.plot(x, y);
            }
        }
    }

    /// Filled triangle about `cx`: a point at `apex_y` widening to `2 * half` at `base_y`.
    fn triangle(&mut self, cx: f64, half: f64, apex_y: f64, base_y: f64) {
        let steps = (base_y - apex_y).abs().ceil().max(1.0) as i64;
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            let y = apex_y + (base_y - apex_y) * t;
            if y < 0.0 {
                continue;
            }
            let width = half * t;
            let mut x = (cx - width).floor() as i64;
            let end = (cx + width).ceil() as i64;
            while x <= end {
                if x >= 0 {
                    self.plot(x as u32, y.round() as u32);
                }
                x += 1;
            }
        }
    }
}

/// The raw glyph, at 2x.
fn glyph() -> Canvas {
    let mut canvas = Canvas::new(SIZE * 2, SIZE * 2);
    let down_x = 9.5;
    let up_x = 30.5;

    // Left: an arrow pointing down. The stem sits on top of a head that
    // narrows to a point at the bottom.
    canvas.rect(down_x - HALF_STEM, TOP, down_x + HALF_STEM, BOTTOM - HEAD);
    canvas.triangle(down_x, HALF_HEAD, BOTTOM, BOTTOM - HEAD);

    // Right: the mirror of it, pointing up.
    canvas.triangle(up_x, HALF_HEAD, TOP, TOP + HEAD);
    canvas.rect(up_x - HALF_STEM, TOP + HEAD, up_x + HALF_STEM, BOTTOM);

    canvas
}

/// Build the menu bar icon.
pub fn activity_icon() -> Icon {
    let canvas = glyph();
    Icon::from_rgba(canvas.pixels, canvas.width, canvas.height).expect("valid icon dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(canvas: &Canvas, x: u32, y: u32) -> u8 {
        canvas.pixels[((y * canvas.width + x) * 4 + 3) as usize]
    }

    fn ink_width(canvas: &Canvas, y: u32, x0: u32, x1: u32) -> usize {
        (x0..x1).filter(|&x| alpha(canvas, x, y) > 0).count()
    }

    fn ink_in_column(canvas: &Canvas, x: u32) -> usize {
        (0..canvas.height)
            .filter(|&y| alpha(canvas, x, y) > 0)
            .count()
    }

    #[test]
    fn both_arrows_have_ink() {
        let canvas = glyph();
        assert_eq!((canvas.width, canvas.height), (SIZE * 2, SIZE * 2));
        let mid = SIZE;
        let left: usize = (0..mid).map(|x| ink_in_column(&canvas, x)).sum();
        let right: usize = (mid..SIZE * 2).map(|x| ink_in_column(&canvas, x)).sum();
        assert!(left > 60, "left arrow too sparse: {left}");
        assert!(right > 60, "right arrow too sparse: {right}");
    }

    #[test]
    fn heads_point_the_right_way() {
        let canvas = glyph();
        let wide = ink_width(&canvas, 24, 0, 20);
        let tip = ink_width(&canvas, 34, 0, 20);
        assert!(
            wide > tip,
            "the down arrow's head should taper to a point (width {wide} -> {tip})"
        );

        let tip = ink_width(&canvas, 5, 20, 40);
        let wide = ink_width(&canvas, 15, 20, 40);
        assert!(
            wide > tip,
            "the up arrow's head should widen downwards (width {tip} -> {wide})"
        );
    }

    #[test]
    fn arrows_are_balanced_and_apart() {
        let canvas = glyph();
        let inked: Vec<u32> = (0..canvas.width)
            .filter(|&x| ink_in_column(&canvas, x) > 0)
            .collect();
        let first = *inked.first().expect("some ink");
        let last = *inked.last().expect("some ink");

        let left_margin = first as usize;
        let right_margin = (canvas.width - 1 - last) as usize;
        assert!(
            left_margin.abs_diff(right_margin) <= 1,
            "margins should be even: {left_margin} vs {right_margin}"
        );

        let span = (last - first + 1) as usize;
        let gap = span - inked.len();
        assert!(gap >= 4, "the two arrows should have a clear gap: {gap}px");
    }

    #[test]
    fn glyph_is_transparent_at_the_corners() {
        let canvas = glyph();
        assert_eq!(alpha(&canvas, 0, 0), 0);
        assert_eq!(alpha(&canvas, canvas.width - 1, canvas.height - 1), 0);
    }

    #[test]
    #[ignore = "dev helper: prints the glyph so it can be eyeballed"]
    fn dump_icon_preview() {
        let canvas = glyph();
        for y in 0..canvas.height {
            let mut row = String::new();
            for x in 0..canvas.width {
                row.push(match alpha(&canvas, x, y) {
                    200..=255 => '#',
                    80..=199 => '+',
                    1..=79 => '.',
                    _ => ' ',
                });
            }
            println!("{row}");
        }
    }
}
