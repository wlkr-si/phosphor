// Phosphor lab: the terminal as a grid of tiny pixels.
// Everything, text included, is drawn into a pixel canvas, then each character cell is
// fitted with one block character and two colours. Throwaway experiment. `rustc -O lab.rs`.
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::process::Command;
use std::time::{Duration, Instant};

include!("tables.rs");

// ---------------- cells ----------------
#[derive(Clone, Copy, PartialEq, Debug)]
struct Cell {
    ch: char,
    fg: u32,
    bg: u32,
}
fn same(a: &Cell, b: &Cell) -> bool {
    a.ch == b.ch && (a.ch == ' ' || a.fg == b.fg) && a.bg == b.bg
}
struct Grid {
    w: usize,
    h: usize,
    c: Vec<Cell>,
}
impl Grid {
    fn new(w: usize, h: usize, ch: char) -> Grid {
        Grid { w, h, c: vec![Cell { ch, fg: 0, bg: 0 }; w * h] }
    }
}

// ---------------- pixels ----------------
const ROOM: u32 = 0x08090b;
const CASE: u32 = 0xc9c1a9;
const CASE_DK: u32 = 0x6f6a59;
const SCREEN: u32 = 0x06120b;
const INK: u32 = 0x33ff66;
const DIMC: u32 = 0x1f9a4c;
const TRACK: u32 = 0x0f3320;
const AMBER: u32 = 0xffb000;

struct Img {
    w: i32,
    h: i32,
    px: Vec<u32>,
}
impl Img {
    fn new(w: usize, h: usize) -> Img {
        Img { w: w as i32, h: h as i32, px: vec![ROOM; w * h] }
    }
    fn set(&mut self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = c;
        }
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        for j in y.max(0)..(y + h).min(self.h) {
            for i in x.max(0)..(x + w).min(self.w) {
                self.px[(j * self.w + i) as usize] = c;
            }
        }
    }
    fn text(&mut self, x: i32, y: i32, s: &str, c: u32) {
        for (i, ch) in s.chars().enumerate() {
            let g = glyph(ch.to_ascii_uppercase());
            for r in 0..7 {
                for k in 0..5 {
                    if g[r] >> (4 - k) & 1 == 1 {
                        self.set(x + i as i32 * 6 + k, y + r as i32, c);
                    }
                }
            }
        }
    }
}

// A 5x7 dot-matrix face, the kind an old terminal had in a ROM chip.
fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0c],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x11, 0x0a, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
        '6' => [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
        '.' => [0, 0, 0, 0, 0, 0x0c, 0x0c],
        ':' => [0, 0x0c, 0x0c, 0, 0x0c, 0x0c, 0],
        '%' => [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
        '/' => [0, 0x01, 0x02, 0x04, 0x08, 0x10, 0],
        '-' => [0, 0, 0, 0x1f, 0, 0, 0],
        '+' => [0, 0x04, 0x04, 0x1f, 0x04, 0x04, 0],
        '>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        _ => [0; 7],
    }
}

fn rnd(seed: u32) -> f64 {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(0x9e3779b9);
    x ^= x >> 15;
    x = x.wrapping_mul(0x85ebca6b);
    x ^= x >> 13;
    (x & 0xffff) as f64 / 65535.0
}
fn smooth(u: f64) -> f64 {
    u * u * (3.0 - 2.0 * u)
}
// smooth wandering value in 0..1
fn noise(seed: u32, x: f64) -> f64 {
    let k = x.floor();
    let (a, b) = (rnd(seed.wrapping_add(k as u32)), rnd(seed.wrapping_add(k as u32 + 1)));
    a + (b - a) * smooth(x - k)
}

