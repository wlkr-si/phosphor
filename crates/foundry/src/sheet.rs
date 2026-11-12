//! Glyph sheets: a typeface baked into dots at one size, and drawing text with one.
//!
//! Baking turns a typeface into a glyph sheet ahead of time. Drawing text then needs no
//! font: each letter is already dots, with a coverage of 0..=255 per dot.
//!
//! Positions: text is laid out in a line box. `y` given to [`GlyphSheet::draw`] is the top
//! of the line box; the baseline sits `ascent` dots below it. A glyph's `top` is how far
//! its first row sits below the top of the line box, and its `left` is how far its first
//! column sits right of the pen. The pen moves right by `advance` after each letter.
//!
//! # Byte format, version 1
//!
//! All numbers are little-endian.
//!
//! ```text
//! "PHGS"                 4 bytes, magic
//! version                1 byte, 1
//! px                     u32
//! ascent                 i32
//! line_height            u32
//! name length            u32, then that many bytes of UTF-8
//! glyph count            u32, then for each glyph, in ascending char order:
//!   char                 u32, a Unicode scalar value
//!   w, h                 u32, u32
//!   left, top            i32, i32
//!   advance              u32
//!   coverage             w * h bytes, row-major
//! ```
//!
//! Nothing may follow the last glyph.

use std::collections::BTreeMap;

use phosphor_kernel::{Canvas, Rgb};

const MAGIC: &[u8; 4] = b"PHGS";
const VERSION: u8 = 1;

/// One letter as dots. `coverage` is row-major, `w * h` long, 0..=255 per dot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub w: u32,
    pub h: u32,
    pub left: i32,
    pub top: i32,
    pub advance: u32,
    pub coverage: Vec<u8>,
}

/// A typeface baked into dots at one size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphSheet {
    pub name: String,
    pub px: u32,
    pub ascent: i32,
    pub line_height: u32,
    pub glyphs: BTreeMap<char, Glyph>,
}

/// How a glyph's coverage becomes ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// A dot becomes the ink where coverage is 128 or more. For rungs with two colours per
    /// cell.
    Hard,
    /// The ink is blended over what is there by coverage. For the half rung, where each dot
    /// has its own colour.
    Soft,
}

/// Bake a TrueType or OpenType font at `px` dots per em into a glyph sheet holding `chars`.
/// A space is always baked. Characters the font lacks are left out of the sheet.
pub fn bake(font: &[u8], px: f32, chars: &str) -> Result<GlyphSheet, String> {
    if !px.is_finite() || !(1.0..=1024.0).contains(&px) {
        return Err(format!("size {px} px is outside 1..=1024"));
    }
    let face = fontdue::Font::from_bytes(font, fontdue::FontSettings::default())
        .map_err(|e| format!("cannot read font: {e}"))?;
    let lines = face
        .horizontal_line_metrics(px)
        .ok_or("font has no horizontal line metrics")?;

    // Rasterize first, measuring against the baseline (dots above it, dots below it).
    let mut above = lines.ascent.ceil().max(0.0) as i32;
    let mut below = (-lines.descent).ceil().max(0.0) as i32;
    let mut raw = Vec::new();
    for ch in chars.chars().chain(std::iter::once(' ')) {
        if ch != ' ' && face.lookup_glyph_index(ch) == 0 {
            continue;
        }
        let (m, coverage) = face.rasterize(ch, px);
        if m.width > 0 && m.height > 0 {
            above = above.max(m.ymin + m.height as i32);
            below = below.max(-m.ymin);
        }
        raw.push((ch, m, coverage));
    }

    let mut glyphs = BTreeMap::new();
    for (ch, m, coverage) in raw {
        let (w, h) = if m.width == 0 || m.height == 0 {
            (0, 0)
        } else {
            (m.width as u32, m.height as u32)
        };
        glyphs.insert(
            ch,
            Glyph {
                w,
                h,
                left: if w == 0 { 0 } else { m.xmin },
                top: if h == 0 {
                    0
                } else {
                    above - (m.ymin + m.height as i32)
                },
                advance: m.advance_width.round().max(0.0) as u32,
                coverage: if w == 0 { Vec::new() } else { coverage },
            },
        );
    }
    Ok(GlyphSheet {
        name: face.name().unwrap_or("unnamed").to_string(),
        px: px.round() as u32,
        ascent: above,
        line_height: (above + below) as u32,
        glyphs,
    })
}

