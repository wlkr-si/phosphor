// Phosphor terminal spike: how few bytes can a live, smooth terminal picture cost?
// Throwaway measuring rig. No dependencies. `rustc -O spike.rs`.
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
struct Cell {
    ch: char,
    fg: u32,
    bg: u32,
}
const BG: u32 = 0x0b0f0c;
const DIMC: u32 = 0x1f6b3a;
const GREEN: u32 = 0x33ff66;
const AMBER: u32 = 0xffb000;
const TRACK: u32 = 0x12241a;
const BLANK: Cell = Cell { ch: ' ', fg: 0, bg: BG };

// Colours a glyph does not show are "don't care": a space has no ink, a full block no paper.
fn same(a: &Cell, b: &Cell) -> bool {
    a.ch == b.ch && (a.ch == ' ' || a.fg == b.fg) && (a.ch == '█' || a.bg == b.bg)
}

struct Grid {
    w: usize,
    h: usize,
    c: Vec<Cell>,
}
impl Grid {
    fn new(w: usize, h: usize) -> Grid {
        Grid { w, h, c: vec![BLANK; w * h] }
    }
    fn put(&mut self, r: usize, c: usize, ch: char, fg: u32, bg: u32) {
        if r < self.h && c < self.w {
            self.c[r * self.w + c] = Cell { ch, fg, bg };
        }
    }
    fn text(&mut self, r: usize, c: usize, s: &str, fg: u32) {
        for (i, ch) in s.chars().enumerate() {
            self.put(r, c + i, ch, fg, BG);
        }
    }
    // A bar whose right edge sits on an eighth of a cell.
    fn bar(&mut self, r: usize, c: usize, cells: usize, frac: f32, fg: u32) {
        let e = (frac.clamp(0.0, 1.0) * (cells * 8) as f32).round() as usize;
        const P: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
        for i in 0..cells {
            let ch = if i < e / 8 { '█' } else if i == e / 8 { P[e % 8] } else { ' ' };
            self.put(r, c + i, ch, fg, TRACK);
        }
    }
}

fn rnd(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(0x9e3779b9);
    x ^= x >> 15;
    x = x.wrapping_mul(0x85ebca6b);
    x ^= x >> 13;
    (x & 0xffff) as f32 / 65535.0
}
fn lerp(a: u32, b: u32, t: f32) -> u32 {
    let m = |s: u32| {
        let (x, y) = ((a >> s) & 255, (b >> s) & 255);
        ((x as f32 + (y as f32 - x as f32) * t) as u32) << s
    };
    m(16) | m(8) | m(0)
}