// The thing being bounced: a retro computer with a live status screen. All of it pixels.
fn computer(s: &mut Img, t: f64) {
    let (w, h) = (s.w, s.h);
    s.px.fill(ROOM);
    let mh = h * 78 / 100;
    let bz = (w / 40).max(5);
    let chin = 11;
    s.rect(0, 0, w, mh, CASE);
    for (x, y) in [(0, 0), (w - 1, 0), (0, mh - 1), (w - 1, mh - 1)] {
        s.set(x, y, ROOM);
    }
    let (sx, sy, sw, sh) = (bz, bz, w - 2 * bz, mh - bz - chin - 2);
    s.rect(sx - 1, sy - 1, sw + 2, sh + 2, CASE_DK);
    s.rect(sx, sy, sw, sh, SCREEN);
    s.text(bz + 2, mh - chin + 1, "OCTAL8", CASE_DK);
    let led = if (t as i64) % 2 == 0 { INK } else { CASE_DK };
    s.rect(w - bz - 6, mh - chin + 3, 3, 3, led);
    s.rect(w / 2 - w / 10, mh, w / 5, 3, CASE_DK);
    let (by, bh) = (mh + 3, h - mh - 3);
    s.rect(w / 20, by, w - w / 10, bh, CASE);
    s.rect(w * 55 / 100, by + bh / 2 - 1, w / 7, 2, CASE_DK);
    s.rect(w * 73 / 100, by + bh / 2 - 1, w / 7, 2, CASE_DK);
    for i in 0..5 {
        s.rect(w / 20 + 6 + i * 4, by + 3, 2, bh - 6, CASE_DK);
    }
    // what is on the screen
    let (x, y, w, h) = (sx + 4, sy + 4, sw - 8, sh - 8);
    s.text(x, y, "OCTAL8 STATUS", INK);
    let sec = t as i64;
    s.text(x + w - 48, y, &format!("{:02}:{:02}:{:02}", sec / 3600 % 24, sec / 60 % 60, sec % 60), AMBER);
    s.rect(x, y + 9, w, 1, DIMC);
    let footer = h >= 70;
    let body = h - 13 - if footer { 11 } else { 0 };
    let n = (w / 150).max(1);
    let pw = w / n;
    for p in 0..n {
        pane(s, x + p * pw, y + 13, pw - 6, body, p as u32, t);
    }
    if footer {
        let done = 12 + (t / 9.0) as i64 % 3;
        s.text(x, y + h - 7, &format!("BUILD MODE {}/14 DONE", done), DIMC);
    }
}
fn pane(s: &mut Img, x: i32, y: i32, w: i32, h: i32, p: u32, t: f64) {
    s.text(x, y, ["WORKHORSE", "FRONT DOOR", "MAC", "SPARE"][p as usize % 4], INK);
    for i in 0..3 {
        let yy = y + 10 + i * 9;
        if yy + 7 > y + h {
            return;
        }
        let id = p * 16 + i as u32;
        let span = [1.0, 0.3, 0.02][i as usize];
        let tick = (t * 2.0).floor();
        let (a, b) = (rnd(id * 7919 + tick as u32), rnd(id * 7919 + tick as u32 + 1));
        let v = 0.15 + span * 0.8 * (a + (b - a) * smooth(t * 2.0 - tick));
        s.text(x, yy, ["CPU", "MEM", "DSK"][i as usize], DIMC);
        let (bx, bw) = (x + 22, w - 22 - 28);
        s.rect(bx, yy + 1, bw, 5, TRACK);
        s.rect(bx, yy + 1, (v * bw as f64).round() as i32, 5, INK);
        s.text(x + w - 24, yy, &format!("{:3}%", ((0.15 + span * 0.8 * b) * 100.0) as i32), AMBER);
    }
    // live history chart, sliding left a pixel at a time
    let cy = y + 10 + 27 + 2;
    let ch = y + h - cy;
    if ch < 8 {
        return;
    }
    s.rect(x, cy + ch - 1, w, 1, DIMC);
    for cx in 0..w {
        let k = t * 20.0 + cx as f64;
        let val = 0.75 * noise(p * 131, k / 14.0) + 0.25 * noise(p * 17 + 9, k / 3.0);
        let hh = (val * (ch - 3) as f64) as i32 + 1;
        s.rect(x + cx, cy + ch - 1 - hh, 1, hh, TRACK);
        s.set(x + cx, cy + ch - 1 - hh, INK);
    }
}