/// The lab's 5 × 7 dot-matrix face: upper case, digits and some punctuation.
pub fn builtin_5x7() -> GlyphSheet {
    #[rustfmt::skip]
    const ROWS: &[(char, [u8; 7])] = &[
        ('A', [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11]),
        ('B', [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e]),
        ('C', [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e]),
        ('D', [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e]),
        ('E', [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f]),
        ('F', [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10]),
        ('G', [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f]),
        ('H', [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11]),
        ('I', [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e]),
        ('J', [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0c]),
        ('K', [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11]),
        ('L', [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f]),
        ('M', [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11]),
        ('N', [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11]),
        ('O', [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e]),
        ('P', [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10]),
        ('Q', [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d]),
        ('R', [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11]),
        ('S', [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e]),
        ('T', [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04]),
        ('U', [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e]),
        ('V', [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04]),
        ('W', [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a]),
        ('X', [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11]),
        ('Y', [0x11, 0x11, 0x11, 0x0a, 0x04, 0x04, 0x04]),
        ('Z', [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f]),
        ('0', [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e]),
        ('1', [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e]),
        ('2', [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f]),
        ('3', [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e]),
        ('4', [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02]),
        ('5', [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e]),
        ('6', [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e]),
        ('7', [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08]),
        ('8', [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e]),
        ('9', [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c]),
        ('.', [0, 0, 0, 0, 0, 0x0c, 0x0c]),
        (':', [0, 0x0c, 0x0c, 0, 0x0c, 0x0c, 0]),
        ('%', [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03]),
        ('/', [0, 0x01, 0x02, 0x04, 0x08, 0x10, 0]),
        ('-', [0, 0, 0, 0x1f, 0, 0, 0]),
        ('+', [0, 0x04, 0x04, 0x1f, 0x04, 0x04, 0]),
        ('>', [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08]),
    ];
    let mut glyphs = BTreeMap::new();
    glyphs.insert(
        ' ',
        Glyph {
            w: 0,
            h: 0,
            left: 0,
            top: 0,
            advance: 6,
            coverage: Vec::new(),
        },
    );
    for &(ch, rows) in ROWS {
        let coverage = rows
            .iter()
            .flat_map(|row| (0..5).map(move |k| if row >> (4 - k) & 1 == 1 { 255 } else { 0 }))
            .collect();
        glyphs.insert(
            ch,
            Glyph {
                w: 5,
                h: 7,
                left: 0,
                top: 0,
                advance: 6,
                coverage,
            },
        );
    }
    GlyphSheet {
        name: "builtin 5x7".to_string(),
        px: 7,
        ascent: 7,
        line_height: 8,
        glyphs,
    }
}

/// Where a line of text sits in its box: the pen's offset from the box's corner, and the
/// box's size. The box holds every dot of ink and the full line height and advances.
struct Layout {
    dx: i64,
    dy: i64,
    w: u32,
    h: u32,
}

impl GlyphSheet {
    /// How far the pen moves for a character not on the sheet.
    fn space_advance(&self) -> u32 {
        self.glyphs
            .get(&' ')
            .map_or(self.px.div_ceil(2), |g| g.advance)
    }

