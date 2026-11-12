//! The shared kernel: the few things every part of Phosphor agrees on.
//!
//! Every word here is defined in CONTEXT.md. This crate is the contract between parts that
//! are built in parallel, so it changes only by agreement on the tracking issue.
//! It has no dependencies and does no input or output.

use std::io;
use std::time::Duration;

/// A colour, as 0xRRGGBB.
pub type Rgb = u32;

/// Squared distance between two colours. 0 means identical.
pub fn colour_distance(a: Rgb, b: Rgb) -> u32 {
    let d = |shift: u32| ((a >> shift & 255) as i32 - (b >> shift & 255) as i32).pow(2) as u32;
    d(16) + d(8) + d(0)
}

/// A picture made of dots. Row-major: dot (x, y) is `dots[y * w + x]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub dots: Vec<Rgb>,
}

impl Canvas {
    pub fn new(w: usize, h: usize, fill: Rgb) -> Canvas {
        Canvas {
            w,
            h,
            dots: vec![fill; w * h],
        }
    }
    /// The dot at (x, y), or `None` outside the canvas.
    pub fn get(&self, x: i64, y: i64) -> Option<Rgb> {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return None;
        }
        Some(self.dots[y as usize * self.w + x as usize])
    }
    /// Set one dot. Dots outside the canvas are ignored.
    pub fn set(&mut self, x: i64, y: i64, c: Rgb) {
        if x >= 0 && y >= 0 && x < self.w as i64 && y < self.h as i64 {
            self.dots[y as usize * self.w + x as usize] = c;
        }
    }
    /// Fill a rectangle, clipped to the canvas.
    pub fn fill_rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: Rgb) {
        for j in y.max(0)..(y + h).min(self.h as i64) {
            for i in x.max(0)..(x + w).min(self.w as i64) {
                self.dots[j as usize * self.w + i as usize] = c;
            }
        }
    }
}

/// One character position on a surface: a character drawn in ink on paper.
/// A space shows only paper, so its ink is ignored when cells are compared for display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub ink: Rgb,
    pub paper: Rgb,
}

impl Cell {
    pub const BLANK: Cell = Cell {
        ch: ' ',
        ink: 0,
        paper: 0,
    };
    /// True when two cells look the same on screen.
    pub fn looks_like(&self, other: &Cell) -> bool {
        self.ch == other.ch
            && self.paper == other.paper
            && (self.ch == ' ' || self.ink == other.ink)
    }
}

/// What a surface shows: `cols` by `rows` cells, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Grid {
        Grid {
            cols,
            rows,
            cells: vec![Cell::BLANK; cols * rows],
        }
    }
    pub fn get(&self, col: usize, row: usize) -> Cell {
        self.cells[row * self.cols + col]
    }
    pub fn set(&mut self, col: usize, row: usize, cell: Cell) {
        self.cells[row * self.cols + col] = cell;
    }
}

/// A rung of the dot ladder: how many dots one cell is split into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ladder {
    /// 1 x 2 dots. Each dot takes its own colour, so there is no ink rule to break.
    Half,
    /// 2 x 2 dots, two colours per cell.
    Quarter,
    /// 2 x 3 dots, two colours per cell.
    Sixth,
    /// 2 x 4 dots, two colours per cell.
    Eighth,
}

impl Ladder {
    /// Dots across and down inside one cell.
    pub fn dots(self) -> (usize, usize) {
        match self {
            Ladder::Half => (1, 2),
            Ladder::Quarter => (2, 2),
            Ladder::Sixth => (2, 3),
            Ladder::Eighth => (2, 4),
        }
    }
    /// The canvas size, in dots, that exactly covers a grid.
    pub fn canvas_size(self, cols: usize, rows: usize) -> (usize, usize) {
        let (dw, dh) = self.dots();
        (cols * dw, rows * dh)
    }
}

/// How many colours a surface can be asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColourDepth {
    /// Any 0xRRGGBB.
    Full,
    /// The standard 256-colour table (16 basic, a 6x6x6 cube, 24 greys).
    Indexed256,
    /// Eight plain colours: black, red, green, yellow, blue, magenta, cyan, white.
    Indexed8,
}