// Load an uncompressed BMP and shrink it to fit, averaging the pixels under each dot.
fn picture(path: &str, maxw: usize, maxh: usize, aspect: f64) -> Img {
    let d = std::fs::read(path).expect("image file");
    let le = |o: usize, n: usize| (0..n).fold(0u32, |a, i| a | (d[o + i] as u32) << (8 * i));
    let (off, w, hraw, bpp) = (le(10, 4) as usize, le(18, 4) as usize, le(22, 4) as i32, le(28, 2) as usize / 8);
    let (h, flip) = (hraw.unsigned_abs() as usize, hraw > 0);
    let stride = (w * bpp + 3) / 4 * 4;
    // a dot is `aspect` times as tall as it is wide
    let scale = (maxw as f64 / w as f64).min(maxh as f64 * aspect / h as f64);
    let (ow, oh) = ((w as f64 * scale) as usize, (h as f64 * scale / aspect) as usize);
    let mut out = Img::new(ow.max(1), oh.max(1));
    for y in 0..oh {
        for x in 0..ow {
            let (x0, x1) = (x * w / ow, ((x + 1) * w / ow).max(x * w / ow + 1));
            let (y0, y1) = (y * h / oh, ((y + 1) * h / oh).max(y * h / oh + 1));
            let (mut acc, mut n) = ([0u32; 3], 0u32);
            for sy in y0..y1.min(h) {
                let row = off + (if flip { h - 1 - sy } else { sy }) * stride;
                for sx in x0..x1.min(w) {
                    for c in 0..3 {
                        acc[c] += d[row + sx * bpp + c] as u32;
                    }
                    n += 1;
                }
            }
            out.px[y * ow + x] = (acc[2] / n) << 16 | (acc[1] / n) << 8 | acc[0] / n;
        }
    }
    out
}

// ---------------- canvas: where the computer is right now ----------------
fn tri(d: f64, room: f64) -> f64 {
    if room <= 0.0 {
        return 0.0;
    }
    let m = d % (2.0 * room);
    if m > room { 2.0 * room - m } else { m }
}
fn mix(a: u32, b: u32, t: f64) -> u32 {
    let m = |s: u32| {
        let (x, y) = ((a >> s & 255) as f64, (b >> s & 255) as f64);
        ((x + (y - x) * t + 0.5) as u32) << s
    };
    m(16) | m(8) | m(0)
}
// Place the sprite on the canvas. `sub` > 1 allows positions between pixels by blending.
fn compose(cv: &mut Img, sp: &Img, t: f64, speed: f64, sub: u32) {
    let d = t * speed;
    let (ox, oy) = (tri(d, (cv.w - sp.w) as f64), tri(d, (cv.h - sp.h) as f64));
    let q = sub as f64;
    let (ox, oy) = ((ox * q).round() / q, (oy * q).round() / q);
    let (ix, iy) = (ox.floor() as i32, oy.floor() as i32);
    let (fx, fy) = (ox - ix as f64, oy - iy as f64);
    cv.px.fill(ROOM);
    if fx == 0.0 && fy == 0.0 {
        for j in 0..sp.h {
            let a = ((j + iy) * cv.w + ix) as usize;
            cv.px[a..a + sp.w as usize].copy_from_slice(&sp.px[(j * sp.w) as usize..((j + 1) * sp.w) as usize]);
        }
        return;
    }
    let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= sp.w || y >= sp.h { ROOM } else { sp.px[(y * sp.w + x) as usize] };
    for y in 0..=sp.h {
        for x in 0..=sp.w {
            let top = mix(at(x, y), at(x - 1, y), fx);
            let bot = mix(at(x, y - 1), at(x - 1, y - 1), fx);
            cv.set(x + ix, y + iy, mix(top, bot, fy));
        }
    }
}

