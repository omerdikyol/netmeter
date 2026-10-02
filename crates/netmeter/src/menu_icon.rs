//! The icon-only menu bar glyph.
//!
//! Two opposing arrows (down for received, up for transmitted) read as network
//! throughput at a glance and stay legible at menu bar size. Drawn by hand so it
//! needs no font and stays crisp.

use tray_icon::Icon;

/// Logical size of the glyph, in menu bar points.
const SIZE: u32 = 20;

#[cfg(target_os = "macos")]
const INK: [u8; 4] = [0, 0, 0, 255];
#[cfg(not(target_os = "macos"))]
const INK: [u8; 4] = [255, 255, 255, 255];

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

    fn rect(&mut self, x0: u32, y0: u32, x1: u32, y1: u32) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.plot(x, y);
            }
        }
    }

    /// Filled triangle pointing from `apex_y` out to a base of width `2 * half` at `base_y`.
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

/// The raw glyph, at 2x for a crisp result on Retina displays.
fn glyph() -> Canvas {
    let mut canvas = Canvas::new(SIZE * 2, SIZE * 2);
    let unit = SIZE as f64 * 2.0 / 20.0;

    // Down arrow on the left: head points down.
    canvas.rect(
        (3.2 * unit) as u32,
        (2.6 * unit) as u32,
        (6.4 * unit) as u32,
        (12.5 * unit) as u32,
    );
    canvas.triangle(4.8 * unit, 3.4 * unit, 10.6 * unit, 17.4 * unit);

    // Up arrow on the right: head points up.
    canvas.triangle(15.2 * unit, 3.4 * unit, 2.6 * unit, 9.4 * unit);
    canvas.rect(
        (13.6 * unit) as u32,
        (7.5 * unit) as u32,
        (16.8 * unit) as u32,
        (17.4 * unit) as u32,
    );

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

    fn ink(canvas: &Canvas, x0: u32, x1: u32) -> usize {
        let mut count = 0;
        for y in 0..canvas.height {
            for x in x0..x1 {
                let idx = ((y * canvas.width + x) * 4 + 3) as usize;
                if canvas.pixels[idx] > 0 {
                    count += 1;
                }
            }
        }
        count
    }

    #[test]
    fn glyph_has_ink_on_both_sides() {
        let canvas = glyph();
        assert_eq!((canvas.width, canvas.height), (SIZE * 2, SIZE * 2));
        let mid = SIZE;
        let left = ink(&canvas, 0, mid);
        let right = ink(&canvas, mid, SIZE * 2);
        assert!(left > 60, "left arrow too sparse: {left}");
        assert!(right > 60, "right arrow too sparse: {right}");
        assert!(
            left.abs_diff(right) < left.max(right) / 2,
            "halves should be similar: {left} vs {right}"
        );
    }

    #[test]
    fn glyph_is_transparent_at_the_corners() {
        let canvas = glyph();
        let corner = |x: u32, y: u32| canvas.pixels[((y * canvas.width + x) * 4 + 3) as usize];
        assert_eq!(corner(0, 0), 0);
        assert_eq!(corner(canvas.width - 1, canvas.height - 1), 0);
    }
}
