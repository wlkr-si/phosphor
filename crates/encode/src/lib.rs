//! Presentation, encoding: the bytes that turn one grid into the next on a given surface.
//!
//! `encode` writes the fewest bytes it can find for the damage, using only the shortcuts the
//! surface's profile has proved, and counts what it sent in a tally.
use phosphor_kernel::{
    colour_distance, Capabilities, Cell, ColourDepth, Grid, Profile, Rgb, Tally,
};

/// Append to `out` the bytes that change a terminal showing `prev` into one showing `next`,
/// using only what `profile` has proved works. `prev` of `None` means the screen is unknown:
/// paint everything. Returns what was sent, for pricing.
pub fn encode(prev: Option<&Grid>, next: &Grid, profile: &Profile, out: &mut Vec<u8>) -> Tally {
    let start = out.len();
    // A grid of another size tells us nothing about what is on screen.
    let prev = prev.filter(|p| p.cols == next.cols && p.rows == next.rows);
    if profile.can.atomic_frames {
        out.extend_from_slice(b"\x1b[?2026h");
    }
    let body = out.len();
    let tally = {
        let mut e = Encoder {
            out: &mut *out,
            cols: next.cols,
            can: profile.can,
            depth: profile.colour,
            fg: None,
            bg: None,
            pos: None,
            tally: Tally::default(),
        };
        e.frame(prev, next);
        e.tally
    };
    if out.len() == body {
        // Nothing to send, so send nothing at all.
        out.truncate(start);
        return Tally::default();
    }
    if profile.can.atomic_frames {
        out.extend_from_slice(b"\x1b[?2026l");
    }
    Tally {
        bytes: (out.len() - start) as u64,
        ..tally
    }
}

struct Encoder<'a> {
    out: &'a mut Vec<u8>,
    cols: usize,
    can: Capabilities,
    depth: ColourDepth,
    /// The ink and paper in force, as the codes sent (colour value or table index).
    fg: Option<u32>,
    bg: Option<u32>,
    /// Where the cursor is, if known.
    pos: Option<(usize, usize)>,
    tally: Tally,
}

/// How to get the cursor somewhere.
#[derive(Clone, Copy)]
enum Plan {
    Absolute,
    Relative { lines: usize, across: Across },
}

#[derive(Clone, Copy)]
enum Across {
    Stay,
    CarriageReturn,
    Forward(usize),
    Back(usize),
    Column,
}

