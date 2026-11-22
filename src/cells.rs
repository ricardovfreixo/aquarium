//! A depth-buffered grid of terminal character cells. Every cell holds one
//! glyph, one 24-bit foreground color, and a depth value; `plot` performs a
//! z-test so nearer geometry overwrites farther geometry. The buffer is
//! serialized as colored ANSI text (black background, spaces for empty
//! cells), which is what gives the scene its ASCII-art look.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub type Color = [u8; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    /// Camera distance of the sample that wrote this cell; `f32::MAX` = empty.
    depth: f32,
}

const BLANK: Cell = Cell {
    ch: ' ',
    fg: [0, 0, 0],
    depth: f32::MAX,
};

pub struct GlyphBuffer {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
}

impl GlyphBuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![BLANK; width * height],
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        *self = GlyphBuffer::new(width, height);
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Reset every cell to blank; called once per frame before painting.
    pub fn clear(&mut self) {
        self.cells.fill(BLANK);
    }

    /// Z-tested write at fractional cell coordinates. The sample wins when
    /// it is strictly nearer than whatever the cell currently holds.
    #[inline]
    pub fn plot(&mut self, x: f32, y: f32, depth: f32, ch: char, fg: Color) {
        let xi = x.round() as i32;
        let yi = y.round() as i32;
        if xi < 0 || yi < 0 || xi as usize >= self.width || yi as usize >= self.height {
            return;
        }
        let c = &mut self.cells[yi as usize * self.width + xi as usize];
        if depth < c.depth {
            *c = Cell { ch, fg, depth };
        }
    }

    pub fn cell(&self, x: usize, y: usize) -> Cell {
        self.cells[y * self.width + x]
    }

    /// Number of cells holding a visible glyph; a cheap complexity metric.
    pub fn nonblank(&self) -> usize {
        self.cells.iter().filter(|c| c.ch != ' ').count()
    }

    /// Serialize as ANSI true-color text: cursor home, colored glyphs,
    /// spaces (with color reset) for empty cells. The background is the
    /// terminal's own black; frames overwrite each other without flicker.
    pub fn to_ansi(&self, out: &mut String) {
        out.push_str("\x1b[H");
        let mut last: Option<Color> = None;
        for row in 0..self.height {
            for x in 0..self.width {
                let c = self.cell(x, row);
                if c.ch == ' ' {
                    if last.is_some() {
                        out.push_str("\x1b[0m");
                        last = None;
                    }
                    out.push(' ');
                } else {
                    if last != Some(c.fg) {
                        out.push_str(&format!("\x1b[38;2;{};{};{}m", c.fg[0], c.fg[1], c.fg[2]));
                        last = Some(c.fg);
                    }
                    out.push(c.ch);
                }
            }
            if row + 1 < self.height {
                out.push_str("\r\n");
                last = None;
            }
        }
        out.push_str("\x1b[0m");
    }

    /// Plain-text view of the glyphs with no color or escape codes.
    pub fn to_text(&self) -> String {
        let mut s = String::with_capacity(self.width * self.height + self.height);
        for row in 0..self.height {
            for x in 0..self.width {
                s.push(self.cell(x, row).ch);
            }
            s.push('\n');
        }
        s
    }

    /// Write the frame as ANSI text (for `less -R` or the PNG rasterizer).
    pub fn write_ansi(&self, path: &Path) -> io::Result<()> {
        let mut s = String::new();
        self.to_ansi(&mut s);
        let mut w = BufWriter::new(File::create(path)?);
        w.write_all(s.as_bytes())?;
        w.flush()
    }

    /// Write the frame as plain text (glyph layout only, no colors).
    pub fn write_text(&self, path: &Path) -> io::Result<()> {
        let mut w = BufWriter::new(File::create(path)?);
        w.write_all(self.to_text().as_bytes())?;
        w.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearer_sample_wins_depth_test() {
        let mut b = GlyphBuffer::new(4, 2);
        b.plot(1.0, 0.0, 5.0, 'x', [10, 0, 0]);
        b.plot(1.0, 0.0, 3.0, 'O', [0, 10, 0]); // nearer: replaces
        b.plot(1.0, 0.0, 7.0, '#', [0, 0, 10]); // farther: ignored
        assert_eq!(b.cell(1, 0).ch, 'O');
        assert_eq!(b.cell(1, 0).fg, [0, 10, 0]);
    }

    #[test]
    fn out_of_bounds_plots_are_ignored() {
        let mut b = GlyphBuffer::new(2, 2);
        b.plot(-3.0, 0.0, 1.0, 'x', [1, 1, 1]);
        b.plot(0.0, 99.0, 1.0, 'x', [1, 1, 1]);
        assert_eq!(b.nonblank(), 0);
    }

    #[test]
    fn ansi_homes_cursor_and_emits_truecolor() {
        let mut b = GlyphBuffer::new(3, 2);
        b.plot(0.0, 0.0, 1.0, '@', [200, 30, 90]);
        let mut s = String::new();
        b.to_ansi(&mut s);
        assert!(s.starts_with("\x1b[H"));
        assert!(s.contains("\x1b[38;2;200;30;90m"));
        assert!(s.contains('@'));
        assert!(s.ends_with("\x1b[0m"));
        // No stray color codes inside blank runs.
        assert!(!s.contains("\x1b[48;"));
    }

    #[test]
    fn text_and_ansi_round_trip_glyph_layout() {
        let mut b = GlyphBuffer::new(5, 2);
        b.plot(2.0, 1.0, 1.0, 'B', [9, 9, 9]);
        let text = b.to_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].chars().nth(2), Some('B'));
    }

    #[test]
    fn write_files() {
        let mut b = GlyphBuffer::new(3, 1);
        b.plot(1.0, 0.0, 1.0, 'o', [1, 2, 3]);
        let dir = std::env::temp_dir();
        let p1 = dir.join("ascii_aquarium_cells_test.ansi");
        let p2 = dir.join("ascii_aquarium_cells_test.txt");
        b.write_ansi(&p1).unwrap();
        b.write_text(&p2).unwrap();
        assert!(std::fs::read_to_string(&p1)
            .unwrap()
            .contains("\x1b[38;2;1;2;3m"));
        assert_eq!(std::fs::read_to_string(&p2).unwrap(), " o \n");
        std::fs::remove_file(p1).ok();
        std::fs::remove_file(p2).ok();
    }
}
