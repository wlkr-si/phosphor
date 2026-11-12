//! Bench: the pretend run. Same input, same bytes, on any machine.
//! It can say "these bytes are consistent with this profile". It can never say "works";
//! only the witness can.
//!
//! The strict terminal is deliberately narrower than any real terminal: it understands a
//! short list of commands and refuses everything else, so an encoder that leans on an
//! unproved behaviour is caught here instead of on a screen.
use phosphor_kernel::{Cell, ColourDepth, Grid, Profile, Rgb};

/// Something the bytes did that the profile has not proved safe, or that is malformed.
/// When a capability is missing the message names it (`relative_moves`, `erase`,
/// `repeat_blocks`, `atomic_frames`); when a colour is too deep it names the depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation(pub String);

fn refuse<T>(why: impl Into<String>) -> Result<T, Violation> {
    Err(Violation(why.into()))
}

/// Where the byte parser is between bytes.
#[derive(Debug, Clone)]
enum State {
    Ground,
    /// Collecting a multi-byte UTF-8 character: the bytes so far, and how many it needs.
    Utf8(Vec<u8>, usize),
    /// Seen ESC.
    Esc,
    /// Inside `ESC [`: the parameter bytes so far.
    Csi(Vec<u8>),
}

/// A terminal that refuses anything the profile does not allow.
pub struct StrictTerminal {
    profile: Profile,
    grid: Grid,
    row: usize,
    col: usize,
    /// Set after printing in the last column. Only `H` may move a parked cursor.
    parked: bool,
    ink: Rgb,
    paper: Rgb,
    /// The last character printed, for repeat.
    last: Option<char>,
    /// Between `?2026h` and `?2026l`.
    in_sync: bool,
    state: State,
    /// Once a violation happens the terminal stays broken.
    failed: Option<Violation>,
}

const DEFAULT_INK: Rgb = 0xffffff;
const DEFAULT_PAPER: Rgb = 0x000000;

/// The 256-colour table as 0xRRGGBB. Indices 0 to 7 are the eight plain colours (channels of
/// 0 or 255); 8 to 15 follow xterm's bright row; 16 to 231 are a 6 x 6 x 6 cube with levels
/// 0, 95, 135, 175, 215, 255; 232 to 255 are greys at 8 + 10n.
fn indexed(n: u8) -> Rgb {
    const LEVELS: [u32; 6] = [0, 95, 135, 175, 215, 255];
    match n {
        0..=7 => plain(n),
        8 => 0x7f7f7f,
        9 => 0xff0000,
        10 => 0x00ff00,
        11 => 0xffff00,
        12 => 0x5c5cff,
        13 => 0xff00ff,
        14 => 0x00ffff,
        15 => 0xffffff,
        16..=231 => {
            let i = (n - 16) as usize;
            LEVELS[i / 36] << 16 | LEVELS[i / 6 % 6] << 8 | LEVELS[i % 6]
        }
        232..=255 => {
            let g = 8 + 10 * (n as u32 - 232);
            g << 16 | g << 8 | g
        }
    }
}

/// Black, red, green, yellow, blue, magenta, cyan, white.
fn plain(n: u8) -> Rgb {
    let bit = |b: u8| if n & b != 0 { 255u32 } else { 0 };
    bit(1) << 16 | bit(2) << 8 | bit(4)
}

impl StrictTerminal {
    pub fn new(cols: usize, rows: usize, profile: &Profile) -> StrictTerminal {
        StrictTerminal {
            profile: profile.clone(),
            grid: Grid::new(cols, rows),
            row: 0,
            col: 0,
            parked: false,
            ink: DEFAULT_INK,
            paper: DEFAULT_PAPER,
            last: None,
            in_sync: false,
            state: State::Ground,
            failed: None,
        }
    }