// A status board: static chrome, numbers that tick twice a second, bars that glide
// between ticks in eighth-cell steps, a history chart, and a sweep with a fading tail.
fn status(w: usize, h: usize, f: u32, level: f32) -> Grid {
    let mut g = Grid::new(w, h);
    let k = |c: u32| lerp(BG, c, level);
    for c in 0..w {
        g.put(0, c, '─', k(DIMC), BG);
        g.put(h - 1, c, '─', k(DIMC), BG);
    }
    for r in 0..h {
        g.put(r, 0, '│', k(DIMC), BG);
        g.put(r, w - 1, '│', k(DIMC), BG);
    }
    let s = f / 60;
    g.text(1, 3, "PHOSPHOR  status board", k(GREEN));
    g.text(1, w.saturating_sub(12), &format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60), k(AMBER));
    let panes = (w / 50).max(1);
    let pw = (w - 2) / panes;
    let tick = f / 30;
    let u = (f % 30) as f32 / 30.0;
    let ease = u * u * (3.0 - 2.0 * u);
    for p in 0..panes {
        let x = 2 + p * pw;
        g.text(3, x, &format!("machine-{}", p + 1), k(GREEN));
        for (i, name) in ["cpu", "mem", "disk"].iter().enumerate() {
            let id = (p * 8 + i) as u32;
            let (a, b) = (rnd(id * 7919 + tick), rnd(id * 7919 + tick + 1));
            // disk barely moves; cpu jumps around
            let span = [1.0, 0.25, 0.01][i];
            let v = 0.2 + span * 0.7 * (a + (b - a) * ease);
            g.text(5 + i, x, name, k(DIMC));
            g.bar(5 + i, x + 5, pw.saturating_sub(16), v, k(GREEN));
            let shown = 0.2 + span * 0.7 * b;
            g.text(5 + i, x + pw - 9, &format!("{:5.1}%", shown * 100.0), k(AMBER));
        }
        g.text(9, x, &format!("in {:7.1} kB/s   out {:7.1} kB/s", rnd(p as u32 + tick * 3) * 900.0, rnd(p as u32 + tick * 5) * 400.0), k(DIMC));
        // history chart: one new column per tick, everything else shifts left
        let (ch, cw) = (6usize, pw.saturating_sub(4));
        const V: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        for c in 0..cw {
            let val = rnd(p as u32 * 131 + tick + c as u32) * 0.6 + 0.4 * rnd(p as u32 * 17 + (tick + c as u32) / 8);
            let e = (val * (ch * 8) as f32) as usize;
            for r in 0..ch {
                let lvl = e.saturating_sub((ch - 1 - r) * 8).min(8);
                g.put(11 + r, x + c, V[lvl], k(GREEN), BG);
            }
        }
    }
    // filler text so the board has realistic static bulk
    for r in 19..h.saturating_sub(4) {
        let line = format!("job {:03}  build-and-test  octal8-workhorse  ok   {:4}s", r, (rnd(r as u32) * 900.0) as u32);
        g.text(r, 3, &line, k(DIMC));
    }
    // sweep: head moves an eighth of a cell per frame, tail fades over 16 cells
    if std::env::var("NOSWEEP").is_ok() {
        return g;
    }
    let tail: usize = std::env::var("TAIL").ok().and_then(|v| v.parse().ok()).unwrap_or(18);
    let row = h.saturating_sub(3);
    let span = (w - 4) * 8;
    let pos = (f as usize * 3) % span;
    let (hc, he) = (pos / 8, pos % 8);
    const P: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    for i in 0..tail {
        if hc < i {
            break;
        }
        let c = 2 + hc - i;
        if i == 0 {
            g.put(row, c, P[he], k(GREEN), BG);
        } else {
            let t = 1.0 - (i as f32 - 1.0 + he as f32 / 8.0) / 17.0;
            g.put(row, c, '█', lerp(BG, k(GREEN), t.clamp(0.0, 1.0)), BG);
        }
    }
    g
}

fn scene(name: &str, w: usize, h: usize, f: u32) -> Grid {
    match name {
        "status" => status(w, h, f, 1.0),
        // nothing but the clock changes
        "quiet" => {
            let mut g = status(w, h, 0, 1.0);
            let s = f / 60;
            g.text(1, w - 12, &format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60), AMBER);
            g
        }
        // the whole picture steps one cell diagonally every 12 frames
        "drift" => {
            let (iw, ih) = (w - 20, h - 6);
            let inner = status(iw, ih, f, 1.0);
            let st = (f / 12) as usize;
            let tri = |n: usize, m: usize| if (n / m) % 2 == 0 { n % m } else { m - n % m };
            let (ox, oy) = (tri(st, 20), tri(st, 6));
            let mut g = Grid::new(w, h);
            for r in 0..ih {
                for c in 0..iw {
                    g.c[(r + oy) * w + c + ox] = inner.c[r * iw + c];
                }
            }
            g
        }
        // every lit cell changes brightness every frame: the worst honest case
        "fade" => status(w, h, f, 0.55 + 0.45 * ((f as f32) * 0.1).sin()),
        _ => panic!("scene?"),
    }
}