/// Shortcuts and behaviours the witness has seen work on a surface. `false` means unproven,
/// and the encoder must not use it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// "Repeat the last character n times" is right for block characters.
    pub repeat_blocks: bool,
    /// "Erase n characters" paints them in the current paper colour.
    pub erase: bool,
    /// Line feed, carriage return, backspace and cursor-forward move as expected.
    pub relative_moves: bool,
    /// The surface can hold a frame back until it is complete.
    pub atomic_frames: bool,
}

/// What a frame contains, counted. The governor prices this before the frame is sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// Bytes sent.
    pub bytes: u64,
    /// Cells painted.
    pub cells: u64,
    /// Colour changes sent. A run is a stretch of cells painted without changing colour.
    pub runs: u64,
    /// For each row painted, the number of runs on it, squared, summed over rows.
    /// Some surfaces get slower per run the more runs a row holds.
    pub crowding: u64,
}

impl Tally {
    pub fn add(&mut self, other: &Tally) {
        self.bytes += other.bytes;
        self.cells += other.cells;
        self.runs += other.runs;
        self.crowding += other.crowding;
    }
}

/// What a surface charges, in nanoseconds of its own time, measured on the real thing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PriceList {
    /// Fixed cost of any frame that sends something.
    pub frame_ns: f64,
    pub byte_ns: f64,
    pub cell_ns: f64,
    pub run_ns: f64,
    /// Charged per unit of `Tally::crowding`.
    pub crowding_ns: f64,
}

impl PriceList {
    /// The time this surface needs to take in a frame with this tally.
    pub fn price(&self, t: &Tally) -> f64 {
        if t.bytes == 0 {
            return 0.0;
        }
        self.frame_ns
            + self.byte_ns * t.bytes as f64
            + self.cell_ns * t.cells as f64
            + self.run_ns * t.runs as f64
            + self.crowding_ns * t.crowding as f64
    }
}

/// The time a surface has for one frame: one beat. It is never less than 1 / the surface's
/// paint rate, because the beat never runs faster than the surface is seen to paint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    pub frame_ns: f64,
}

impl Budget {
    pub fn per_second(frames: f64) -> Budget {
        Budget {
            frame_ns: 1e9 / frames,
        }
    }
}

/// Everything known to be true about one surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub name: String,
    /// Rungs of the dot ladder this surface draws correctly, coarsest first.
    pub ladders: Vec<Ladder>,
    pub colour: ColourDepth,
    pub can: Capabilities,
    pub prices: PriceList,
    /// How many times a second the surface has been **seen** to paint: measured on film,
    /// never taken from acks. The beat never runs faster than this.
    pub paint_hz: f64,
}

impl Profile {
    /// Assume the least. Safe on anything that understands cursor moves and colour.
    pub fn cautious() -> Profile {
        Profile {
            name: "cautious".into(),
            ladders: vec![Ladder::Half],
            colour: ColourDepth::Indexed256,
            can: Capabilities {
                repeat_blocks: false,
                erase: false,
                relative_moves: false,
                atomic_frames: false,
            },
            prices: PriceList {
                frame_ns: 0.0,
                byte_ns: 500.0,
                cell_ns: 0.0,
                run_ns: 20_000.0,
                crowding_ns: 100.0,
            },
            // Not filmed. Below the 22 a second Apple's Terminal is seen to paint a plain picture
            // (docs/measurements/cadence-ticker.md); a rich one there is seen at about 14.
            paint_hz: 20.0,
        }
    }
    /// Apple's Terminal 2.15 on macOS 27.2, from docs/measurements/apple-terminal-2.15.md.
    /// The prices are a first fit from seven runs and will be replaced by a proper calibration.
    pub fn apple_terminal_2_15() -> Profile {
        Profile {
            name: "apple-terminal-2.15".into(),
            ladders: vec![Ladder::Half, Ladder::Quarter],
            colour: ColourDepth::Full,
            can: Capabilities {
                repeat_blocks: false,
                erase: true,
                relative_moves: true,
                atomic_frames: false,
            },
            prices: PriceList {
                frame_ns: 0.0,
                byte_ns: 46.0,
                cell_ns: 0.0,
                run_ns: 1_700.0,
                crowding_ns: 93.0,
            },
            // The fastest beat at which every frame of the governed benchmark was seen in every
            // take: filmed for 30 s, quarter rung, Menlo 12, 160 x 45, 0 frames skipped in five
            // takes of five at 14 a second. At 15, two takes of six missed 1 or 2 frames; 18 to
            // 24 a second are all seen at about 14.5. See docs/measurements/cadence-paced.md.
            paint_hz: 14.0,
        }
    }
}

