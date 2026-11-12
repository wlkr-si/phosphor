//! Probing: questions put to a surface at start-up to fill in its profile.
//!
//! A probe can show something is broken. It can rarely show something works: a terminal once
//! answered "yes" to a repeat test done with a letter, then got block characters wrong. So the
//! probe starts from the cautious profile, takes a known surface's profile when the surface
//! names itself, and otherwise upgrades only what a reply proves. Repeat and erase stay
//! unproven; only a profile made by the witness can prove them.
//!
//! The probe writes a few characters near the top-left corner. The first frame paints over them.

use phosphor_kernel::{ColourDepth, Ladder, Profile, Wire};
use std::time::{Duration, Instant};

/// The longest the probe waits for any one answer.
const LIMIT: Duration = Duration::from_millis(150);
/// The longest the whole probe may take, however slowly the surface answers.
const TOTAL: Duration = Duration::from_millis(900);

const ESC: u8 = 0x1b;

/// The profile of a surface known by name, if there is one.
///
/// Names are matched without regard to case, and `_` or a space counts as `-`, so the
/// `TERM_PROGRAM` value `Apple_Terminal` finds `apple-terminal`.
pub fn known(name: &str) -> Option<Profile> {
    match normalise(name).as_str() {
        "apple-terminal" => Some(Profile::apple_terminal_2_15()),
        "ghostty" => Some(ghostty()),
        _ => None,
    }
}

/// Work out which surface is on the other end of `wire` and what it has proved it can do.
/// `env` reads an environment variable. When the surface cannot be told, assume the least.
pub fn probe(wire: &mut dyn Wire, env: &dyn Fn(&str) -> Option<String>) -> Profile {
    let deadline = Instant::now() + TOTAL;

    // 1. Identify by the environment.
    if let Some(p) = env("TERM_PROGRAM").and_then(|n| known(&n)) {
        return p;
    }

    // 2. Identify by asking. Every terminal worth drawing on answers `ESC [ c`; some also
    //    give their name in answer to `ESC [ > q`, which arrives first.
    let Some(reply) = ask(wire, b"\x1b[>q\x1b[c", b'c', da1_complete, deadline) else {
        // 5. No reply at all.
        return Profile::cautious();
    };
    let name = reported_name(&reply);

    // 3. A known surface.
    if let Some(p) = name.as_deref().and_then(known) {
        return p;
    }

    // 4. An unknown surface: upgrade only what a reply proves.
    let mut p = Profile::cautious();
    p.name = name.unwrap_or_else(|| "unknown".into());

    p.can.atomic_frames = ask(
        wire,
        b"\x1b[?2026$p",
        b'y',
        |b| contains(b, b"$y"),
        deadline,
    )
    .is_some_and(|r| contains(&r, b"?2026;1$y") || contains(&r, b"?2026;2$y"));

    p.can.relative_moves = relative_moves_land(wire, deadline);

    // A rung stays only if two of its characters move the cursor exactly two columns.
    // Column 3 proves the width, not the shape, so it never adds a rung.
    let rungs = std::mem::take(&mut p.ladders);
    for rung in rungs {
        let mut q = b"\x1b[1;1H".to_vec();
        q.extend_from_slice(sample(rung).as_bytes());
        if cursor_after(wire, &q, deadline) == Some((1, 3)) {
            p.ladders.push(rung);
        }
    }

    // Repeat and erase stay unproven.
    p
}

/// Ghostty: all four rungs, full colour, relative moves, erase and atomic frames. Its prices
/// have not been measured, so it pays the cautious ones.
fn ghostty() -> Profile {
    let mut p = Profile::cautious();
    p.name = "ghostty".into();
    p.ladders = vec![Ladder::Half, Ladder::Quarter, Ladder::Sixth, Ladder::Eighth];
    p.colour = ColourDepth::Full;
    p.can.relative_moves = true;
    p.can.erase = true;
    p.can.atomic_frames = true;
    p.can.repeat_blocks = false;
    // Not yet filmed: issue #50 is measuring how often Ghostty is seen to paint.
    p.paint_hz = 60.0;
    p
}