// ---------- encoders ----------
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
    fn style(&mut self, cell: &Cell, careful: bool) {
        let nf = (!careful || cell.ch != ' ') && self.fg != Some(cell.fg);
        let nb = (!careful || cell.ch != '█') && self.bg != Some(cell.bg);
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
    fn cup(&mut self, r: usize, c: usize) {
        self.b.extend_from_slice(b"\x1b[");
        num(&mut self.b, r + 1);
        self.b.push(b';');
        num(&mut self.b, c + 1);
        self.b.push(b'H');
        self.pos = Some((r, c));
    }
    // cheapest way to get the cursor somewhere
    fn goto(&mut self, r: usize, c: usize) {
        if self.pos == Some((r, c)) {
            return;
        }
        let abs = 4 + digits(r + 1) + digits(c + 1);
        if let Some((pr, pc)) = self.pos {
            if r >= pr && r - pr <= 3 {
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
        self.cup(r, c);
    }
}

// A: what a naive program sends. Every cell, every colour, every frame.
fn enc_naive(next: &Grid, o: &mut Out) {
    o.b.extend_from_slice(b"\x1b[H");
    for r in 0..next.h {
        o.cup(r, 0);
        for c in 0..next.w {
            let x = &next.c[r * next.w + c];
            o.fg = None;
            o.bg = None;
            o.style(x, false);
            o.ch(x.ch);
        }
    }
    o.pos = None;
}
// B: every cell, but colours only when they change.
fn enc_full(next: &Grid, o: &mut Out) {
    for r in 0..next.h {
        o.cup(r, 0);
        for c in 0..next.w {
            let x = &next.c[r * next.w + c];
            o.style(x, false);
            o.ch(x.ch);
        }
    }
    o.pos = None;
}
// C: only changed cells, absolute cursor jump before each run.
fn enc_diff(prev: &Grid, next: &Grid, o: &mut Out) {
    let w = next.w;
    for r in 0..next.h {
        let mut c = 0;
        while c < w {
            if prev.c[r * w + c] == next.c[r * w + c] {
                c += 1;
                continue;
            }
            o.cup(r, c);
            while c < w && prev.c[r * w + c] != next.c[r * w + c] {
                let x = &next.c[r * w + c];
                o.style(x, false);
                o.ch(x.ch);
                c += 1;
            }
        }
    }
    o.pos = None;
}
// D: changed cells, cheapest cursor path, don't-care colours, repeats, erases, gap bridging.
fn enc_tight(prev: &Grid, next: &Grid, o: &mut Out) {
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
                // length of the run of identical cells starting here that still need painting
                let x = row[c];
                let mut n = 1;
                while c + n < w && row[c + n] == x && !same(&old[c + n], &row[c + n]) {
                    n += 1;
                }
                o.style(&x, true);
                let len = x.len();
                if x.ch == ' ' && n >= 5 {
                    o.b.extend_from_slice(b"\x1b[");
                    num(&mut o.b, n);
                    o.b.push(b'X');
                    c += n;
                    // erase leaves the cursor where it was
                    if c < w && !same(&old[c], &row[c]) {
                        o.goto(r, c);
                        continue;
                    }
                    break;
                } else if n * len > len + 3 + digits(n - 1) {
                    o.ch(x.ch);
                    o.b.extend_from_slice(b"\x1b[");
                    num(&mut o.b, n - 1);
                    o.b.push(b'b');
                    c += n;
                    o.pos = Some((r, c));
                } else {
                    for _ in 0..n {
                        o.ch(x.ch);
                    }
                    c += n;
                    o.pos = Some((r, c));
                }
                if c >= w {
                    o.pos = None; // cursor is parked on the margin; position is not portable
                    break;
                }
                if !same(&old[c], &row[c]) {
                    continue;
                }
                // bridge a short unchanged gap if rewriting it is cheaper than jumping it
                let mut g = 0;
                while c + g < w && same(&old[c + g], &row[c + g]) {
                    g += 1;
                }
                if c + g >= w || g > 3 {
                    break;
                }
                let plain = (0..g).all(|i| {
                    let y = &row[c + i];
                    (y.ch == ' ' || Some(y.fg) == o.fg) && (y.ch == '█' || Some(y.bg) == o.bg)
                });
                let cost: usize = (0..g).map(|i| row[c + i].len()).sum();
                if !plain || cost > 3 {
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
impl Cell {
    fn len(&self) -> usize {
        self.ch.len_utf8()
    }
}

// ---------- a tiny reference terminal, to prove the bytes paint the right picture ----------
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
        Vt { g: Grid::new(w, h), r: 0, c: 0, fg: 0xffffff, bg: 0, last: ' ' }
    }
    fn print(&mut self, ch: char) {
        assert!(self.c < self.g.w, "wrote past the right margin");
        let (r, c, fg, bg) = (self.r, self.c, self.fg, self.bg);
        self.g.put(r, c, ch, fg, bg);
        self.c += 1;
        self.last = ch;
    }
    fn feed(&mut self, b: &[u8]) {
        let s = std::str::from_utf8(b).expect("utf8");
        let mut it = s.chars().peekable();
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
                                let (r, c, bg) = (self.r, self.c + i, self.bg);
                                self.g.put(r, c, ' ', 0, bg);
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

fn bench(name: &str, w: usize, h: usize, frames: u32) {
    println!("\nscene {name} {w}x{h}, {frames} frames at 60/s");
    println!("{:<34}{:>12}{:>12}{:>12}{:>10}{:>10}", "method", "avg B/frame", "worst B", "kB/s", "us/frame", "idle %");
    for (label, kind) in [("A every cell, every colour", 0), ("B every cell, colour on change", 1), ("C changed cells, absolute jumps", 2), ("D changed cells, tight", 3)] {
        let mut prev = Grid::new(w, h);
        let mut vt = Vt::new(w, h);
        let mut o = Out::new();
        // first frame is a full paint for everyone; not counted
        let first = scene(name, w, h, 0);
        enc_full(&first, &mut o);
        vt.feed(&o.b);
        prev.c.copy_from_slice(&first.c);
        let (mut total, mut worst, mut idle, mut spent) = (0usize, 0usize, 0u32, Duration::ZERO);
        let mut sizes = Vec::new();
        for f in 1..=frames {
            let next = scene(name, w, h, f);
            o.b.clear();
            let t = Instant::now();
            match kind {
                0 => enc_naive(&next, &mut o),
                1 => enc_full(&next, &mut o),
                2 => enc_diff(&prev, &next, &mut o),
                _ => enc_tight(&prev, &next, &mut o),
            }
            spent += t.elapsed();
            vt.feed(&o.b);
            for i in 0..w * h {
                assert!(same(&vt.g.c[i], &next.c[i]), "{label}: frame {f} cell {i} wrong: {:?} vs {:?}", vt.g.c[i], next.c[i]);
            }
            total += o.b.len();
            worst = worst.max(o.b.len());
            sizes.push(o.b.len());
            if o.b.is_empty() {
                idle += 1;
            }
            prev.c.copy_from_slice(&next.c);
        }
        println!(
            "{:<34}{:>12.0}{:>12}{:>12.1}{:>10.1}{:>10.0}",
            label,
            total as f64 / frames as f64,
            worst,
            total as f64 / frames as f64 * 60.0 / 1000.0,
            spent.as_secs_f64() * 1e6 / frames as f64,
            idle as f64 * 100.0 / frames as f64
        );
    }
}

// ---------- live probe: ask a real terminal what it can do and how fast it answers ----------
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
fn probe(report: &str) {
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty").expect("needs a terminal");
    let saved = stty(&tty, &["-g"]);
    let size = stty(&tty, &["size"]);
    let mut it = size.split_whitespace().map(|x| x.parse::<usize>().unwrap());
    let (h, w) = (it.next().unwrap(), it.next().unwrap());
    stty(&tty, &["raw", "-echo", "min", "0", "time", "1"]);
    let mut rep = String::new();
    let mut run = || -> std::io::Result<()> {
        tty.write_all(b"\x1b[?1049h\x1b[?25l\x1b[2J")?;
        tty.write_all(b"\x1b[?2026$p\x1b[>q\x1b[c")?;
        let caps = read_until(&mut tty, b'c', Duration::from_secs(2));
        let caps_s = String::from_utf8_lossy(&caps).replace('\x1b', "ESC");
        let sync = caps_s.contains("?2026;1$y") || caps_s.contains("?2026;2$y");
        rep += &format!("size {w}x{h}\nreplies {caps_s}\nsynchronized_output {sync}\n");
        // does the repeat command work here? write x, repeat it 3 times, ask where the cursor is
        tty.write_all(b"\x1b[1;1Hx\x1b[3b\x1b[6n")?;
        let r = read_until(&mut tty, b'R', Duration::from_secs(1));
        rep += &format!("repeat_works {}\n", String::from_utf8_lossy(&r).contains(";5R"));
        tty.write_all(b"\x1b[1;1H\xe2\x96\x8c\xe2\x96\x8c\x1b[6n")?;
        let r = read_until(&mut tty, b'R', Duration::from_secs(1));
        rep += &format!("block_glyphs_single_width {}\n", String::from_utf8_lossy(&r).contains(";3R"));
        let mut o = Out::new();
        let mut prev = scene("status", w, h, 0);
        enc_full(&prev, &mut o);
        tty.write_all(&o.b)?;
        for (label, tight, n) in [("tight", true, 600u32), ("naive", false, 120)] {
            let mut lat = Vec::new();
            let mut bytes = 0usize;
            let mut missed = 0;
            let start = Instant::now();
            for f in 1..=n {
                let next = scene("status", w, h, f);
                o.b.clear();
                if sync {
                    o.b.extend_from_slice(b"\x1b[?2026h");
                }
                if tight { enc_tight(&prev, &next, &mut o) } else { enc_naive(&next, &mut o) }
                if sync {
                    o.b.extend_from_slice(b"\x1b[?2026l");
                }
                o.b.extend_from_slice(b"\x1b[6n");
                o.pos = None;
                let t = Instant::now();
                tty.write_all(&o.b)?;
                let got = read_until(&mut tty, b'R', Duration::from_millis(400));
                if !got.contains(&b'R') {
                    missed += 1;
                    if missed > 5 {
                        break;
                    }
                }
                lat.push(t.elapsed().as_secs_f64() * 1e3);
                bytes += o.b.len();
                prev.c.copy_from_slice(&next.c);
            }
            let secs = start.elapsed().as_secs_f64();
            let n = lat.len() as u32;
            lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
            rep += &format!(
                "{label}: missed acks {missed}; {n} frames in {secs:.2}s = {:.0} acknowledged frames/s, {:.0} B/frame, {:.2} MB/s, ack ms median {:.2} p99 {:.2} max {:.2}\n",
                n as f64 / secs,
                bytes as f64 / n as f64,
                bytes as f64 / secs / 1e6,
                lat[lat.len() / 2],
                lat[lat.len() * 99 / 100],
                lat[lat.len() - 1]
            );
        }
        Ok(())
    };
    let res = run();
    let _ = tty.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
    stty(&tty, &[&saved]);
    rep += &format!("ok {}\n", res.is_ok());
    std::fs::write(report, &rep).unwrap();
    println!("{rep}");
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(|s| s.as_str()) == Some("probe") {
        probe(&a[2]);
        return;
    }
    let (w, h) = (200, 50);
    for s in ["status", "quiet", "drift", "fade"] {
        bench(s, w, h, 600);
    }
    bench("status", 80, 24, 600);
}