/// The two-way line to a surface. The surface part implements it for a real terminal;
/// tests implement it with a script.
pub trait Wire {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Read until `end` arrives or `timeout` passes. Returns whatever arrived.
    fn recv_until(&mut self, end: u8, timeout: Duration) -> io::Result<Vec<u8>>;
}

/// A test picture that shows up every kind of fault: a white one-dot border, a smooth colour
/// field, one-dot checks, one-dot diagonals and rings. `shift` slides it diagonally, wrapping.
pub fn test_card(w: usize, h: usize, shift: usize) -> Canvas {
    let mut c = Canvas::new(w, h, 0);
    let third = (w / 3).max(1);
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = ((x + shift) % w, (y + shift) % h);
            let colour = if sx == 0 || sy == 0 || sx == w - 1 || sy == h - 1 {
                0xffffff
            } else if sx < third {
                ((255 * sx / third) as u32) << 16 | ((255 * sy / h) as u32) << 8 | 160
            } else if sx < 2 * third {
                if sy < h / 2 {
                    if (sx + sy) & 1 == 0 {
                        0xffffff
                    } else {
                        0x000000
                    }
                } else if (sx + sy) % 8 == 0 {
                    0x33ff66
                } else {
                    0x06120b
                }
            } else {
                let (dx, dy) = (
                    sx as i64 - (2 * third + third / 2) as i64,
                    sy as i64 - h as i64 / 2,
                );
                if ((dx * dx + dy * dy) as f64).sqrt() as usize % 16 < 8 {
                    0xffb000
                } else {
                    0x202020
                }
            };
            c.dots[y * w + x] = colour;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_ignores_dots_outside_it() {
        let mut c = Canvas::new(4, 3, 7);
        c.set(-1, 0, 9);
        c.set(4, 0, 9);
        c.set(3, 2, 9);
        assert_eq!(c.get(3, 2), Some(9));
        assert_eq!(c.get(4, 0), None);
        assert_eq!(c.dots.iter().filter(|d| **d == 9).count(), 1);
    }

    #[test]
    fn a_space_ignores_its_ink() {
        let a = Cell {
            ch: ' ',
            ink: 1,
            paper: 5,
        };
        let b = Cell {
            ch: ' ',
            ink: 2,
            paper: 5,
        };
        assert!(a.looks_like(&b));
        assert!(!Cell {
            ch: '▀',
            ink: 1,
            paper: 5
        }
        .looks_like(&Cell {
            ch: '▀',
            ink: 2,
            paper: 5
        }));
    }

    #[test]
    fn ladder_sizes() {
        assert_eq!(Ladder::Half.canvas_size(160, 45), (160, 90));
        assert_eq!(Ladder::Eighth.canvas_size(160, 45), (320, 180));
    }

    #[test]
    fn apple_terminal_prices_match_what_was_measured() {
        // 160 x 45 cells, every cell its own colour: measured 120 ms a frame.
        let p = Profile::apple_terminal_2_15().prices;
        let every_cell = Tally {
            bytes: 250_000,
            cells: 7_200,
            runs: 7_200,
            crowding: 45 * 160 * 160,
        };
        let ms = p.price(&every_cell) / 1e6;
        assert!((100.0..150.0).contains(&ms), "{ms}");
        // The same cells with shapes only: measured about 1 ms.
        let shapes = Tally {
            bytes: 22_000,
            cells: 7_200,
            runs: 1,
            crowding: 1,
        };
        assert!(p.price(&shapes) / 1e6 < 2.0);
        // Nothing sent costs nothing.
        assert_eq!(p.price(&Tally::default()), 0.0);
    }

    #[test]
    fn test_card_has_a_white_border_and_slides() {
        let c = test_card(60, 40, 0);
        assert_eq!(c.get(0, 5), Some(0xffffff));
        assert_eq!(c.get(59, 39), Some(0xffffff));
        assert_ne!(test_card(60, 40, 1), c);
        assert_eq!(test_card(60, 40, 0), c);
    }
}