// ---------------- fitting pixels into character cells ----------------
#[derive(Clone, Copy, PartialEq)]
enum Blocks {
    Half,
    Quad,
    Sext,
    Oct,
}
impl Blocks {
    fn dims(self) -> (usize, usize) {
        match self {
            Blocks::Half => (1, 2),
            Blocks::Quad => (2, 2),
            Blocks::Sext => (2, 3),
            Blocks::Oct => (2, 4),
        }
    }
    fn ch(self, bits: usize) -> char {
        match self {
            Blocks::Half => [' ', '▀', '▄', '█'][bits],
            Blocks::Quad => QUAD[bits],
            Blocks::Sext => SEXT[bits],
            Blocks::Oct => OCT[bits],
        }
    }
    fn name(self) -> &'static str {
        match self {
            Blocks::Half => "half 1x2",
            Blocks::Quad => "quad 2x2",
            Blocks::Sext => "sext 2x3",
            Blocks::Oct => "oct  2x4",
        }
    }
    fn parse(s: &str) -> Blocks {
        match s {
            "half" => Blocks::Half,
            "quad" => Blocks::Quad,
            "sext" => Blocks::Sext,
            _ => Blocks::Oct,
        }
    }
}
fn dist(a: u32, b: u32) -> i32 {
    let d = |s: u32| (a >> s & 255) as i32 - (b >> s & 255) as i32;
    d(16) * d(16) + d(8) * d(8) + d(0) * d(0)
}
fn lum(a: u32) -> u32 {
    (a >> 16 & 255) * 2 + (a >> 8 & 255) * 5 + (a & 255)
}
// Each cell can show one shape in two colours. Find the two colours and the shape that
// come closest to the pixels underneath. Exact whenever a cell holds two colours or fewer.
fn fit(cv: &Img, b: Blocks, g: &mut Grid) {
    let (bw, bh) = b.dims();
    let n = bw * bh;
    let full = (1usize << n) - 1;
    for r in 0..g.h {
        for c in 0..g.w {
            let mut s = [0u32; 8];
            for j in 0..bh {
                for i in 0..bw {
                    s[j * bw + i] = cv.px[(r * bh + j) * cv.w as usize + c * bw + i];
                }
            }
            let (mut far, mut fd) = (0, 0);
            for k in 1..n {
                let d = dist(s[0], s[k]);
                if d > fd {
                    fd = d;
                    far = k;
                }
            }
            let out = &mut g.c[r * g.w + c];
            if fd == 0 {
                *out = Cell { ch: ' ', fg: 0, bg: s[0] };
                continue;
            }
            let (mut c0, mut c1, mut bits) = (s[0], s[far], 0usize);
            for _ in 0..2 {
                bits = 0;
                let mut acc = [[0u32; 3]; 2];
                let mut cnt = [0u32; 2];
                for k in 0..n {
                    let w = (dist(s[k], c1) < dist(s[k], c0)) as usize;
                    bits |= w << k;
                    cnt[w] += 1;
                    for (ch, sh) in [16u32, 8, 0].iter().enumerate() {
                        acc[w][ch] += s[k] >> sh & 255;
                    }
                }
                let mean = |w: usize, old: u32| {
                    if cnt[w] == 0 { old } else { (acc[w][0] / cnt[w]) << 16 | (acc[w][1] / cnt[w]) << 8 | acc[w][2] / cnt[w] }
                };
                c0 = mean(0, c0);
                c1 = mean(1, c1);
            }
            if lum(c0) > lum(c1) {
                std::mem::swap(&mut c0, &mut c1);
                bits = !bits & full;
            }
            *out = if bits == 0 {
                Cell { ch: ' ', fg: 0, bg: c0 }
            } else if bits == full {
                Cell { ch: ' ', fg: 0, bg: c1 }
            } else {
                Cell { ch: b.ch(bits), fg: c1, bg: c0 }
            };
        }
    }
}

// Shortcuts can be switched off one at a time to find which ones a real terminal mishandles:
// LAB_OFF is a sum of 1 erase, 2 repeat, 4 gap bridging, 8 relative cursor moves.
static OFF: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
fn off(bit: u32) -> bool {
    OFF.load(std::sync::atomic::Ordering::Relaxed) & bit != 0
}