fn normalise(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| match c {
            '_' | ' ' => '-',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

/// Two characters of a rung, each meant to take one cell.
fn sample(rung: Ladder) -> &'static str {
    match rung {
        Ladder::Half => "\u{2580}\u{2584}",
        Ladder::Quarter => "\u{259A}\u{259E}",
        Ladder::Sixth => "\u{1FB00}\u{1FB01}",
        Ladder::Eighth => "\u{1CD00}\u{1CD01}",
    }
}

/// Line feed, carriage return, backspace and cursor-forward, each from row 2 column 5.
/// All four must land where expected.
fn relative_moves_land(wire: &mut dyn Wire, deadline: Instant) -> bool {
    let forms: [(&[u8], (u32, u32)); 4] = [
        (b"\n", (3, 5)),
        (b"\r", (2, 1)),
        (b"\x08", (2, 4)),
        (b"\x1b[3C", (2, 8)),
    ];
    forms.iter().all(|(form, expected)| {
        let mut q = b"\x1b[2;5H".to_vec();
        q.extend_from_slice(form);
        cursor_after(wire, &q, deadline) == Some(*expected)
    })
}

/// Send `bytes`, then ask where the cursor is. Returns (row, column), counted from 1.
fn cursor_after(wire: &mut dyn Wire, bytes: &[u8], deadline: Instant) -> Option<(u32, u32)> {
    let mut q = bytes.to_vec();
    q.extend_from_slice(b"\x1b[6n");
    let reply = ask(wire, &q, b'R', |b| cursor_report(b).is_some(), deadline)?;
    cursor_report(&reply)
}

/// Send `query` and gather the reply until `done` says it is complete, or the limit passes.
/// `None` when the reply is incomplete or nothing could be sent.
fn ask(
    wire: &mut dyn Wire,
    query: &[u8],
    end: u8,
    done: impl Fn(&[u8]) -> bool,
    deadline: Instant,
) -> Option<Vec<u8>> {
    let stop = deadline.min(Instant::now() + LIMIT);
    wire.send(query).ok()?;
    let mut reply = Vec::new();
    loop {
        let left = stop.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        let chunk = wire.recv_until(end, left).ok()?;
        if chunk.is_empty() {
            return None;
        }
        reply.extend_from_slice(&chunk);
        if done(&reply) {
            return Some(reply);
        }
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// True once the reply holds a whole primary device attributes answer: `ESC [ ?` digits and
/// semicolons, then `c`. A name sent before it may itself contain a `c`.
fn da1_complete(b: &[u8]) -> bool {
    (0..b.len()).any(|i| {
        b[i..].starts_with(&[ESC, b'[', b'?']) && {
            let rest = &b[i + 3..];
            let n = rest
                .iter()
                .take_while(|c| c.is_ascii_digit() || **c == b';')
                .count();
            rest.get(n) == Some(&b'c')
        }
    })
}

/// The name a surface gave in answer to `ESC [ > q`: the first word between `>|` and `ESC \`,
/// in lower case. `ghostty 1.4.0` gives `ghostty`.
fn reported_name(b: &[u8]) -> Option<String> {
    let start = b.windows(2).position(|w| w == b">|")? + 2;
    let rest = &b[start..];
    let end = rest.windows(2).position(|w| w == [ESC, b'\\'])?;
    let text = String::from_utf8_lossy(&rest[..end]);
    let word = text.split_whitespace().next()?;
    Some(normalise(word))
}

/// The last cursor position report in a reply: `ESC [` row `;` column `R`.
fn cursor_report(b: &[u8]) -> Option<(u32, u32)> {
    let mut found = None;
    for i in 0..b.len() {
        if !b[i..].starts_with(&[ESC, b'[']) {
            continue;
        }
        let rest = &b[i + 2..];
        let digits = |s: &[u8]| s.iter().take_while(|c| c.is_ascii_digit()).count();
        let r = digits(rest);
        if r == 0 || rest.get(r) != Some(&b';') {
            continue;
        }
        let cols = &rest[r + 1..];
        let c = digits(cols);
        if c == 0 || cols.get(c) != Some(&b'R') {
            continue;
        }
        let num = |s: &[u8]| std::str::from_utf8(s).ok()?.parse::<u32>().ok();
        if let (Some(row), Some(col)) = (num(&rest[..r]), num(&cols[..c])) {
            found = Some((row, col));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io;

    /// A scripted terminal. It follows the cursor through the few things the probe sends and
    /// answers the questions it has been told to answer.
    struct Fake {
        /// Answers `ESC [ c` at all.
        alive: bool,
        /// Its answer to `ESC [ > q`, if it gives one.
        name: Option<&'static str>,
        /// Answers `ESC [ ? 2026 $ p` with "set" or "reset".
        atomic: bool,
        /// Line feed also returns the carriage, as when the line still adds one.
        lf_returns: bool,
        /// How many columns a half block takes.
        half_width: u32,
        row: u32,
        col: u32,
        sent: Vec<u8>,
        pending: VecDeque<u8>,
    }

    impl Fake {
        fn silent() -> Fake {
            Fake {
                alive: false,
                name: None,
                atomic: false,
                lf_returns: false,
                half_width: 1,
                row: 1,
                col: 1,
                sent: Vec::new(),
                pending: VecDeque::new(),
            }
        }
        fn alive() -> Fake {
            Fake {
                alive: true,
                ..Fake::silent()
            }
        }
        fn reply(&mut self, s: &[u8]) {
            self.pending.extend(s.iter().copied());
        }
        fn run(&mut self, b: &[u8]) {
            let text = String::from_utf8_lossy(b).into_owned();
            let mut it = text.chars().peekable();
            while let Some(c) = it.next() {
                match c {
                    '\x1b' => {
                        let mut seq = String::new();
                        for c in it.by_ref() {
                            seq.push(c);
                            if seq.len() > 1 && (c.is_ascii_alphabetic() || c == '@') {
                                break;
                            }
                        }
                        self.escape(&seq);
                    }
                    '\n' => {
                        self.row += 1;
                        if self.lf_returns {
                            self.col = 1;
                        }
                    }
                    '\r' => self.col = 1,
                    '\x08' => self.col = self.col.saturating_sub(1).max(1),
                    '\u{2580}' | '\u{2584}' => self.col += self.half_width,
                    _ => self.col += 1,
                }
            }
        }
        fn escape(&mut self, seq: &str) {
            let alive = self.alive;
            match seq {
                "[c" if alive => self.reply(b"\x1b[?62;22c"),
                "[>q" if alive => {
                    if let Some(n) = self.name {
                        self.reply(format!("\x1bP>|{n}\x1b\\").as_bytes());
                    }
                }
                "[?2026$p" if alive && self.atomic => self.reply(b"\x1b[?2026;2$y"),
                "[6n" if alive => {
                    let r = format!("\x1b[{};{}R", self.row, self.col);
                    self.reply(r.as_bytes());
                }
                s if s.ends_with('H') => {
                    let mut n = s[1..s.len() - 1].split(';').map(|x| x.parse().unwrap_or(1));
                    self.row = n.next().unwrap_or(1);
                    self.col = n.next().unwrap_or(1);
                }
                s if s.ends_with('C') => {
                    self.col += s[1..s.len() - 1].parse::<u32>().unwrap_or(1);
                }
                _ => {}
            }
        }
    }

    impl Wire for Fake {
        fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.sent.extend_from_slice(bytes);
            self.run(bytes);
            Ok(())
        }
        fn recv_until(&mut self, end: u8, timeout: Duration) -> io::Result<Vec<u8>> {
            if self.pending.is_empty() {
                std::thread::sleep(timeout);
                return Ok(Vec::new());
            }
            let mut out = Vec::new();
            while let Some(b) = self.pending.pop_front() {
                out.push(b);
                if b == end {
                    break;
                }
            }
            Ok(out)
        }
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_silent_terminal_gives_the_cautious_profile_within_a_second() {
        let mut wire = Fake::silent();
        let start = Instant::now();
        let p = probe(&mut wire, &no_env);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(p, Profile::cautious());
    }

    #[test]
    fn an_unknown_term_program_on_a_silent_terminal_is_still_cautious() {
        let mut wire = Fake::silent();
        let env = |k: &str| (k == "TERM_PROGRAM").then(|| "iTerm.app".to_string());
        assert_eq!(probe(&mut wire, &env), Profile::cautious());
    }

    #[test]
    fn apple_terminal_by_environment_whatever_the_wire_says() {
        let mut wire = Fake {
            name: Some("ghostty 1.4.0"),
            atomic: true,
            ..Fake::alive()
        };
        let env = |k: &str| (k == "TERM_PROGRAM").then(|| "Apple_Terminal".to_string());
        assert_eq!(probe(&mut wire, &env), Profile::apple_terminal_2_15());
        assert!(wire.sent.is_empty(), "asked the wire anyway");
    }

    #[test]
    fn a_terminal_that_names_itself_ghostty_gets_the_ghostty_profile() {
        let mut wire = Fake {
            name: Some("ghostty 1.4.0"),
            ..Fake::alive()
        };
        let p = probe(&mut wire, &no_env);
        assert_eq!(p, known("ghostty").unwrap());
        assert_eq!(p.name, "ghostty");
        assert_eq!(
            p.ladders,
            vec![Ladder::Half, Ladder::Quarter, Ladder::Sixth, Ladder::Eighth]
        );
        assert_eq!(p.colour, ColourDepth::Full);
        assert!(p.can.relative_moves && p.can.erase && p.can.atomic_frames);
        assert!(!p.can.repeat_blocks);
        assert_eq!(p.prices, Profile::cautious().prices);
    }

    #[test]
    fn known_names() {
        assert_eq!(
            known("apple-terminal"),
            Some(Profile::apple_terminal_2_15())
        );
        assert_eq!(
            known("Apple_Terminal"),
            Some(Profile::apple_terminal_2_15())
        );
        assert!(known("ghostty").is_some());
        assert!(known("xterm").is_none());
    }

    #[test]
    fn an_unknown_terminal_gets_exactly_what_it_proves() {
        let mut wire = Fake {
            name: Some("someterm 2.0"),
            atomic: true,
            ..Fake::alive()
        };
        let p = probe(&mut wire, &no_env);
        let mut expected = Profile::cautious();
        expected.name = "someterm".into();
        expected.can.atomic_frames = true;
        expected.can.relative_moves = true;
        assert_eq!(p, expected);
    }

    #[test]
    fn an_unknown_terminal_without_a_name_proves_nothing_it_cannot_show() {
        // No name, no answer about atomic frames, and line feed also returns the carriage.
        let mut wire = Fake {
            lf_returns: true,
            ..Fake::alive()
        };
        let p = probe(&mut wire, &no_env);
        let mut expected = Profile::cautious();
        expected.name = "unknown".into();
        assert_eq!(p, expected);
    }

    #[test]
    fn half_blocks_on_the_wrong_column_lose_the_half_rung() {
        let mut wire = Fake {
            half_width: 2,
            ..Fake::alive()
        };
        let p = probe(&mut wire, &no_env);
        assert!(p.ladders.is_empty(), "{:?}", p.ladders);
        assert!(p.can.relative_moves);
    }

    #[test]
    fn replies_are_read_whole() {
        assert!(!da1_complete(b"\x1bP>|contour\x1b\\"));
        assert!(da1_complete(b"\x1bP>|contour\x1b\\\x1b[?65;1;9c"));
        assert_eq!(
            reported_name(b"\x1bP>|ghostty 1.4.0\x1b\\\x1b[?62;22c").as_deref(),
            Some("ghostty")
        );
        assert_eq!(reported_name(b"\x1b[?62;22c"), None);
        assert_eq!(cursor_report(b"\x1b[?2026;0$y\x1b[12;34R"), Some((12, 34)));
        assert_eq!(cursor_report(b"\x1b[?62;22c"), None);
    }
}