    fn layout(&self, text: &str) -> Layout {
        let (mut x0, mut y0) = (0i64, 0i64);
        let (mut x1, mut y1) = (0i64, self.line_height as i64);
        let mut pen = 0i64;
        for ch in text.chars() {
            match self.glyphs.get(&ch) {
                Some(g) => {
                    if g.w > 0 && g.h > 0 {
                        x0 = x0.min(pen + g.left as i64);
                        x1 = x1.max(pen + g.left as i64 + g.w as i64);
                        y0 = y0.min(g.top as i64);
                        y1 = y1.max(g.top as i64 + g.h as i64);
                    }
                    pen += g.advance as i64;
                }
                None => pen += self.space_advance() as i64,
            }
        }
        x1 = x1.max(pen);
        Layout {
            dx: -x0,
            dy: -y0,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        }
    }

    /// The size, in dots, of the box `draw` fills for `text`: (width, height).
    pub fn measure(&self, text: &str) -> (u32, u32) {
        let l = self.layout(text);
        (l.w, l.h)
    }

    /// Draw `text` with the box's top-left corner at (x, y). Dots outside the canvas are
    /// skipped. A character not on the sheet draws nothing and advances by a space.
    pub fn draw(&self, canvas: &mut Canvas, x: i64, y: i64, text: &str, ink: Rgb, edge: Edge) {
        let l = self.layout(text);
        let mut pen = x + l.dx;
        let base = y + l.dy;
        for ch in text.chars() {
            let Some(g) = self.glyphs.get(&ch) else {
                pen += self.space_advance() as i64;
                continue;
            };
            for row in 0..g.h as i64 {
                for col in 0..g.w as i64 {
                    let c = g.coverage[(row * g.w as i64 + col) as usize];
                    let (dx, dy) = (pen + g.left as i64 + col, base + g.top as i64 + row);
                    match edge {
                        Edge::Hard if c >= 128 => canvas.set(dx, dy, ink),
                        Edge::Soft if c > 0 => {
                            if let Some(old) = canvas.get(dx, dy) {
                                canvas.set(dx, dy, blend(old, ink, c));
                            }
                        }
                        _ => {}
                    }
                }
            }
            pen += g.advance as i64;
        }
    }

    /// The sheet in the byte format described at the top of this file.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&self.px.to_le_bytes());
        out.extend_from_slice(&self.ascent.to_le_bytes());
        out.extend_from_slice(&self.line_height.to_le_bytes());
        out.extend_from_slice(&(self.name.len() as u32).to_le_bytes());
        out.extend_from_slice(self.name.as_bytes());
        out.extend_from_slice(&(self.glyphs.len() as u32).to_le_bytes());
        for (&ch, g) in &self.glyphs {
            out.extend_from_slice(&(ch as u32).to_le_bytes());
            out.extend_from_slice(&g.w.to_le_bytes());
            out.extend_from_slice(&g.h.to_le_bytes());
            out.extend_from_slice(&g.left.to_le_bytes());
            out.extend_from_slice(&g.top.to_le_bytes());
            out.extend_from_slice(&g.advance.to_le_bytes());
            out.extend_from_slice(&g.coverage);
        }
        out
    }

    /// Read a sheet written by [`GlyphSheet::to_bytes`]. Anything malformed is an error.
    pub fn from_bytes(bytes: &[u8]) -> Result<GlyphSheet, String> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != MAGIC {
            return Err("not a glyph sheet: bad magic bytes".to_string());
        }
        let version = r.take(1)?[0];
        if version != VERSION {
            return Err(format!("glyph sheet version {version} is not supported"));
        }
        let px = r.u32()?;
        let ascent = r.i32()?;
        let line_height = r.u32()?;
        let name_len = r.u32()? as usize;
        let name = std::str::from_utf8(r.take(name_len)?)
            .map_err(|_| "glyph sheet name is not UTF-8")?
            .to_string();
        let count = r.u32()?;
        let mut glyphs = BTreeMap::new();
        for _ in 0..count {
            let code = r.u32()?;
            let ch = char::from_u32(code).ok_or(format!("{code:#x} is not a character"))?;
            let w = r.u32()?;
            let h = r.u32()?;
            let left = r.i32()?;
            let top = r.i32()?;
            let advance = r.u32()?;
            let len = (w as usize)
                .checked_mul(h as usize)
                .ok_or("glyph is too large")?;
            let coverage = r.take(len)?.to_vec();
            let glyph = Glyph {
                w,
                h,
                left,
                top,
                advance,
                coverage,
            };
            if glyphs.insert(ch, glyph).is_some() {
                return Err(format!("{ch:?} appears twice"));
            }
        }
        if r.at != bytes.len() {
            return Err("unexpected bytes after the last glyph".to_string());
        }
        Ok(GlyphSheet {
            name,
            px,
            ascent,
            line_height,
            glyphs,
        })
    }
}