impl Encoder<'_> {
    fn frame(&mut self, prev: Option<&Grid>, next: &Grid) {
        let cols = next.cols;
        for r in 0..next.rows {
            let new = &next.cells[r * cols..(r + 1) * cols];
            let old = prev.map(|p| &p.cells[r * cols..(r + 1) * cols]);
            let changed = |c: usize| old.is_none_or(|o| !o[c].looks_like(&new[c]));
            let mut row_runs = 0u64;
            let mut c = 0;
            while c < cols {
                if !changed(c) {
                    c += 1;
                    continue;
                }
                self.goto(r, c);
                let x = new[c];
                let mut n = 1;
                while c + n < cols && new[c + n].looks_like(&x) && changed(c + n) {
                    n += 1;
                }
                row_runs += self.style(&x);
                let moved = self.paint(&x, r, c, n);
                self.tally.cells += n as u64;
                c += n;
                if moved && c < cols && !changed(c) {
                    let mut gap = 0;
                    while c + gap < cols && !changed(c + gap) {
                        gap += 1;
                    }
                    // Only worth it if another changed cell follows on this row.
                    if c + gap < cols && gap <= 3 && self.bridge(&new[c..c + gap], r, c) {
                        c += gap;
                    }
                }
            }
            self.tally.runs += row_runs;
            self.tally.crowding += row_runs * row_runs;
        }
    }

    /// The code a colour is sent as at this depth.
    fn code(&self, c: Rgb) -> u32 {
        match self.depth {
            ColourDepth::Full => c,
            ColourDepth::Indexed256 => nearest_256(c) as u32,
            ColourDepth::Indexed8 => nearest_8(c) as u32,
        }
    }

    /// Send a colour instruction if the cell needs ink or paper not already in force.
    /// Returns 1 if one was sent, else 0.
    fn style(&mut self, cell: &Cell) -> u64 {
        let ink = (cell.ch != ' ')
            .then(|| self.code(cell.ink))
            .filter(|i| self.fg != Some(*i));
        let paper = Some(self.code(cell.paper)).filter(|p| self.bg != Some(*p));
        if ink.is_none() && paper.is_none() {
            return 0;
        }
        self.out.extend_from_slice(b"\x1b[");
        if let Some(i) = ink {
            self.colour(true, i);
            self.fg = Some(i);
        }
        if let Some(p) = paper {
            if ink.is_some() {
                self.out.push(b';');
            }
            self.colour(false, p);
            self.bg = Some(p);
        }
        self.out.push(b'm');
        1
    }

    fn colour(&mut self, ink: bool, code: u32) {
        match self.depth {
            ColourDepth::Full => {
                self.out
                    .extend_from_slice(if ink { b"38;2;" } else { b"48;2;" });
                push_num(self.out, code >> 16 & 255);
                self.out.push(b';');
                push_num(self.out, code >> 8 & 255);
                self.out.push(b';');
                push_num(self.out, code & 255);
            }
            ColourDepth::Indexed256 => {
                self.out
                    .extend_from_slice(if ink { b"38;5;" } else { b"48;5;" });
                push_num(self.out, code);
            }
            ColourDepth::Indexed8 => {
                self.out.push(if ink { b'3' } else { b'4' });
                push_num(self.out, code);
            }
        }
    }

    /// Paint `n` cells that look like `x` starting at (r, c), the cursor already there and the
    /// colour in force. Returns whether the cursor moved on past them.
    fn paint(&mut self, x: &Cell, r: usize, c: usize, n: usize) -> bool {
        let len = x.ch.len_utf8();
        if x.ch == ' ' && n >= 5 && self.can.erase {
            // Erasing leaves the cursor where it was.
            self.out.extend_from_slice(b"\x1b[");
            push_num(self.out, n as u32);
            self.out.push(b'X');
            return false;
        }
        if self.can.repeat_blocks && n * len > len + 3 + digits(n as u32 - 1) {
            push_char(self.out, x.ch);
            self.out.extend_from_slice(b"\x1b[");
            push_num(self.out, n as u32 - 1);
            self.out.push(b'b');
        } else {
            for _ in 0..n {
                push_char(self.out, x.ch);
            }
        }
        // After the last column the cursor's place is unknown.
        self.pos = (c + n < self.cols).then_some((r, c + n));
        true
    }

    /// Send unchanged `cells` (at (r, c) onward) instead of moving past them, if that is
    /// cheaper and needs no colour change. Returns whether it did.
    fn bridge(&mut self, cells: &[Cell], r: usize, c: usize) -> bool {
        let plain = cells.iter().all(|cell| {
            Some(self.code(cell.paper)) == self.bg
                && (cell.ch == ' ' || Some(self.code(cell.ink)) == self.fg)
        });
        if !plain {
            return false;
        }
        let cost: usize = cells.iter().map(|cell| cell.ch.len_utf8()).sum();
        if cost > self.move_cost(r, c + cells.len()) {
            return false;
        }
        for cell in cells {
            push_char(self.out, cell.ch);
        }
        self.tally.cells += cells.len() as u64;
        self.pos = Some((r, c + cells.len()));
        true
    }

    fn move_cost(&self, r: usize, c: usize) -> usize {
        self.plan(r, c).0
    }

    /// The shortest allowed way to put the cursor at (r, c), and its length in bytes.
    fn plan(&self, r: usize, c: usize) -> (usize, Plan) {
        let mut best = (
            4 + digits(r as u32 + 1) + digits(c as u32 + 1),
            Plan::Absolute,
        );
        let Some((pr, pc)) = self.pos else {
            return best;
        };
        if !self.can.relative_moves || r < pr {
            return best;
        }
        let lines = r - pr;
        let column = 3 + digits(c as u32 + 1);
        let mut across = Candidates(None);
        if c == pc {
            across.offer(0, Across::Stay);
        } else {
            if c == 0 {
                across.offer(1, Across::CarriageReturn);
            }
            if c > pc {
                let d = c - pc;
                across.offer(
                    if d == 1 { 3 } else { 3 + digits(d as u32) },
                    Across::Forward(d),
                );
            } else {
                across.offer(pc - c, Across::Back(pc - c));
            }
            across.offer(column, Across::Column);
        }
        let (cost, how) = across.best();
        if lines + cost < best.0 {
            best = (lines + cost, Plan::Relative { lines, across: how });
        }
        best
    }

    fn goto(&mut self, r: usize, c: usize) {
        if self.pos == Some((r, c)) {
            return;
        }
        match self.plan(r, c).1 {
            Plan::Absolute => {
                self.out.extend_from_slice(b"\x1b[");
                push_num(self.out, r as u32 + 1);
                self.out.push(b';');
                push_num(self.out, c as u32 + 1);
                self.out.push(b'H');
            }
            Plan::Relative { lines, across } => {
                for _ in 0..lines {
                    self.out.push(b'\n');
                }
                match across {
                    Across::Stay => {}
                    Across::CarriageReturn => self.out.push(b'\r'),
                    Across::Forward(d) => {
                        self.out.extend_from_slice(b"\x1b[");
                        if d > 1 {
                            push_num(self.out, d as u32);
                        }
                        self.out.push(b'C');
                    }
                    Across::Back(d) => {
                        for _ in 0..d {
                            self.out.push(8);
                        }
                    }
                    Across::Column => {
                        self.out.extend_from_slice(b"\x1b[");
                        push_num(self.out, c as u32 + 1);
                        self.out.push(b'G');
                    }
                }
            }
        }
        self.pos = Some((r, c));
    }
}