// ---------------- bytes: only what changed, as tightly as it will go ----------------
struct Out {
    b: Vec<u8>,
    fg: Option<u32>,
    bg: Option<u32>,
    pos: Option<(usize, usize)>,
}
fn num(b: &mut Vec<u8>, mut n: usize) {
    let mut t = [0u8; 8];
    let mut i = 8;
    loop {
        i -= 1;
        t[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    b.extend_from_slice(&t[i..]);
}
fn digits(n: usize) -> usize {
    if n < 10 { 1 } else if n < 100 { 2 } else if n < 1000 { 3 } else { 4 }
}
impl Out {
    fn new() -> Out {
        Out { b: Vec::with_capacity(1 << 20), fg: None, bg: None, pos: None }
    }
    fn rgb(&mut self, c: u32) {
        num(&mut self.b, (c >> 16 & 255) as usize);
        self.b.push(b';');
        num(&mut self.b, (c >> 8 & 255) as usize);
        self.b.push(b';');
        num(&mut self.b, (c & 255) as usize);
    }
    fn style(&mut self, cell: &Cell) {
        let nf = cell.ch != ' ' && self.fg != Some(cell.fg);
        let nb = self.bg != Some(cell.bg);
        if !nf && !nb {
            return;
        }
        self.b.extend_from_slice(b"\x1b[");
        if nf {
            self.b.extend_from_slice(b"38;2;");
            self.rgb(cell.fg);
            self.fg = Some(cell.fg);
        }
        if nb {
            if nf {
                self.b.push(b';');
            }
            self.b.extend_from_slice(b"48;2;");
            self.rgb(cell.bg);
            self.bg = Some(cell.bg);
        }
        self.b.push(b'm');
    }
    fn ch(&mut self, c: char) {
        let mut t = [0u8; 4];
        self.b.extend_from_slice(c.encode_utf8(&mut t).as_bytes());
    }
    fn goto(&mut self, r: usize, c: usize) {
        if self.pos == Some((r, c)) {
            return;
        }
        let abs = 4 + digits(r + 1) + digits(c + 1);
        if let Some((pr, pc)) = self.pos {
            if r >= pr && r - pr <= 3 && !off(8) {
                let dr = r - pr;
                let horiz = if c == pc {
                    0
                } else if c == 0 {
                    1
                } else if c > pc {
                    if c - pc == 1 { 3 } else { 3 + digits(c - pc) }
                } else if pc - c <= 3 {
                    pc - c
                } else {
                    3 + digits(c + 1)
                };
                if dr + horiz < abs {
                    for _ in 0..dr {
                        self.b.push(b'\n');
                    }
                    if c == pc {
                    } else if c == 0 {
                        self.b.push(b'\r');
                    } else if c > pc {
                        self.b.extend_from_slice(b"\x1b[");
                        if c - pc > 1 {
                            num(&mut self.b, c - pc);
                        }
                        self.b.push(b'C');
                    } else if pc - c <= 3 {
                        for _ in 0..pc - c {
                            self.b.push(8);
                        }
                    } else {
                        self.b.extend_from_slice(b"\x1b[");
                        num(&mut self.b, c + 1);
                        self.b.push(b'G');
                    }
                    self.pos = Some((r, c));
                    return;
                }
            }
        }
        self.b.extend_from_slice(b"\x1b[");
        num(&mut self.b, r + 1);
        self.b.push(b';');
        num(&mut self.b, c + 1);
        self.b.push(b'H');
        self.pos = Some((r, c));
    }
}
fn encode(prev: &Grid, next: &Grid, o: &mut Out) {
    let w = next.w;
    for r in 0..next.h {
        let row = &next.c[r * w..(r + 1) * w];
        let old = &prev.c[r * w..(r + 1) * w];
        let mut c = 0;
        while c < w {
            if same(&old[c], &row[c]) {
                c += 1;
                continue;
            }
            o.goto(r, c);
            loop {
                let x = row[c];
                let mut n = 1;
                while c + n < w && same(&row[c + n], &x) && !same(&old[c + n], &row[c + n]) {
                    n += 1;
                }
                o.style(&x);
                let len = x.ch.len_utf8();
                if x.ch == ' ' && n >= 5 && !off(1) {
                    o.b.extend_from_slice(b"\x1b[");
                    num(&mut o.b, n);
                    o.b.push(b'X');
                    c += n;
                    if c < w && !same(&old[c], &row[c]) {
                        o.goto(r, c);
                        continue;
                    }
                    break;
                } else if !off(2) && n * len > len + 3 + digits(n - 1) {
                    o.ch(x.ch);
                    o.b.extend_from_slice(b"\x1b[");
                    num(&mut o.b, n - 1);
                    o.b.push(b'b');
                } else {
                    for _ in 0..n {
                        o.ch(x.ch);
                    }
                }
                c += n;
                o.pos = Some((r, c));
                if c >= w {
                    o.pos = None;
                    break;
                }
                if !same(&old[c], &row[c]) {
                    continue;
                }
                let mut g = 0;
                while c + g < w && same(&old[c + g], &row[c + g]) {
                    g += 1;
                }
                if c + g >= w || g > 3 || off(4) {
                    break;
                }
                let plain = (0..g).all(|i| (row[c + i].ch == ' ' || Some(row[c + i].fg) == o.fg) && Some(row[c + i].bg) == o.bg);
                let cost: usize = (0..g).map(|i| row[c + i].ch.len_utf8()).sum();
                if !plain || cost > 4 {
                    break;
                }
                for i in 0..g {
                    o.ch(row[c + i].ch);
                }
                c += g;
                o.pos = Some((r, c));
            }
        }
    }
}

// ---------------- a tiny reference terminal: proves the bytes paint the right cells ----------------
struct Vt {
    g: Grid,
    r: usize,
    c: usize,
    fg: u32,
    bg: u32,
    last: char,
}
impl Vt {
    fn new(w: usize, h: usize) -> Vt {
        Vt { g: Grid::new(w, h, '\0'), r: 0, c: 0, fg: 0xffffff, bg: 0, last: ' ' }
    }
    fn print(&mut self, ch: char) {
        assert!(self.c < self.g.w, "wrote past the right margin");
        self.g.c[self.r * self.g.w + self.c] = Cell { ch, fg: self.fg, bg: self.bg };
        self.c += 1;
        self.last = ch;
    }
    fn feed(&mut self, b: &[u8]) {
        let s = std::str::from_utf8(b).expect("utf8");
        let mut it = s.chars();
        while let Some(ch) = it.next() {
            match ch {
                '\x1b' => {
                    assert_eq!(it.next(), Some('['));
                    let mut p: Vec<usize> = vec![0];
                    let fin = loop {
                        let d = it.next().unwrap();
                        if d.is_ascii_digit() {
                            *p.last_mut().unwrap() = *p.last().unwrap() * 10 + d as usize - 48;
                        } else if d == ';' {
                            p.push(0);
                        } else {
                            break d;
                        }
                    };
                    let n = p[0].max(1);
                    match fin {
                        'H' => {
                            self.r = p[0].max(1) - 1;
                            self.c = p.get(1).copied().unwrap_or(1).max(1) - 1;
                        }
                        'G' => self.c = n - 1,
                        'C' => self.c += n,
                        'X' => {
                            for i in 0..n {
                                self.g.c[self.r * self.g.w + self.c + i] = Cell { ch: ' ', fg: 0, bg: self.bg };
                            }
                        }
                        'b' => {
                            for _ in 0..n {
                                self.print(self.last);
                            }
                        }
                        'm' => {
                            let mut i = 0;
                            while i < p.len() {
                                let v = ((p[i + 2] as u32) << 16) | ((p[i + 3] as u32) << 8) | p[i + 4] as u32;
                                if p[i] == 38 { self.fg = v } else { self.bg = v }
                                i += 5;
                            }
                        }
                        _ => panic!("unknown CSI {}", fin),
                    }
                }
                '\n' => self.r += 1,
                '\r' => self.c = 0,
                '\x08' => self.c -= 1,
                _ => self.print(ch),
            }
        }
    }
}

// ---------------- one frame, start to finish ----------------
struct Stage {
    cols: usize,
    rows: usize,
    blocks: Blocks,
    sub: u32,
    speed: f64,
    sprite: Img,
    still: bool,
    canvas: Img,
    next: Grid,
    prev: Grid,
    out: Out,
    t_draw: Duration,
    t_fit: Duration,
    t_enc: Duration,
}
impl Stage {
    fn new(cols: usize, rows: usize, blocks: Blocks, sub: u32, speed: f64) -> Stage {
        let (bw, bh) = blocks.dims();
        let (cw, ch) = (cols * bw, rows * bh);
        let (mut sprite, mut still) = (Img::new(cw * 9 / 10, ch * 9 / 10), false);
        if let Ok(path) = std::env::var("LAB_IMAGE") {
            // pixels are bw wide by bh tall inside a 1:2 cell, so correct the shape
            let aspect = (2.0 / bh as f64) / (1.0 / bw as f64);
            sprite = picture(&path, cw * 8 / 10, ch * 8 / 10, aspect);
            still = true;
        }
        Stage {
            cols,
            rows,
            blocks,
            sub,
            speed,
            sprite,
            still,
            canvas: Img::new(cw, ch),
            next: Grid::new(cols, rows, ' '),
            prev: Grid::new(cols, rows, '\0'),
            out: Out::new(),
            t_draw: Duration::ZERO,
            t_fit: Duration::ZERO,
            t_enc: Duration::ZERO,
        }
    }
    // returns the bytes for the frame at time t (seconds)
    fn frame(&mut self, t: f64) -> &[u8] {
        let a = Instant::now();
        if !self.still {
            computer(&mut self.sprite, t);
        }
        compose(&mut self.canvas, &self.sprite, t, self.speed, self.sub);
        let b = Instant::now();
        fit(&self.canvas, self.blocks, &mut self.next);
        let c = Instant::now();
        self.out.b.clear();
        self.out.pos = None;
        encode(&self.prev, &self.next, &mut self.out);
        self.prev.c.copy_from_slice(&self.next.c);
        self.t_draw += b - a;
        self.t_fit += c - b;
        self.t_enc += c.elapsed();
        &self.out.b
    }
}

fn bench(cols: usize, rows: usize, speed: f64) {
    println!("\nbouncing computer, {cols}x{rows} characters, 600 frames at 60/s, drifting {speed} pixels a second");
    println!("{:<12}{:>6}{:>11}{:>13}{:>10}{:>10}{:>9}{:>9}{:>9}", "blocks", "blend", "pixels", "avg B/frame", "worst B", "kB/s", "draw us", "fit us", "enc us");
    for blocks in [Blocks::Half, Blocks::Quad, Blocks::Sext, Blocks::Oct] {
        for sub in [1u32, 4] {
            let mut st = Stage::new(cols, rows, blocks, sub, speed);
            let mut vt = Vt::new(cols, rows);
            let (mut total, mut worst) = (0usize, 0usize);
            let n = 600;
            for f in 0..=n {
                let bytes = st.frame(f as f64 / 60.0).to_vec();
                vt.feed(&bytes);
                for i in 0..cols * rows {
                    assert!(same(&vt.g.c[i], &st.next.c[i]), "frame {f} cell {i}: {:?} vs {:?}", vt.g.c[i], st.next.c[i]);
                }
                if f > 0 {
                    total += bytes.len();
                    worst = worst.max(bytes.len());
                }
            }
            let us = |d: Duration| d.as_secs_f64() * 1e6 / (n + 1) as f64;
            println!(
                "{:<12}{:>6}{:>11}{:>13.0}{:>10}{:>10.1}{:>9.0}{:>9.0}{:>9.0}",
                blocks.name(),
                if sub > 1 { "yes" } else { "no" },
                format!("{}x{}", st.canvas.w, st.canvas.h),
                total as f64 / n as f64,
                worst,
                total as f64 / n as f64 * 60.0 / 1000.0,
                us(st.t_draw),
                us(st.t_fit),
                us(st.t_enc)
            );
        }
    }
}

// What a terminal would show for this grid, as a picture (PPM), 8x16 pixels per character.
fn shot(cols: usize, rows: usize, blocks: Blocks, sub: u32, speed: f64, t: f64, path: &str) {
    let mut st = Stage::new(cols, rows, blocks, sub, speed);
    st.frame(t);
    let (bw, bh) = blocks.dims();
    let (pw, ph) = (cols * 8, rows * 16);
    let mut out = format!("P6\n{pw} {ph}\n255\n").into_bytes();
    let table: Vec<char> = (0..1usize << (bw * bh)).map(|b| blocks.ch(b)).collect();
    for y in 0..ph {
        for x in 0..pw {
            let cell = &st.next.c[(y / 16) * cols + x / 8];
            let k = (y % 16) * bh / 16 * bw + (x % 8) * bw / 8;
            let bits = if cell.ch == ' ' { 0 } else { table.iter().position(|c| *c == cell.ch).unwrap() };
            let c = if bits >> k & 1 == 1 { cell.fg } else { cell.bg };
            out.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
        }
    }
    std::fs::write(path, out).unwrap();
}

// ---------------- live, in a real terminal ----------------
fn stty(tty: &File, args: &[&str]) -> String {
    let out = Command::new("stty").args(args).stdin(tty.try_clone().unwrap()).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
fn read_until(tty: &mut File, end: u8, max: Duration) -> Vec<u8> {
    let mut v = Vec::new();
    let t = Instant::now();
    let mut b = [0u8; 256];
    while t.elapsed() < max {
        let n = tty.read(&mut b).unwrap_or(0);
        v.extend_from_slice(&b[..n]);
        if v.contains(&end) {
            break;
        }
    }
    v
}
fn run(blocks: Blocks, sub: u32, speed: f64, seconds: f64, report: Option<String>) {
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty").expect("needs a terminal");
    let saved = stty(&tty, &["-g"]);
    std::thread::sleep(Duration::from_millis(800)); // let a freshly opened window settle
    let size = stty(&tty, &["size"]);
    let mut it = size.split_whitespace().map(|x| x.parse::<usize>().unwrap());
    let (rows, cols) = (it.next().unwrap(), it.next().unwrap());
    stty(&tty, &["raw", "-echo", "min", "0", "time", "1"]);
    let mut rep = String::new();
    let mut go = || -> std::io::Result<()> {
        tty.write_all(b"\x1b[?1049h\x1b[?25l\x1b[2J\x1b[?2026$p\x1b[>q\x1b[c")?;
        let caps = String::from_utf8_lossy(&read_until(&mut tty, b'c', Duration::from_secs(2))).replace('\x1b', "ESC");
        let sync = caps.contains("?2026;1$y") || caps.contains("?2026;2$y");
        // do the block characters we want each take exactly one cell here?
        let probe: String = [blocks.ch(1), blocks.ch(2)].iter().collect();
        tty.write_all(format!("\x1b[1;1H{probe}\x1b[6n").as_bytes())?;
        let width_ok = String::from_utf8_lossy(&read_until(&mut tty, b'R', Duration::from_secs(1))).contains(";3R");
        rep += &format!("terminal {cols}x{rows}; replies {caps}; atomic frames {sync}; block width ok {width_ok}\n");
        let mut st = Stage::new(cols, rows, blocks, sub, speed);
        let (mut bytes, mut frames, mut missed, mut acks) = (0usize, 0u32, 0u32, true);
        let mut lat = Vec::new();
        let start = Instant::now();
        let mut buf = Vec::new();
        'outer: loop {
            let t = start.elapsed().as_secs_f64();
            if t >= seconds {
                break;
            }
            buf.clear();
            if sync {
                buf.extend_from_slice(b"\x1b[?2026h");
            }
            buf.extend_from_slice(st.frame(t));
            if sync {
                buf.extend_from_slice(b"\x1b[?2026l");
            }
            if acks {
                buf.extend_from_slice(b"\x1b[6n");
            }
            let sent = Instant::now();
            tty.write_all(&buf)?;
            bytes += buf.len();
            frames += 1;
            let got = if acks {
                let g = read_until(&mut tty, b'R', Duration::from_millis(300));
                if g.contains(&b'R') {
                    lat.push(sent.elapsed().as_secs_f64() * 1e3);
                    missed = 0;
                } else {
                    missed += 1;
                    acks = missed < 4;
                }
                g
            } else {
                let mut b = [0u8; 64];
                let n = tty.read(&mut b).unwrap_or(0);
                b[..n].to_vec()
            };
            if got.iter().any(|c| *c == b'q' || *c == 3) {
                break 'outer;
            }
            // hold an even 60 a second
            let due = Duration::from_secs_f64(frames as f64 / 60.0);
            if let Some(wait) = due.checked_sub(start.elapsed()) {
                std::thread::sleep(wait);
            }
        }
        let secs = start.elapsed().as_secs_f64();
        lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let us = |d: Duration| d.as_secs_f64() * 1e6 / frames.max(1) as f64;
        rep += &format!(
            "{} blocks, blend {}, canvas {}x{} pixels, speed {} px/s\n{} frames in {:.1}s = {:.1} frames/s; {:.1} kB/s; {:.0} B/frame\nper frame: draw {:.0} us, fit {:.0} us, encode {:.0} us\n",
            blocks.name(), sub, st.canvas.w, st.canvas.h, speed, frames, secs, frames as f64 / secs, bytes as f64 / secs / 1e3, bytes as f64 / frames.max(1) as f64, us(st.t_draw), us(st.t_fit), us(st.t_enc)
        );
        if !lat.is_empty() {
            rep += &format!("terminal answered in: median {:.2} ms, 99th {:.2} ms, worst {:.2} ms; answers still arriving at the end: {}\n", lat[lat.len() / 2], lat[lat.len() * 99 / 100], lat[lat.len() - 1], acks);
        } else {
            rep += "terminal never answered; ran on a fixed beat\n";
        }
        let _ = (st.cols, st.rows);
        Ok(())
    };
    let res = go();
    let _ = tty.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
    stty(&tty, &[&saved]);
    if let Err(e) = res {
        rep += &format!("error {e}\n");
    }
    if let Some(p) = report {
        std::fs::write(p, &rep).unwrap();
    }
    println!("{rep}");
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opt = |k: &str, d: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned().unwrap_or(d.to_string());
    OFF.store(opt("--off", "2").parse().unwrap(), std::sync::atomic::Ordering::Relaxed);
    if let Some(i) = a.iter().position(|x| x == "--image") {
        std::env::set_var("LAB_IMAGE", &a[i + 1]);
    }
    let blocks = Blocks::parse(&opt("--blocks", "oct"));
    let sub: u32 = if a.iter().any(|x| x == "--blend") { 4 } else { 1 };
    let speed: f64 = opt("--speed", "10").parse().unwrap();
    match a.get(1).map(|s| s.as_str()) {
        Some("bench") => bench(opt("--cols", "180").parse().unwrap(), opt("--rows", "49").parse().unwrap(), speed),
        Some("shot") => shot(opt("--cols", "180").parse().unwrap(), opt("--rows", "49").parse().unwrap(), blocks, sub, speed, opt("--at", "7.3").parse().unwrap(), &opt("--out", "shot.ppm")),
        _ => run(blocks, sub, speed, opt("--seconds", "3600").parse().unwrap(), a.iter().position(|x| x == "--report").and_then(|i| a.get(i + 1)).cloned()),
    }
}