    /// Apply bytes. Fails on the first command the profile forbids.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), Violation> {
        for &b in bytes {
            if let Some(v) = &self.failed {
                return Err(v.clone());
            }
            if let Err(v) = self.step(b) {
                self.failed = Some(v.clone());
                return Err(v);
            }
        }
        match &self.failed {
            Some(v) => Err(v.clone()),
            None => Ok(()),
        }
    }

    /// What the terminal is showing now.
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    // ---- bytes ----

    fn step(&mut self, b: u8) -> Result<(), Violation> {
        match std::mem::replace(&mut self.state, State::Ground) {
            State::Ground => match b {
                0x1b => {
                    self.state = State::Esc;
                    Ok(())
                }
                0x0a => self.line_feed(),
                0x0d => self.carriage_return(),
                0x08 => self.backspace(),
                0x00..=0x1f | 0x7f => refuse(format!("control byte 0x{b:02X} is not understood")),
                0x20..=0x7e => self.print(b as char),
                0xc2..=0xf4 => {
                    let need = match b {
                        0xc2..=0xdf => 2,
                        0xe0..=0xef => 3,
                        _ => 4,
                    };
                    self.state = State::Utf8(vec![b], need);
                    Ok(())
                }
                _ => refuse(format!("byte 0x{b:02X} is not valid UTF-8")),
            },
            State::Utf8(mut buf, need) => {
                if !(0x80..=0xbf).contains(&b) {
                    return refuse(format!("byte 0x{b:02X} breaks a UTF-8 character"));
                }
                buf.push(b);
                if buf.len() < need {
                    self.state = State::Utf8(buf, need);
                    return Ok(());
                }
                match std::str::from_utf8(&buf) {
                    Ok(s) => {
                        let ch = s.chars().next().expect("one character");
                        if ('\u{80}'..='\u{9f}').contains(&ch) {
                            return refuse(format!("control character U+{:04X}", ch as u32));
                        }
                        self.print(ch)
                    }
                    Err(_) => refuse("malformed UTF-8"),
                }
            }
            State::Esc => {
                if b == b'[' {
                    self.state = State::Csi(Vec::new());
                    Ok(())
                } else {
                    refuse(format!("ESC followed by 0x{b:02X} is not understood"))
                }
            }
            State::Csi(mut params) => match b {
                0x30..=0x3f => {
                    if params.len() >= 64 {
                        return refuse("control sequence is too long");
                    }
                    params.push(b);
                    self.state = State::Csi(params);
                    Ok(())
                }
                0x40..=0x7e => self.csi(&params, b),
                _ => refuse(format!("byte 0x{b:02X} inside a control sequence")),
            },
        }
    }

    /// True when the bytes fed so far end inside a character or a control sequence.
    fn mid_sequence(&self) -> bool {
        !matches!(self.state, State::Ground)
    }

    // ---- commands ----

    fn need_relative(&self, what: &str) -> Result<(), Violation> {
        if self.profile.can.relative_moves {
            Ok(())
        } else {
            refuse(format!(
                "relative_moves: {what} is a relative move and the profile has not proved it"
            ))
        }
    }

    fn not_parked(&self, what: &str) -> Result<(), Violation> {
        if self.parked {
            refuse(format!(
                "{what} with the cursor parked after the last column; only an absolute move (H) may follow"
            ))
        } else {
            Ok(())
        }
    }

    fn line_feed(&mut self) -> Result<(), Violation> {
        self.need_relative("line feed")?;
        self.not_parked("line feed")?;
        if self.row + 1 >= self.grid.rows {
            return refuse("line feed on the last row would scroll");
        }
        self.row += 1;
        Ok(())
    }

    fn carriage_return(&mut self) -> Result<(), Violation> {
        self.need_relative("carriage return")?;
        self.not_parked("carriage return")?;
        self.col = 0;
        Ok(())
    }

    fn backspace(&mut self) -> Result<(), Violation> {
        self.need_relative("backspace")?;
        self.not_parked("backspace")?;
        if self.col == 0 {
            return refuse("backspace in the first column");
        }
        self.col -= 1;
        Ok(())
    }

    fn print(&mut self, ch: char) -> Result<(), Violation> {
        if self.parked {
            return refuse(format!(
                "printing {ch:?} past the last column (cursor is parked)"
            ));
        }
        if self.col >= self.grid.cols {
            return refuse(format!("printing {ch:?} past the last column"));
        }
        let cell = Cell {
            ch,
            ink: self.ink,
            paper: self.paper,
        };
        self.grid.set(self.col, self.row, cell);
        self.last = Some(ch);
        if self.col + 1 == self.grid.cols {
            self.parked = true;
        } else {
            self.col += 1;
        }
        Ok(())
    }

    fn csi(&mut self, raw: &[u8], fin: u8) -> Result<(), Violation> {
        if raw.first() == Some(&b'?') {
            return self.private_mode(&raw[1..], fin);
        }
        let params = parse_params(raw)?;
        let name = fin as char;
        match fin {
            b'H' => {
                if params.len() > 2 {
                    return refuse("H takes at most two parameters");
                }
                let at = |i: usize| nonzero(&params, i, "H");
                let (r, c) = (at(0)?, at(1)?);
                if r > self.grid.rows || c > self.grid.cols {
                    return refuse(format!("H moves to row {r}, column {c}, outside the grid"));
                }
                self.row = r - 1;
                self.col = c - 1;
                self.parked = false;
                Ok(())
            }
            b'G' => {
                self.need_relative("G (column move)")?;
                let n = one_count(&params, name)?;
                self.not_parked("G")?;
                if n > self.grid.cols {
                    return refuse(format!("G moves to column {n}, outside the grid"));
                }
                self.col = n - 1;
                Ok(())
            }
            b'C' => {
                self.need_relative("C (cursor forward)")?;
                let n = one_count(&params, name)?;
                self.not_parked("C")?;
                if self.col + n >= self.grid.cols {
                    return refuse(format!("C by {n} moves past the last column"));
                }
                self.col += n;
                Ok(())
            }
            b'X' => {
                if !self.profile.can.erase {
                    return refuse(
                        "erase: X (erase characters) has not been proved by the profile",
                    );
                }
                let n = one_count(&params, name)?;
                self.not_parked("X")?;
                if self.col + n > self.grid.cols {
                    return refuse(format!("X erases {n} cells past the last column"));
                }
                let blank = Cell {
                    ch: ' ',
                    ink: 0,
                    paper: self.paper,
                };
                for i in 0..n {
                    self.grid.set(self.col + i, self.row, blank);
                }
                Ok(())
            }
            b'b' => {
                if !self.profile.can.repeat_blocks {
                    return refuse(
                        "repeat_blocks: b (repeat character) has not been proved by the profile",
                    );
                }
                let n = one_count(&params, name)?;
                let Some(ch) = self.last else {
                    return refuse("b with no character printed yet");
                };
                for _ in 0..n {
                    self.print(ch)?;
                }
                Ok(())
            }
            b'm' => self.sgr(&params),
            _ => refuse(format!(
                "control sequence ending in {name:?} is not understood"
            )),
        }
    }

    fn private_mode(&mut self, raw: &[u8], fin: u8) -> Result<(), Violation> {
        if raw != b"2026" || !matches!(fin, b'h' | b'l') {
            return refuse("private mode sequence is not understood");
        }
        if !self.profile.can.atomic_frames {
            return refuse(
                "atomic_frames: ?2026 (synchronised update) has not been proved by the profile",
            );
        }
        match (fin, self.in_sync) {
            (b'h', false) => self.in_sync = true,
            (b'l', true) => self.in_sync = false,
            (b'h', true) => return refuse("?2026h while a frame is already open"),
            _ => return refuse("?2026l with no frame open"),
        }
        Ok(())
    }

    fn sgr(&mut self, params: &[Option<u32>]) -> Result<(), Violation> {
        let depth = self.profile.colour;
        if params.len() == 1 && params[0].is_none() {
            self.ink = DEFAULT_INK;
            self.paper = DEFAULT_PAPER;
            return Ok(());
        }
        let mut i = 0;
        while i < params.len() {
            let Some(p) = params[i] else {
                return refuse("empty parameter in a colour instruction");
            };
            i += 1;
            match p {
                0 => {
                    self.ink = DEFAULT_INK;
                    self.paper = DEFAULT_PAPER;
                }
                30..=37 => self.ink = plain((p - 30) as u8),
                40..=47 => self.paper = plain((p - 40) as u8),
                38 | 48 => {
                    let mut take = |what: &str| -> Result<u8, Violation> {
                        match params.get(i) {
                            Some(Some(v)) if *v <= 255 => {
                                i += 1;
                                Ok(*v as u8)
                            }
                            _ => refuse(format!("{p} instruction has a missing or bad {what}")),
                        }
                    };
                    let colour = match take("colour form")? {
                        2 => {
                            if depth != ColourDepth::Full {
                                return refuse(format!(
                                    "{p};2 is a full colour form but the profile allows only {depth:?}"
                                ));
                            }
                            let r = take("red")?;
                            let g = take("green")?;
                            let b = take("blue")?;
                            (r as u32) << 16 | (g as u32) << 8 | b as u32
                        }
                        5 => {
                            if depth == ColourDepth::Indexed8 {
                                return refuse(format!(
                                    "{p};5 is a 256-colour form but the profile allows only {depth:?}"
                                ));
                            }
                            indexed(take("index")?)
                        }
                        f => return refuse(format!("{p};{f} is not a colour form")),
                    };
                    if p == 38 {
                        self.ink = colour;
                    } else {
                        self.paper = colour;
                    }
                }
                _ => return refuse(format!("colour instruction {p} is not understood")),
            }
        }
        Ok(())
    }
}