/// Keeps the cheapest of a few offers; the first offer wins a tie.
struct Candidates(Option<(usize, Across)>);

impl Candidates {
    fn offer(&mut self, cost: usize, how: Across) {
        if self.0.is_none_or(|(best, _)| cost < best) {
            self.0 = Some((cost, how));
        }
    }
    fn best(self) -> (usize, Across) {
        self.0.expect("at least one offer")
    }
}

fn digits(n: u32) -> usize {
    match n {
        0..=9 => 1,
        10..=99 => 2,
        100..=999 => 3,
        1000..=9999 => 4,
        _ => 5,
    }
}

fn push_num(out: &mut Vec<u8>, mut n: u32) {
    let mut t = [0u8; 10];
    let mut i = t.len();
    loop {
        i -= 1;
        t[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(&t[i..]);
}

fn push_char(out: &mut Vec<u8>, c: char) {
    let mut t = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut t).as_bytes());
}

/// The colour of entry `i` of the standard 256-colour table. Entries 0 to 7 are the eight
/// plain colours.
fn table_colour(i: u8) -> Rgb {
    const BASIC: [Rgb; 16] = [
        0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5, 0x7f7f7f,
        0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff,
    ];
    match i {
        0..=15 => BASIC[i as usize],
        16..=231 => {
            let k = i as u32 - 16;
            let level = |v: u32| if v == 0 { 0 } else { 55 + 40 * v };
            level(k / 36) << 16 | level(k / 6 % 6) << 8 | level(k % 6)
        }
        _ => {
            let g = 8 + 10 * (i as u32 - 232);
            g << 16 | g << 8 | g
        }
    }
}

/// The table entry nearest to `c`; the lowest index wins a tie.
fn nearest_256(c: Rgb) -> u8 {
    let mut best = (u32::MAX, 0u8);
    let mut offer = |i: u8| {
        let d = colour_distance(c, table_colour(i));
        if d < best.0 || (d == best.0 && i < best.1) {
            best = (d, i);
        }
    };
    (0..16).for_each(&mut offer);
    // The nearest cube entry is the nearest level in each channel.
    let near = |v: u32| (0..6u32).min_by_key(|l| level_gap(v, *l)).unwrap_or(0);
    let (r, g, b) = (c >> 16 & 255, c >> 8 & 255, c & 255);
    offer((16 + 36 * near(r) + 6 * near(g) + near(b)) as u8);
    // The nearest grey is next to the grey of the average.
    let avg = (r + g + b) / 3;
    let step = (avg.saturating_sub(8) / 10).min(23);
    for s in [step.saturating_sub(1), step, (step + 1).min(23)] {
        offer(232 + s as u8);
    }
    best.1
}

/// How far `v` is from cube level `l`, for choosing the nearest level.
fn level_gap(v: u32, l: u32) -> i32 {
    let level = if l == 0 { 0 } else { 55 + 40 * l };
    (v as i32 - level as i32).abs()
}