/// `ink` over `old`, by coverage `c` out of 255.
fn blend(old: Rgb, ink: Rgb, c: u8) -> Rgb {
    let c = c as u32;
    let mix = |shift: u32| {
        let (o, i) = (old >> shift & 255, ink >> shift & 255);
        ((i * c + o * (255 - c) + 127) / 255) << shift
    };
    mix(16) | mix(8) | mix(0)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.bytes.len() - self.at < n {
            return Err("glyph sheet ends early".to_string());
        }
        let out = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONO: &[u8] = include_bytes!("../fixtures/JetBrainsMono-Regular.ttf");
    const PAPER: Rgb = 0x102030;
    const INK: Rgb = 0xffcc00;

    fn printable() -> String {
        (' '..='~').collect()
    }

    #[test]
    fn bytes_round_trip() {
        for sheet in [
            bake(MONO, 20.0, &printable()).unwrap(),
            bake(MONO, 12.0, "Hgé→").unwrap(),
            builtin_5x7(),
        ] {
            assert_eq!(GlyphSheet::from_bytes(&sheet.to_bytes()).unwrap(), sheet);
        }
    }

    #[test]
    fn bad_bytes_are_errors() {
        let good = builtin_5x7().to_bytes();
        let mut bad_magic = good.clone();
        bad_magic[0] = b'X';
        assert!(GlyphSheet::from_bytes(&bad_magic).is_err());
        assert!(GlyphSheet::from_bytes(b"").is_err());
        assert!(GlyphSheet::from_bytes(b"PHG").is_err());
        let mut bad_version = good.clone();
        bad_version[4] = 99;
        assert!(GlyphSheet::from_bytes(&bad_version).is_err());
        for cut in 0..good.len() {
            assert!(GlyphSheet::from_bytes(&good[..cut]).is_err());
        }
        let mut extra = good.clone();
        extra.push(0);
        assert!(GlyphSheet::from_bytes(&extra).is_err());
        // A huge glyph size must not allocate or panic.
        let mut huge = b"PHGS\x01".to_vec();
        for v in [12u32, 10, 12, 0, 1, 'A' as u32, u32::MAX, u32::MAX, 0, 0, 7] {
            huge.extend_from_slice(&v.to_le_bytes());
        }
        assert!(GlyphSheet::from_bytes(&huge).is_err());
    }

    #[test]
    fn hg_at_20_px_stays_inside_its_box() {
        let sheet = bake(MONO, 20.0, &printable()).unwrap();
        let (w, h) = sheet.measure("Hg");
        assert!(w > 0 && h > 0);
        let (x, y) = (7i64, 5i64);
        for edge in [Edge::Hard, Edge::Soft] {
            let mut canvas = Canvas::new(w as usize + 20, h as usize + 20, PAPER);
            sheet.draw(&mut canvas, x, y, "Hg", INK, edge);
            let mut inked = 0;
            for cy in 0..canvas.h as i64 {
                for cx in 0..canvas.w as i64 {
                    if canvas.get(cx, cy) != Some(PAPER) {
                        inked += 1;
                        assert!(
                            cx >= x && cx < x + w as i64 && cy >= y && cy < y + h as i64,
                            "{edge:?} ink at ({cx}, {cy}) outside {w}x{h} at ({x}, {y})"
                        );
                    }
                }
            }
            assert!(inked > 20, "{edge:?} drew only {inked} dots");
        }
    }

    #[test]
    fn descender_reaches_below_capital() {
        let sheet = bake(MONO, 20.0, "Hg").unwrap();
        let h = &sheet.glyphs[&'H'];
        let g = &sheet.glyphs[&'g'];
        assert!(g.top + g.h as i32 > h.top + h.h as i32);
        assert!(h.top + h.h as i32 <= sheet.ascent);
    }

    #[test]
    fn hard_writes_only_ink() {
        let sheet = bake(MONO, 20.0, "Hg").unwrap();
        let mut canvas = Canvas::new(40, 40, PAPER);
        sheet.draw(&mut canvas, 2, 2, "Hg", INK, Edge::Hard);
        assert!(canvas.dots.iter().all(|&d| d == PAPER || d == INK));
        assert!(canvas.dots.contains(&INK));
    }

    fn one_dot(coverage: u8) -> GlyphSheet {
        let mut glyphs = BTreeMap::new();
        glyphs.insert(
            'x',
            Glyph {
                w: 2,
                h: 1,
                left: 0,
                top: 0,
                advance: 2,
                coverage: vec![coverage, 128],
            },
        );
        GlyphSheet {
            name: "test".to_string(),
            px: 1,
            ascent: 1,
            line_height: 1,
            glyphs,
        }
    }

    #[test]
    fn soft_blends_by_coverage() {
        let mut canvas = Canvas::new(2, 1, PAPER);
        one_dot(255).draw(&mut canvas, 0, 0, "x", INK, Edge::Soft);
        assert_eq!(canvas.get(0, 0), Some(INK));
        let mut canvas = Canvas::new(2, 1, PAPER);
        one_dot(0).draw(&mut canvas, 0, 0, "x", INK, Edge::Soft);
        assert_eq!(canvas.get(0, 0), Some(PAPER));
        // Half coverage lands between the two.
        let mid = canvas.get(1, 0).unwrap();
        assert!(mid != PAPER && mid != INK);
    }

    #[test]
    fn hard_threshold_is_128() {
        let mut canvas = Canvas::new(2, 1, PAPER);
        one_dot(127).draw(&mut canvas, 0, 0, "x", INK, Edge::Hard);
        assert_eq!(canvas.dots, vec![PAPER, INK]);
    }

    #[test]
    fn missing_character_advances_by_a_space() {
        let sheet = builtin_5x7();
        assert_eq!(sheet.measure("A~A"), sheet.measure("A A"));
        let mut a = Canvas::new(20, 8, 0);
        let mut b = Canvas::new(20, 8, 0);
        sheet.draw(&mut a, 0, 0, "A~A", 1, Edge::Hard);
        sheet.draw(&mut b, 0, 0, "A A", 1, Edge::Hard);
        assert_eq!(a, b);
    }

    #[test]
    fn builtin_draws_the_labs_a() {
        let sheet = builtin_5x7();
        let mut canvas = Canvas::new(5, 7, 0);
        sheet.draw(&mut canvas, 0, 0, "A", 1, Edge::Hard);
        let lab = [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11];
        for (y, row) in lab.iter().enumerate() {
            for x in 0..5 {
                let want = (row >> (4 - x) & 1) as u32;
                assert_eq!(canvas.get(x, y as i64), Some(want), "dot ({x}, {y})");
            }
        }
        assert_eq!(sheet.measure("AB"), (12, 8));
    }

    #[test]
    fn bake_rejects_nonsense() {
        assert!(bake(b"not a font", 12.0, "A").is_err());
        assert!(bake(MONO, 0.0, "A").is_err());
        assert!(bake(MONO, f32::NAN, "A").is_err());
    }
}