/// Split `1;;3` into `[Some(1), None, Some(3)]`. Only digits and `;` are understood.
fn parse_params(raw: &[u8]) -> Result<Vec<Option<u32>>, Violation> {
    let mut out = Vec::new();
    for part in raw.split(|b| *b == b';') {
        if part.is_empty() {
            out.push(None);
            continue;
        }
        let mut v: u32 = 0;
        for &d in part {
            if !d.is_ascii_digit() {
                return refuse(format!("parameter byte 0x{d:02X} is not understood"));
            }
            v = v * 10 + (d - b'0') as u32;
            if v > 65_535 {
                return refuse("parameter is too large");
            }
        }
        out.push(Some(v));
    }
    Ok(out)
}

/// Parameter `i`, one-based, with a missing one meaning 1. An explicit 0 is refused: real
/// terminals disagree about it.
fn nonzero(params: &[Option<u32>], i: usize, name: &str) -> Result<usize, Violation> {
    match params.get(i).copied().flatten() {
        None => Ok(1),
        Some(0) => refuse(format!("{name} with an explicit 0 parameter")),
        Some(v) => Ok(v as usize),
    }
}

/// The single count of `G`, `C`, `X` and `b`.
fn one_count(params: &[Option<u32>], name: char) -> Result<usize, Violation> {
    if params.len() > 1 {
        return refuse(format!("{name} takes one parameter"));
    }
    nonzero(params, 0, &name.to_string())
}