/// The plain colour nearest to `c`; the lowest index wins a tie.
fn nearest_8(c: Rgb) -> u8 {
    let mut best = (u32::MAX, 0u8);
    for i in 0..8 {
        let d = colour_distance(c, table_colour(i));
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // ---------- a small reference terminal: only the commands the encoder may use ----------

    const JUNK_INK: Rgb = 0x010203;
    const JUNK_PAPER: Rgb = 0x040506;

    #[derive(Default, Debug)]
    struct Seen {
        sgr: u64,
        repeat: u64,
        erase: u64,
        relative: u64,
        absolute: u64,
        atomic_on: u64,
        atomic_off: u64,
    }

    struct Term {
        g: Grid,
        r: usize,
        c: usize,
        fg: Rgb,
        bg: Rgb,
        last: char,
        seen: Seen,
    }

    impl Term {
        fn new(start: Option<&Grid>, cols: usize, rows: usize) -> Term {
            let g = match start {
                Some(p) => p.clone(),
                None => {
                    let mut g = Grid::new(cols, rows);
                    g.cells.fill(Cell {
                        ch: '?',
                        ink: 0x777777,
                        paper: 0x888888,
                    });
                    g
                }
            };
            // Cursor and colours start somewhere the encoder cannot know.
            Term {
                g,
                r: rows / 2,
                c: cols / 3,
                fg: JUNK_INK,
                bg: JUNK_PAPER,
                last: '?',
                seen: Seen::default(),
            }
        }

        fn print(&mut self, ch: char) {
            assert!(self.r < self.g.rows, "wrote below the screen");
            assert!(self.c < self.g.cols, "wrote past the right margin");
            self.g.set(
                self.c,
                self.r,
                Cell {
                    ch,
                    ink: self.fg,
                    paper: self.bg,
                },
            );
            self.c += 1;
            self.last = ch;
        }

        fn feed(&mut self, bytes: &[u8]) {
            let text = std::str::from_utf8(bytes).expect("utf8");
            let mut it = text.chars();
            while let Some(ch) = it.next() {
                match ch {
                    '\x1b' => self.csi(&mut it),
                    '\n' => {
                        self.seen.relative += 1;
                        self.r += 1;
                        assert!(self.r < self.g.rows, "line feed off the screen");
                    }
                    '\r' => {
                        self.seen.relative += 1;
                        self.c = 0;
                    }
                    '\x08' => {
                        self.seen.relative += 1;
                        assert!(self.c > 0 && self.c < self.g.cols, "backspace off the line");
                        self.c -= 1;
                    }
                    _ => self.print(ch),
                }
            }
        }

        fn csi(&mut self, it: &mut std::str::Chars) {
            assert_eq!(it.next(), Some('['));
            let mut private = false;
            let mut p: Vec<usize> = vec![0];
            let fin = loop {
                let d = it.next().expect("unfinished escape");
                if d == '?' {
                    private = true;
                } else if let Some(v) = d.to_digit(10) {
                    *p.last_mut().unwrap() = *p.last().unwrap() * 10 + v as usize;
                } else if d == ';' {
                    p.push(0);
                } else {
                    break d;
                }
            };
            let n = p[0].max(1);
            if private {
                assert_eq!(p, [2026]);
                match fin {
                    'h' => self.seen.atomic_on += 1,
                    'l' => self.seen.atomic_off += 1,
                    _ => panic!("unknown private mode"),
                }
                return;
            }
            match fin {
                'H' => {
                    self.seen.absolute += 1;
                    self.r = n - 1;
                    self.c = p.get(1).copied().unwrap_or(1).max(1) - 1;
                    assert!(
                        self.r < self.g.rows && self.c < self.g.cols,
                        "H off the screen"
                    );
                }
                'G' => {
                    self.seen.relative += 1;
                    self.c = n - 1;
                    assert!(self.c < self.g.cols, "G off the line");
                }
                'C' => {
                    self.seen.relative += 1;
                    assert!(self.c < self.g.cols, "C from a pending wrap");
                    self.c += n;
                    assert!(self.c < self.g.cols, "C off the line");
                }
                'X' => {
                    self.seen.erase += 1;
                    assert!(self.c + n <= self.g.cols, "erase past the right margin");
                    for i in 0..n {
                        self.g.set(
                            self.c + i,
                            self.r,
                            Cell {
                                ch: ' ',
                                ink: self.fg,
                                paper: self.bg,
                            },
                        );
                    }
                }
                'b' => {
                    self.seen.repeat += 1;
                    for _ in 0..n {
                        self.print(self.last);
                    }
                }
                'm' => {
                    self.seen.sgr += 1;
                    let mut i = 0;
                    while i < p.len() {
                        match p[i] {
                            38 | 48 => {
                                let (v, used) = match p[i + 1] {
                                    2 => (
                                        (p[i + 2] as u32) << 16
                                            | (p[i + 3] as u32) << 8
                                            | p[i + 4] as u32,
                                        5,
                                    ),
                                    5 => (table_colour(p[i + 2] as u8), 3),
                                    _ => panic!("bad extended colour"),
                                };
                                if p[i] == 38 {
                                    self.fg = v
                                } else {
                                    self.bg = v
                                }
                                i += used;
                            }
                            30..=37 => {
                                self.fg = table_colour(p[i] as u8 - 30);
                                i += 1;
                            }
                            40..=47 => {
                                self.bg = table_colour(p[i] as u8 - 40);
                                i += 1;
                            }
                            other => panic!("unknown colour instruction {other}"),
                        }
                    }
                }
                _ => panic!("unknown CSI {fin}"),
            }
        }
    }

    // ---------- seeded random grids ----------

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() >> 11) as usize % n
        }
        fn chance(&mut self, percent: usize) -> bool {
            self.below(100) < percent
        }
    }

    const CHARS: [char; 5] = [' ', '▀', '▄', '▌', '█'];
    const COLOURS: [Rgb; 4] = [0x000000, 0xff00ff, 0x33ff66, 0xffffff];

    fn random_cell(rng: &mut Rng, left: Option<Cell>) -> Cell {
        if let Some(l) = left {
            if rng.chance(60) {
                return l;
            }
        }
        Cell {
            ch: if rng.chance(50) {
                ' '
            } else {
                CHARS[rng.below(5)]
            },
            ink: COLOURS[rng.below(4)],
            paper: COLOURS[rng.below(4)],
        }
    }

    fn random_grid(rng: &mut Rng, cols: usize, rows: usize) -> Grid {
        let mut g = Grid::new(cols, rows);
        for r in 0..rows {
            for c in 0..cols {
                let left = (c > 0).then(|| g.get(c - 1, r));
                g.set(c, r, random_cell(rng, left));
            }
        }
        g
    }

    fn mutated(rng: &mut Rng, g: &Grid) -> Grid {
        let mut n = g.clone();
        let rate = [5, 30, 100][rng.below(3)];
        for r in 0..g.rows {
            for c in 0..g.cols {
                if rng.chance(rate) {
                    let left = (c > 0).then(|| n.get(c - 1, r));
                    n.set(c, r, random_cell(rng, left));
                }
            }
        }
        n
    }

    fn profile(relative: bool, erase: bool, repeat: bool, atomic: bool) -> Profile {
        let mut p = Profile::cautious();
        p.colour = ColourDepth::Full;
        p.can = Capabilities {
            repeat_blocks: repeat,
            erase,
            relative_moves: relative,
            atomic_frames: atomic,
        };
        p
    }

    fn capability_sets() -> [Profile; 4] {
        [
            profile(false, false, false, false),
            profile(true, false, false, false),
            profile(true, true, false, false),
            profile(true, true, true, true),
        ]
    }

    fn all_look_alike(a: &Grid, b: &Grid) -> bool {
        a.cells.iter().zip(&b.cells).all(|(x, y)| x.looks_like(y))
    }

    fn describe(g: &Grid) -> String {
        g.cells.iter().map(|c| c.ch).collect()
    }

    // ---------- the tests ----------

    #[test]
    fn bytes_turn_prev_into_next_for_200_random_pairs_under_four_capability_sets() {
        for (set, profile) in capability_sets().iter().enumerate() {
            let mut rng = Rng(0x9e3779b97f4a7c15 ^ set as u64);
            for pair in 0..200 {
                let cols = 1 + rng.below(40);
                let rows = 1 + rng.below(6);
                let first = random_grid(&mut rng, cols, rows);
                let next = if rng.chance(50) {
                    mutated(&mut rng, &first)
                } else {
                    random_grid(&mut rng, cols, rows)
                };
                let prev = (pair % 8 != 0).then_some(&first);
                let mut out = Vec::new();
                let tally = encode(prev, &next, profile, &mut out);
                let mut term = Term::new(prev, cols, rows);
                term.feed(&out);
                assert!(
                    all_look_alike(&term.g, &next),
                    "set {set} pair {pair}: {} vs {}",
                    describe(&term.g),
                    describe(&next)
                );
                assert_eq!(tally.bytes, out.len() as u64);
            }
        }
    }

    #[test]
    fn bytes_are_right_at_the_indexed_depths() {
        for depth in [ColourDepth::Indexed256, ColourDepth::Indexed8] {
            let quantise = |c: Rgb| match depth {
                ColourDepth::Indexed256 => table_colour(nearest_256(c)),
                _ => table_colour(nearest_8(c)),
            };
            let mut rng = Rng(77);
            for profile in capability_sets() {
                let mut p = profile;
                p.colour = depth;
                for pair in 0..100 {
                    let (cols, rows) = (1 + rng.below(40), 1 + rng.below(6));
                    let first = random_grid(&mut rng, cols, rows);
                    let next = mutated(&mut rng, &first);
                    let prev = (pair % 8 != 0).then_some(&first);
                    let mut out = Vec::new();
                    encode(prev, &next, &p, &mut out);
                    let mut term = Term::new(prev, cols, rows);
                    term.feed(&out);
                    for (got, want) in term.g.cells.iter().zip(&next.cells) {
                        assert_eq!(quantise(got.paper), quantise(want.paper));
                        if want.ch != ' ' {
                            assert_eq!(got.ch, want.ch);
                            assert_eq!(quantise(got.ink), quantise(want.ink));
                        } else {
                            assert_eq!(got.ch, ' ');
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn indexed_colour_uses_the_right_instructions() {
        let mut g = Grid::new(2, 1);
        g.set(
            0,
            0,
            Cell {
                ch: '▀',
                ink: 0xff0000,
                paper: 0x0000ff,
            },
        );
        g.set(
            1,
            0,
            Cell {
                ch: ' ',
                ink: 0xff0000,
                paper: 0x00ff00,
            },
        );
        let mut p = profile(false, false, false, false);
        p.colour = ColourDepth::Indexed8;
        let mut out = Vec::new();
        encode(None, &g, &p, &mut out);
        assert_eq!(out, "\x1b[1;1H\x1b[31;44m▀\x1b[42m ".as_bytes());
        p.colour = ColourDepth::Indexed256;
        let mut out = Vec::new();
        encode(None, &g, &p, &mut out);
        assert_eq!(
            out,
            "\x1b[1;1H\x1b[38;5;9;48;5;21m▀\x1b[48;5;10m ".as_bytes()
        );
    }

    #[test]
    fn nearest_256_agrees_with_looking_at_every_entry() {
        let mut rng = Rng(5);
        for _ in 0..20_000 {
            let c = (rng.next() & 0xffffff) as Rgb;
            let (d, i) = (0..=255u8)
                .map(|i| (colour_distance(c, table_colour(i)), i))
                .min()
                .unwrap();
            let got = nearest_256(c);
            assert_eq!(
                colour_distance(c, table_colour(got)),
                d,
                "{c:06x}: {got} vs {i}"
            );
        }
    }

    #[test]
    fn identical_grids_send_nothing() {
        let mut rng = Rng(3);
        let g = random_grid(&mut rng, 30, 5);
        for p in capability_sets() {
            let mut out = Vec::new();
            assert_eq!(encode(Some(&g), &g.clone(), &p, &mut out), Tally::default());
            assert!(out.is_empty());
        }
        // A space whose ink changed is still the same picture.
        let mut a = Grid::new(4, 1);
        a.set(
            1,
            0,
            Cell {
                ch: ' ',
                ink: 1,
                paper: 9,
            },
        );
        let mut b = a.clone();
        b.set(
            1,
            0,
            Cell {
                ch: ' ',
                ink: 2,
                paper: 9,
            },
        );
        let mut out = Vec::new();
        assert_eq!(
            encode(Some(&a), &b, &capability_sets()[3], &mut out),
            Tally::default()
        );
        assert!(out.is_empty());
    }

    #[test]
    fn unproven_shortcuts_are_never_used() {
        for (set, p) in capability_sets().iter().enumerate() {
            let mut rng = Rng(11 + set as u64);
            let mut seen = Seen::default();
            for _ in 0..200 {
                let (cols, rows) = (1 + rng.below(60), 1 + rng.below(5));
                let first = random_grid(&mut rng, cols, rows);
                let next = mutated(&mut rng, &first);
                let mut out = Vec::new();
                encode(Some(&first), &next, p, &mut out);
                let mut term = Term::new(Some(&first), cols, rows);
                term.feed(&out);
                seen.repeat += term.seen.repeat;
                seen.erase += term.seen.erase;
                seen.relative += term.seen.relative;
                seen.absolute += term.seen.absolute;
                seen.atomic_on += term.seen.atomic_on;
            }
            assert_eq!(seen.repeat > 0, p.can.repeat_blocks, "set {set} {seen:?}");
            assert_eq!(seen.erase > 0, p.can.erase, "set {set} {seen:?}");
            assert_eq!(
                seen.relative > 0,
                p.can.relative_moves,
                "set {set} {seen:?}"
            );
            assert_eq!(
                seen.atomic_on > 0,
                p.can.atomic_frames,
                "set {set} {seen:?}"
            );
            assert!(seen.absolute > 0);
        }
    }

    #[test]
    fn tally_counts_what_was_sent() {
        for p in capability_sets() {
            let mut rng = Rng(21);
            for _ in 0..100 {
                let (cols, rows) = (1 + rng.below(40), 1 + rng.below(6));
                let first = random_grid(&mut rng, cols, rows);
                let next = mutated(&mut rng, &first);
                let mut out = b"junk already in the buffer".to_vec();
                let before = out.len();
                let tally = encode(Some(&first), &next, &p, &mut out);
                assert_eq!(tally.bytes, (out.len() - before) as u64);
                let mut term = Term::new(Some(&first), cols, rows);
                term.feed(&out[before..]);
                assert_eq!(tally.runs, term.seen.sgr);
                assert_eq!(&out[..before], b"junk already in the buffer");
            }
        }
    }

    #[test]
    fn tally_counts_cells_runs_and_crowding() {
        let blank = Grid::new(3, 2);
        let mut next = Grid::new(3, 2);
        let paper = |p| Cell {
            ch: ' ',
            ink: 0,
            paper: p,
        };
        for (c, p) in [0x111111, 0x222222, 0x333333].into_iter().enumerate() {
            next.set(c, 0, paper(p));
            next.set(c, 1, paper(0x444444));
        }
        let mut out = Vec::new();
        let t = encode(Some(&blank), &next, &capability_sets()[0], &mut out);
        // Row 0 changes colour three times; row 1 once.
        assert_eq!((t.cells, t.runs, t.crowding), (6, 4, 9 + 1));
    }

    #[test]
    fn a_prev_of_none_paints_every_cell_even_blank_ones() {
        let next = Grid::new(6, 2);
        let mut out = Vec::new();
        let t = encode(None, &next, &capability_sets()[0], &mut out);
        assert_eq!(t.cells, 12);
        let mut term = Term::new(None, 6, 2);
        term.feed(&out);
        assert!(all_look_alike(&term.g, &next));
    }

    #[test]
    fn long_runs_of_space_are_erased_when_proved() {
        let blank = Grid::new(20, 1);
        let mut next = blank.clone();
        for c in 3..15 {
            next.set(
                c,
                0,
                Cell {
                    ch: ' ',
                    ink: 0,
                    paper: 0xff00ff,
                },
            );
        }
        let mut out = Vec::new();
        let t = encode(Some(&blank), &next, &capability_sets()[2], &mut out);
        assert_eq!(out, b"\x1b[1;4H\x1b[48;2;255;0;255m\x1b[12X");
        assert_eq!((t.cells, t.runs), (12, 1));
    }

    #[test]
    fn repeats_are_used_only_when_proved() {
        let blank = Grid::new(20, 1);
        let mut next = blank.clone();
        for c in 0..10 {
            next.set(
                c,
                0,
                Cell {
                    ch: '▀',
                    ink: 0xffffff,
                    paper: 0,
                },
            );
        }
        let mut out = Vec::new();
        encode(Some(&blank), &next, &capability_sets()[2], &mut out);
        assert!(!out.contains(&b'b'));
        let mut out = Vec::new();
        let t = encode(Some(&blank), &next, &capability_sets()[3], &mut out);
        assert!(String::from_utf8(out.clone()).unwrap().contains("▀\x1b[9b"));
        assert_eq!(t.cells, 10);
    }

    #[test]
    fn a_short_gap_is_sent_rather_than_jumped() {
        let mut prev = Grid::new(10, 1);
        let ink = Cell {
            ch: '█',
            ink: 0xffffff,
            paper: 0,
        };
        prev.cells.fill(ink);
        let mut next = prev.clone();
        let half = Cell { ch: '▀', ..ink };
        next.set(2, 0, half);
        next.set(5, 0, half);
        let mut out = Vec::new();
        let t = encode(Some(&prev), &next, &capability_sets()[0], &mut out);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b[1;3H\x1b[38;2;255;255;255;48;2;0;0;0m▀██▀"
        );
        assert_eq!((t.cells, t.runs), (4, 1));
        // A gap that needs a colour change is jumped instead.
        let mut next = prev.clone();
        next.set(2, 0, half);
        next.set(5, 0, half);
        next.set(
            3,
            0,
            Cell {
                ch: '█',
                ink: 0xff0000,
                paper: 0,
            },
        );
        next.set(
            4,
            0,
            Cell {
                ch: '█',
                ink: 0xff0000,
                paper: 0,
            },
        );
        let mut out = Vec::new();
        encode(Some(&prev), &next, &capability_sets()[0], &mut out);
        let mut term = Term::new(Some(&prev), 10, 1);
        term.feed(&out);
        assert!(all_look_alike(&term.g, &next));
    }

    #[test]
    fn relative_moves_replace_long_absolute_ones() {
        let blank = Grid::new(100, 30);
        let mut next = blank.clone();
        let dot = Cell {
            ch: '█',
            ink: 0xffffff,
            paper: 0,
        };
        next.set(50, 10, dot);
        next.set(50, 11, dot);
        let mut out = Vec::new();
        encode(Some(&blank), &next, &capability_sets()[1], &mut out);
        let s = String::from_utf8(out).unwrap();
        // After the first cell the cursor is at column 51 of row 11; the next is one line down
        // and one column back.
        assert!(s.contains("█\n\x08█"), "{s:?}");
    }

    #[test]
    fn an_atomic_frame_is_wrapped_only_when_something_is_sent() {
        let blank = Grid::new(8, 2);
        let mut next = blank.clone();
        next.set(
            1,
            1,
            Cell {
                ch: '▄',
                ink: 1,
                paper: 2,
            },
        );
        let mut out = Vec::new();
        let t = encode(Some(&blank), &next, &capability_sets()[3], &mut out);
        assert!(out.starts_with(b"\x1b[?2026h") && out.ends_with(b"\x1b[?2026l"));
        assert_eq!(t.bytes, out.len() as u64);
        let mut out = Vec::new();
        assert_eq!(
            encode(Some(&blank), &blank, &capability_sets()[3], &mut out),
            Tally::default()
        );
        assert!(out.is_empty());
    }

    #[test]
    fn writing_the_last_column_forgets_where_the_cursor_is() {
        let blank = Grid::new(4, 3);
        let mut next = blank.clone();
        let dot = Cell {
            ch: '█',
            ink: 0xffffff,
            paper: 0,
        };
        for r in 0..3 {
            next.set(3, r, dot);
            next.set(0, r, dot);
        }
        let mut out = Vec::new();
        encode(Some(&blank), &next, &capability_sets()[3], &mut out);
        let mut term = Term::new(Some(&blank), 4, 3);
        term.feed(&out);
        assert!(all_look_alike(&term.g, &next));
    }

    #[test]
    fn a_full_repaint_of_160_by_45_takes_under_5_ms() {
        let mut rng = Rng(99);
        let mut next = Grid::new(160, 45);
        for cell in next.cells.iter_mut() {
            *cell = Cell {
                ch: '▀',
                ink: (rng.next() & 0xffffff) as Rgb,
                paper: (rng.next() & 0xffffff) as Rgb,
            };
        }
        let p = Profile::apple_terminal_2_15();
        let mut best = f64::MAX;
        let mut out = Vec::new();
        for _ in 0..10 {
            out.clear();
            let t = Instant::now();
            let tally = encode(None, &next, &p, &mut out);
            best = best.min(t.elapsed().as_secs_f64() * 1e3);
            assert_eq!(tally.cells, 7200);
        }
        eprintln!("full repaint: {best:.3} ms");
        if !cfg!(debug_assertions) {
            assert!(best < 5.0, "{best} ms");
        }
    }
}