/// Feed whole frames to a fresh strict terminal and return the grid left behind.
/// Each frame must end outside a control sequence and outside an open `?2026` frame.
pub fn replay(
    profile: &Profile,
    cols: usize,
    rows: usize,
    frames: &[Vec<u8>],
) -> Result<Grid, Violation> {
    let mut t = StrictTerminal::new(cols, rows, profile);
    for (n, frame) in frames.iter().enumerate() {
        let tag = |v: Violation| Violation(format!("frame {n}: {}", v.0));
        t.feed(frame).map_err(tag)?;
        if t.mid_sequence() {
            return Err(tag(Violation("frame ends inside a sequence".into())));
        }
        if t.in_sync {
            return Err(tag(Violation("frame ends with ?2026 still open".into())));
        }
    }
    Ok(t.grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_can() -> Profile {
        let mut p = Profile::cautious();
        p.colour = ColourDepth::Full;
        p.can.repeat_blocks = true;
        p.can.erase = true;
        p.can.relative_moves = true;
        p.can.atomic_frames = true;
        p
    }

    fn none_can(colour: ColourDepth) -> Profile {
        let mut p = Profile::cautious();
        p.colour = colour;
        p
    }

    fn run(p: &Profile, cols: usize, rows: usize, bytes: &[u8]) -> StrictTerminal {
        let mut t = StrictTerminal::new(cols, rows, p);
        t.feed(bytes).expect("accepted");
        t
    }

    fn refused(p: &Profile, cols: usize, rows: usize, bytes: &[u8]) -> String {
        let mut t = StrictTerminal::new(cols, rows, p);
        t.feed(bytes).expect_err("refused").0
    }

    fn ch(t: &StrictTerminal, c: usize, r: usize) -> char {
        t.grid().get(c, r).ch
    }

    // ---- one test per command ----

    #[test]
    fn prints_utf8_with_the_current_colours() {
        let t = run(
            &all_can(),
            5,
            2,
            "\x1b[38;2;1;2;3m\x1b[48;2;4;5;6m▀é".as_bytes(),
        );
        assert_eq!(
            t.grid().get(0, 0),
            Cell {
                ch: '▀',
                ink: 0x010203,
                paper: 0x040506
            }
        );
        assert_eq!(ch(&t, 1, 0), 'é');
        assert_eq!(ch(&t, 2, 0), ' ');
    }

    #[test]
    fn h_moves_absolutely() {
        let t = run(&all_can(), 5, 3, b"\x1b[3;4Hx\x1b[Hy\x1b[2Hz");
        assert_eq!(ch(&t, 3, 2), 'x');
        assert_eq!(ch(&t, 0, 0), 'y');
        assert_eq!(ch(&t, 0, 1), 'z');
    }

    #[test]
    fn line_feed_goes_down_and_keeps_the_column() {
        let t = run(&all_can(), 5, 3, b"ab\nc");
        assert_eq!((ch(&t, 0, 0), ch(&t, 1, 0)), ('a', 'b'));
        assert_eq!(ch(&t, 2, 1), 'c');
    }

    #[test]
    fn carriage_return_goes_to_column_one() {
        let t = run(&all_can(), 5, 2, b"abc\rx");
        assert_eq!(ch(&t, 0, 0), 'x');
        assert_eq!(ch(&t, 1, 0), 'b');
    }

    #[test]
    fn backspace_goes_back_one() {
        let t = run(&all_can(), 5, 1, b"abc\x08x");
        assert_eq!(ch(&t, 2, 0), 'x');
        assert_eq!(ch(&t, 1, 0), 'b');
    }

    #[test]
    fn g_moves_to_a_column() {
        let t = run(&all_can(), 6, 1, b"a\x1b[5Gb\x1b[Gc");
        assert_eq!(ch(&t, 4, 0), 'b');
        assert_eq!(ch(&t, 0, 0), 'c');
    }

    #[test]
    fn c_moves_forward() {
        let t = run(&all_can(), 8, 1, b"a\x1b[3Cb\x1b[Cc");
        assert_eq!(ch(&t, 4, 0), 'b');
        assert_eq!(ch(&t, 6, 0), 'c');
    }

    #[test]
    fn x_erases_in_the_current_paper_and_leaves_the_cursor() {
        let t = run(&all_can(), 6, 1, b"abcdef\x1b[1;2H\x1b[44m\x1b[3Xz");
        let blue = 0x0000ff;
        for c in 2..=3 {
            assert_eq!(
                t.grid().get(c, 0),
                Cell {
                    ch: ' ',
                    ink: 0,
                    paper: blue
                }
            );
        }
        // The cursor stayed on column 2, so `z` landed on the first erased cell.
        assert_eq!(t.grid().get(1, 0).ch, 'z');
        assert_eq!((ch(&t, 0, 0), ch(&t, 4, 0), ch(&t, 5, 0)), ('a', 'e', 'f'));
    }

    #[test]
    fn b_repeats_the_last_character() {
        let t = run(&all_can(), 6, 1, "\x1b[31m▀\x1b[3b".as_bytes());
        for c in 0..4 {
            assert_eq!(t.grid().get(c, 0).ch, '▀');
            assert_eq!(t.grid().get(c, 0).ink, 0xff0000);
        }
        assert_eq!(ch(&t, 4, 0), ' ');
    }

    #[test]
    fn synchronised_frames_open_and_close() {
        let t = run(&all_can(), 3, 1, b"\x1b[?2026ha\x1b[?2026l");
        assert_eq!(ch(&t, 0, 0), 'a');
    }

    #[test]
    fn m_zero_resets_the_colours() {
        let t = run(&all_can(), 3, 1, b"\x1b[31;42m\x1b[0ma\x1b[31m\x1b[mb");
        assert_eq!(
            t.grid().get(0, 0),
            Cell {
                ch: 'a',
                ink: 0xffffff,
                paper: 0
            }
        );
        assert_eq!(
            t.grid().get(1, 0),
            Cell {
                ch: 'b',
                ink: 0xffffff,
                paper: 0
            }
        );
    }

    #[test]
    fn plain_colours_have_channels_of_0_or_255() {
        let t = run(
            &all_can(),
            8,
            1,
            b"\x1b[30ma\x1b[31mb\x1b[32mc\x1b[33md\x1b[34me\x1b[35mf\x1b[36mg\x1b[37mh",
        );
        let want = [
            0, 0xff0000, 0x00ff00, 0xffff00, 0x0000ff, 0xff00ff, 0x00ffff, 0xffffff,
        ];
        for (c, w) in want.iter().enumerate() {
            assert_eq!(t.grid().get(c, 0).ink, *w);
        }
        let t = run(&all_can(), 8, 1, b"\x1b[40m \x1b[41m \x1b[47m ");
        assert_eq!(t.grid().get(0, 0).paper, 0);
        assert_eq!(t.grid().get(1, 0).paper, 0xff0000);
        assert_eq!(t.grid().get(2, 0).paper, 0xffffff);
    }

    #[test]
    fn indexed_colours_are_stored_as_rgb() {
        let t = run(
            &all_can(),
            8,
            1,
            b"\x1b[38;5;1ma\x1b[38;5;16mb\x1b[38;5;17mc\x1b[38;5;196md\x1b[38;5;231me\x1b[38;5;232mf\x1b[38;5;255mg\x1b[48;5;59mh",
        );
        let ink = |c| t.grid().get(c, 0).ink;
        assert_eq!(ink(0), 0xff0000);
        assert_eq!(ink(1), 0x000000);
        assert_eq!(ink(2), 0x00005f);
        assert_eq!(ink(3), 0xff0000);
        assert_eq!(ink(4), 0xffffff);
        assert_eq!(ink(5), 0x080808);
        assert_eq!(ink(6), 0xeeeeee);
        assert_eq!(t.grid().get(7, 0).paper, 0x5f5f5f);
    }

    #[test]
    fn the_256_table_has_the_cube_levels() {
        assert_eq!(indexed(16 + 36 * 2 + 6 * 3 + 4), 0x87afd7);
        assert_eq!(indexed(9), 0xff0000);
        assert_eq!(indexed(15), 0xffffff);
    }

    // ---- one test per capability ----

    #[test]
    fn relative_moves_are_refused_without_the_capability() {
        let p = none_can(ColourDepth::Full);
        for (bytes, what) in [
            (&b"\n"[..], "line feed"),
            (b"\r", "carriage return"),
            (b"a\x08", "backspace"),
            (b"\x1b[2C", "C"),
            (b"\x1b[2G", "G"),
        ] {
            let why = refused(&p, 5, 3, bytes);
            assert!(why.contains("relative_moves"), "{what}: {why}");
        }
        // The same bytes are fine once the capability is proved.
        let mut q = p.clone();
        q.can.relative_moves = true;
        run(&q, 5, 3, b"a\x08\n\r\x1b[2C\x1b[2G");
    }

    #[test]
    fn erase_is_refused_without_the_capability() {
        let mut p = all_can();
        p.can.erase = false;
        assert!(refused(&p, 5, 1, b"\x1b[2X").contains("erase"));
    }

    #[test]
    fn repeat_is_refused_without_the_capability() {
        let mut p = all_can();
        p.can.repeat_blocks = false;
        assert!(refused(&p, 5, 1, b"a\x1b[2b").contains("repeat_blocks"));
    }

    #[test]
    fn atomic_frames_are_refused_without_the_capability() {
        let mut p = all_can();
        p.can.atomic_frames = false;
        assert!(refused(&p, 5, 1, b"\x1b[?2026h").contains("atomic_frames"));
        assert!(refused(&p, 5, 1, b"\x1b[?2026l").contains("atomic_frames"));
    }

    #[test]
    fn colour_forms_deeper_than_the_profile_are_refused() {
        let truecolor = b"\x1b[38;2;1;2;3m";
        let indexed = b"\x1b[48;5;100m";
        let plain = b"\x1b[31m\x1b[42m\x1b[0m";
        let full = none_can(ColourDepth::Full);
        let d256 = none_can(ColourDepth::Indexed256);
        let d8 = none_can(ColourDepth::Indexed8);
        run(&full, 2, 1, truecolor);
        run(&full, 2, 1, indexed);
        assert!(refused(&d256, 2, 1, truecolor).contains("Indexed256"));
        run(&d256, 2, 1, indexed);
        run(&d256, 2, 1, plain);
        assert!(refused(&d8, 2, 1, truecolor).contains("Indexed8"));
        assert!(refused(&d8, 2, 1, indexed).contains("Indexed8"));
        assert!(refused(&d8, 2, 1, b"\x1b[48;2;1;2;3m").contains("Indexed8"));
        run(&d8, 2, 1, plain);
    }

    // ---- edges and parking ----

    #[test]
    fn writing_past_the_right_edge_is_refused() {
        let p = all_can();
        assert!(refused(&p, 3, 2, b"abcd").contains("past the last column"));
        // Moving a cursor beyond the edge is refused too.
        assert!(refused(&p, 3, 2, b"\x1b[3C").contains("past the last column"));
        assert!(refused(&p, 3, 2, b"a\x1b[3X").contains("X"));
        assert!(refused(&p, 3, 1, b"a\x1b[3b").contains("past the last column"));
    }

    #[test]
    fn printing_in_the_last_column_is_allowed_then_the_cursor_is_parked() {
        let p = all_can();
        let t = run(&p, 3, 2, b"abc");
        assert_eq!(ch(&t, 2, 0), 'c');
        // Anything but H from a parked cursor is refused.
        for bytes in [
            &b"abc\n"[..],
            b"abc\r",
            b"abc\x08",
            b"abc\x1b[C",
            b"abc\x1b[1G",
            b"abc\x1b[X",
            b"abc\x1b[b",
            b"abcd",
        ] {
            let why = refused(&p, 3, 2, bytes);
            assert!(
                why.contains("parked") || why.contains("past the last"),
                "{why}"
            );
        }
        // H frees it, and colour changes do not move it.
        let t = run(&p, 3, 2, b"abc\x1b[31m\x1b[2;1Hd");
        assert_eq!(ch(&t, 0, 1), 'd');
    }

    #[test]
    fn moves_off_the_grid_are_refused() {
        let p = all_can();
        assert!(refused(&p, 3, 2, b"\n\n").contains("scroll"));
        assert!(refused(&p, 3, 2, b"\x08").contains("first column"));
        assert!(refused(&p, 3, 2, b"\x1b[3;1H").contains("outside"));
        assert!(refused(&p, 3, 2, b"\x1b[1;4H").contains("outside"));
        assert!(refused(&p, 3, 2, b"\x1b[4G").contains("outside"));
    }

    // ---- malformed and not understood ----

    #[test]
    fn anything_not_understood_is_refused() {
        let p = all_can();
        for bytes in [
            &b"\x07"[..],
            b"\t",
            b"\x7f",
            b"\x1b7",
            b"\x1b[2J",
            b"\x1b[K",
            b"\x1b[?25l",
            b"\x1b[?2026;1h",
            b"\x1b[>c",
            b"\x1b[1m",
            b"\x1b[39m",
            b"\x1b[38;2;1;2m",
            b"\x1b[38;2;1;2;256m",
            b"\x1b[38;5m",
            b"\x1b[38;7;1m",
            b"\x1b[31;m",
            b"\x1b[0H",
            b"\x1b[0C",
            b"\x1b[1;2;3H",
            b"\x1b[1:2m",
            b"\x1b[1 q",
            b"\x1b[99999C",
            b"\xff",
            b"\x80",
            b"\xe2\x28\xa1",
            b"\xc2\x85",
            b"\xc0\x80",
        ] {
            let mut t = StrictTerminal::new(8, 2, &p);
            assert!(t.feed(bytes).is_err(), "{bytes:?} was accepted");
        }
    }

    #[test]
    fn repeat_with_nothing_printed_is_refused() {
        assert!(refused(&all_can(), 3, 1, b"\x1b[2b").contains("no character"));
    }

    #[test]
    fn sync_must_pair_up() {
        let p = all_can();
        assert!(refused(&p, 3, 1, b"\x1b[?2026l").contains("no frame open"));
        assert!(refused(&p, 3, 1, b"\x1b[?2026h\x1b[?2026h").contains("already open"));
    }

    #[test]
    fn a_violation_is_final() {
        let mut t = StrictTerminal::new(2, 1, &all_can());
        let first = t.feed(b"\x07").unwrap_err();
        assert_eq!(t.feed(b"a").unwrap_err(), first);
    }

    #[test]
    fn sequences_may_be_split_between_feeds() {
        let mut t = StrictTerminal::new(4, 1, &all_can());
        t.feed(b"\x1b[38;2;99;").unwrap();
        t.feed(b"99;99ma\xe2\x96").unwrap();
        t.feed(b"\x80").unwrap();
        assert_eq!(t.grid().get(0, 0).ink, 0x636363);
        assert_eq!(t.grid().get(1, 0).ch, '▀');
    }

    #[test]
    fn replay_reports_the_frame_and_checks_frame_ends() {
        let p = all_can();
        let ok = replay(&p, 3, 1, &[b"a".to_vec(), b"\x1b[1;2Hb".to_vec()]).unwrap();
        assert_eq!((ok.get(0, 0).ch, ok.get(1, 0).ch), ('a', 'b'));
        let bad = replay(&p, 3, 1, &[b"a".to_vec(), b"\x07".to_vec()]).unwrap_err();
        assert!(bad.0.starts_with("frame 1:"), "{}", bad.0);
        assert!(replay(&p, 3, 1, &[b"\x1b[3".to_vec()]).is_err());
        assert!(replay(&p, 3, 1, &[b"\x1b[?2026ha".to_vec()]).is_err());
    }

    // ---- a whole picture ----

    #[test]
    fn a_hand_written_script_reproduces_a_known_4_by_2_grid() {
        // Apple's Terminal profile: relative moves and erase, no repeat, no atomic frames.
        let p = Profile::apple_terminal_2_15();
        let script = concat!(
            "\x1b[1;1H",
            "\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▀▄",
            "\x1b[0m",
            "\x1b[2X",
            "\x1b[1;4H",
            "\x1b[38;5;196m▌",
            "\x1b[2;1H",
            "\x1b[48;5;232m\x1b[3X",
            "\x1b[3C",
            "\x1b[32mG",
        )
        .as_bytes()
        .to_vec();
        // Row 0: red/blue ▀ and ▄, then two erased cells (the 0 reset made paper black), then ▌.
        // Row 1: three erased cells on grey 8, then G in green, parked.
        let g = replay(&p, 4, 2, &[script]).unwrap();
        let mut want = Grid::new(4, 2);
        want.set(
            0,
            0,
            Cell {
                ch: '▀',
                ink: 0xff0000,
                paper: 0x0000ff,
            },
        );
        want.set(
            1,
            0,
            Cell {
                ch: '▄',
                ink: 0xff0000,
                paper: 0x0000ff,
            },
        );
        want.set(
            2,
            0,
            Cell {
                ch: ' ',
                ink: 0,
                paper: 0,
            },
        );
        want.set(
            3,
            0,
            Cell {
                ch: '▌',
                ink: 0xff0000,
                paper: 0,
            },
        );
        let grey = Cell {
            ch: ' ',
            ink: 0,
            paper: 0x080808,
        };
        want.set(0, 1, grey);
        want.set(1, 1, grey);
        want.set(2, 1, grey);
        want.set(
            3,
            1,
            Cell {
                ch: 'G',
                ink: 0x00ff00,
                paper: 0x080808,
            },
        );
        assert_eq!(g, want);
    }
}
