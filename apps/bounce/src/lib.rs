//! The benchmark app: a retro computer with a live status screen, drifting around the canvas.
//! An app, not part of Phosphor. It only draws dots onto a canvas. Built in its own issue.
//!
//! The picture depends only on `seconds`. Moving the computer is Composition's job.

use phosphor_foundry::sheet::{builtin_5x7, Edge, GlyphSheet};
use phosphor_kernel::{Canvas, Rgb};

/// The colour around the computer.
pub const ROOM: Rgb = 0x08090b;
const CASE: Rgb = 0xc9c1a9;
const CASE_DK: Rgb = 0x6f6a59;
const SCREEN: Rgb = 0x06120b;
const INK: Rgb = 0x33ff66;
const DIMC: Rgb = 0x1f9a4c;
const TRACK: Rgb = 0x0f3320;
const AMBER: Rgb = 0xffb000;

const PANE_NAMES: [&str; 4] = ["WORKHORSE", "FRONT DOOR", "MAC", "SPARE"];
const LABELS: [&str; 3] = ["CPU", "MEM", "DSK"];
/// How far each bar wanders: CPU a lot, MEM a little, DSK hardly at all.
const SPANS: [f64; 3] = [1.0, 0.3, 0.02];
/// Width in dots of one small letter, spacing included.
const ADVANCE: i64 = 6;

/// The headline's typeface, baked at 20 px: JetBrains Mono, licence beside it.
const HEADLINE_SHEET: &[u8] = include_bytes!("../assets/jetbrains-mono-20.sheet");
/// The screen must be this many dots wide for the headline to be drawn from the baked sheet.
const HEADLINE_MIN_WIDTH: i64 = 400;

/// The benchmark picture: a retro computer showing a live status board.
pub struct Bounce {
    w: usize,
    h: usize,
    faces: Faces,
}

/// The typefaces and how their edges are drawn.
struct Faces {
    /// Small text: the foundry's built-in 5 x 7.
    small: GlyphSheet,
    /// The headline: JetBrains Mono at 20 px.
    headline: GlyphSheet,
    edge: Edge,
}

impl Bounce {
    /// The computer is 90% of each side of the canvas it will move on. With `soft_edges`, as
    /// on the half rung, drawn text blends its edges; otherwise its edges are hard.
    pub fn new(canvas_w: usize, canvas_h: usize, soft_edges: bool) -> Bounce {
        Bounce {
            w: canvas_w * 9 / 10,
            h: canvas_h * 9 / 10,
            faces: Faces {
                small: builtin_5x7(),
                headline: GlyphSheet::from_bytes(HEADLINE_SHEET)
                    .expect("the baked headline sheet is well formed"),
                edge: if soft_edges { Edge::Soft } else { Edge::Hard },
            },
        }
    }

    /// The computer's size in dots.
    pub fn size(&self) -> (usize, usize) {
        (self.w, self.h)
    }

    /// Draw the computer, filling `out`, which must be `size()`.
    pub fn draw(&self, seconds: f64, out: &mut Canvas) {
        assert_eq!(
            (out.w, out.h),
            (self.w, self.h),
            "the canvas must be the computer's size"
        );
        out.dots.fill(ROOM);
        let t = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        computer(out, &self.faces, t);
    }
}

// ---------------- drawing helpers ----------------

impl Faces {
    /// Small text, clipped to `max_x` (exclusive): a letter is dropped if it would pass it.
    fn text(&self, c: &mut Canvas, x: i64, y: i64, s: &str, ink: Rgb, max_x: i64) {
        let room = ((max_x - x + 1) / ADVANCE).max(0) as usize;
        let s: String = s.chars().take(room).collect();
        self.small.draw(c, x, y, &s, ink, self.edge);
    }

    /// The rows the headline's ink covers, as (first, one past the last) below the top of
    /// its line box.
    fn headline_rows(&self, s: &str) -> (i64, i64) {
        let mut rows = (i64::MAX, i64::MIN);
        for g in s.chars().filter_map(|ch| self.headline.glyphs.get(&ch)) {
            if g.w > 0 && g.h > 0 {
                rows = (
                    rows.0.min(g.top as i64),
                    rows.1.max((g.top + g.h as i32) as i64),
                );
            }
        }
        if rows.0 > rows.1 {
            (0, 0)
        } else {
            rows
        }
    }
}

/// How wide `n` small letters are, without the spacing after the last.
fn text_width(n: usize) -> i64 {
    if n == 0 {
        0
    } else {
        n as i64 * ADVANCE - 1
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

/// A smooth wandering value in 0..1.
fn noise(seed: u32, x: f64) -> f64 {
    let k = x.floor();
    let n = k as u32;
    let (a, b) = (
        rnd(seed.wrapping_add(n)),
        rnd(seed.wrapping_add(n).wrapping_add(1)),
    );
    a + (b - a) * smooth(x - k)
}

// ---------------- the computer ----------------

/// The computer: a case, a screen, a stand and a base. Nothing is drawn that does not fit.
fn computer(c: &mut Canvas, f: &Faces, t: f64) {
    let (w, h) = (c.w as i64, c.h as i64);
    let mh = h * 78 / 100;
    let bz = (w / 40).max(5);
    let chin = 11;
    c.fill_rect(0, 0, w, mh, CASE);
    for (x, y) in [(0, 0), (w - 1, 0), (0, mh - 1), (w - 1, mh - 1)] {
        c.set(x, y, ROOM);
    }
    let (sx, sy, sw, sh) = (bz, bz, w - 2 * bz, mh - bz - chin - 2);
    c.fill_rect(sx - 1, sy - 1, sw + 2, sh + 2, CASE_DK);
    c.fill_rect(sx, sy, sw, sh, SCREEN);
    f.text(c, bz + 2, mh - chin + 1, "OCTAL8", CASE_DK, w);
    let led = if (t as i64) % 2 == 0 { INK } else { CASE_DK };
    c.fill_rect(w - bz - 6, mh - chin + 3, 3, 3, led);
    c.fill_rect(w / 2 - w / 10, mh, w / 5, 3, CASE_DK);
    let (by, bh) = (mh + 3, h - mh - 3);
    c.fill_rect(w / 20, by, w - w / 10, bh, CASE);
    c.fill_rect(w * 55 / 100, by + bh / 2 - 1, w / 7, 2, CASE_DK);
    c.fill_rect(w * 73 / 100, by + bh / 2 - 1, w / 7, 2, CASE_DK);
    for i in 0..5 {
        c.fill_rect(w / 20 + 6 + i * 4, by + 3, 2, bh - 6, CASE_DK);
    }
    status_board(
        c,
        f,
        (sx + 4, sy + 4, sw - 8, sh - 8),
        sw >= HEADLINE_MIN_WIDTH,
        t,
    );
}

/// What is on the screen: a title, a clock, then panes of bars and a chart, then a footer.
/// `area` is the board's (x, y, w, h). With `big_title` the title is drawn from the baked
/// JetBrains Mono sheet, if the board is tall enough to hold it.
fn status_board(c: &mut Canvas, f: &Faces, area: (i64, i64, i64, i64), big_title: bool, t: f64) {
    let (x, y, w, h) = area;
    if w < ADVANCE * 4 || h < 8 {
        return;
    }
    let right = x + w;
    let title = "OCTAL8 STATUS";
    // The title's ink is `head` dots tall, whichever face draws it.
    let (top, bottom) = f.headline_rows(title);
    let big = big_title && h >= bottom - top + 16;
    let head = if big { bottom - top } else { 7 };
    let title_w = if big {
        f.headline.measure(title).0 as i64
    } else {
        text_width(title.len())
    };
    let clock_w = text_width(8);
    if big {
        f.headline.draw(c, x, y - top, title, INK, f.edge);
    } else {
        f.text(c, x, y, title, INK, right);
    }
    if w >= title_w + clock_w + 8 {
        let sec = t as i64;
        let clock = format!(
            "{:02}:{:02}:{:02}",
            sec / 3600 % 24,
            sec / 60 % 60,
            sec % 60
        );
        f.text(c, right - clock_w, y + (head - 7) / 2, &clock, AMBER, right);
    }
    // Rules, text and bars of different colours stay four dots or more apart.
    let top_of_panes = head + 9;
    if h >= top_of_panes {
        c.fill_rect(x, y + head + 4, w, 1, DIMC);
    }
    let footer = h >= 70 && w >= text_width(21);
    let body = h - top_of_panes - if footer { 12 } else { 0 };
    let n = (w / 150).max(1);
    let pw = w / n;
    if body >= 7 {
        for p in 0..n {
            let px = x + p * pw;
            pane(
                c,
                f,
                (px, y + top_of_panes, (pw - 6).max(1), body),
                p as u32,
                t,
            );
        }
    }
    if footer {
        let done = 12 + (t / 9.0) as i64 % 3;
        f.text(
            c,
            x,
            y + h - 7,
            &format!("BUILD MODE {}/14 DONE", done),
            DIMC,
            right,
        );
    }
}

/// One machine: a name, three bars that glide twice a second, and a scrolling chart.
fn pane(c: &mut Canvas, f: &Faces, area: (i64, i64, i64, i64), p: u32, t: f64) {
    let (x, y, w, h) = area;
    let right = x + w;
    f.text(c, x, y, PANE_NAMES[p as usize % 4], INK, right);
    let label_w = text_width(3);
    // Room for the label, a gap of five, the bar, a gap of four, and the percentage.
    let bx = x + label_w + 5;
    let pct_x = right - text_width(4);
    let bw = pct_x - 4 - bx;
    let tick = (t * 2.0).floor();
    for i in 0..3i64 {
        let yy = y + 11 + i * 10;
        if yy + 7 > y + h || bw < 4 {
            return;
        }
        let id = p * 16 + i as u32;
        let span = SPANS[i as usize];
        let (a, b) = (
            rnd(id * 7919 + tick as u32),
            rnd(id * 7919 + tick as u32 + 1),
        );
        let v = 0.15 + span * 0.8 * (a + (b - a) * smooth(t * 2.0 - tick));
        f.text(c, x, yy, LABELS[i as usize], DIMC, right);
        c.fill_rect(bx, yy + 1, bw, 5, TRACK);
        c.fill_rect(bx, yy + 1, (v * bw as f64).round() as i64, 5, INK);
        let pct = ((0.15 + span * 0.8 * b) * 100.0) as i64;
        f.text(c, pct_x, yy, &format!("{:3}%", pct), AMBER, right);
    }
    // A live history chart, sliding left a dot at a time. It needs room for a line five dots
    // above its base, and is left out when it has none.
    let cy = y + 11 + 3 * 10 + 1;
    let ch = y + h - cy;
    if ch < 12 {
        return;
    }
    let base = cy + ch - 1;
    c.fill_rect(x, base, w, 1, DIMC);
    for cx in 0..w {
        let k = t * 20.0 + cx as f64;
        let val = 0.75 * noise(p * 131, k / 14.0) + 0.25 * noise(p * 17 + 9, k / 3.0);
        let hh = 5 + (val * (ch - 7) as f64) as i64;
        c.fill_rect(x + cx, base - hh, 1, hh, TRACK);
        c.set(x + cx, base - hh, INK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(cw: usize, ch: usize, t: f64) -> Canvas {
        let b = Bounce::new(cw, ch, false);
        let (w, h) = b.size();
        let mut c = Canvas::new(w, h, 0);
        b.draw(t, &mut c);
        c
    }

    #[test]
    fn the_computer_is_ninety_percent() {
        assert_eq!(Bounce::new(288, 162, false).size(), (259, 145));
    }

    #[test]
    fn same_seconds_same_canvas() {
        assert_eq!(drawn(288, 162, 12.3), drawn(288, 162, 12.3));
    }

    #[test]
    fn drawing_over_old_dots_gives_the_same_canvas() {
        let b = Bounce::new(288, 162, false);
        let (w, h) = b.size();
        let mut c = Canvas::new(w, h, 0x123456);
        b.draw(3.0, &mut c);
        assert_eq!(c, drawn(288, 162, 3.0));
    }

    #[test]
    fn time_moves_the_picture() {
        assert_ne!(drawn(288, 162, 1.0), drawn(288, 162, 1.6));
    }

    #[test]
    fn the_four_colours_are_there() {
        let c = drawn(288, 162, 5.0);
        assert_eq!(c.dots.len(), c.w * c.h);
        for want in [ROOM, CASE, INK, AMBER] {
            assert!(c.dots.contains(&want), "missing {want:06x}");
        }
        let known = [ROOM, CASE, CASE_DK, SCREEN, INK, DIMC, TRACK, AMBER];
        assert!(c.dots.iter().all(|d| known.contains(d)));
    }

    #[test]
    fn corners_are_room() {
        let c = drawn(288, 162, 5.0);
        assert_eq!(c.get(0, 0), Some(ROOM));
        assert_eq!(c.get(c.w as i64 - 1, c.h as i64 - 1), Some(ROOM));
    }

    #[test]
    fn many_sizes_draw() {
        for (w, h) in [
            (80, 40),
            (288, 162),
            (800, 440),
            (0, 0),
            (1, 1),
            (10, 200),
            (200, 10),
            (150, 80),
            (600, 100),
        ] {
            for t in [0.0, 0.7, 59.9, 3600.5, 1e9] {
                drawn(w, h, t);
            }
        }
    }

    #[test]
    fn odd_times_do_not_panic() {
        for t in [-5.0, f64::NAN, f64::INFINITY] {
            drawn(288, 162, t);
        }
    }

    #[test]
    fn wide_canvases_get_more_panes() {
        let count = |c: &Canvas| c.dots.iter().filter(|&&d| d == INK).count();
        assert!(count(&drawn(800, 440, 5.0)) > count(&drawn(288, 162, 5.0)));
    }

    #[test]
    fn short_screens_leave_the_chart_out() {
        // The header rule and a chart's baseline are both long unbroken runs of DIMC.
        let long_rows = |c: &Canvas| {
            c.dots
                .chunks(c.w)
                .filter(|row| row.iter().filter(|&&d| d == DIMC).count() > 100)
                .count()
        };
        assert_eq!(long_rows(&drawn(288, 162, 5.0)), 2);
        assert_eq!(long_rows(&drawn(288, 100, 5.0)), 1);
    }

    /// Different bright inks never sit within four dots of each other, so a cell of up to
    /// 2 x 4 dots never holds two different inks.
    #[test]
    fn inks_stay_four_dots_apart() {
        let inks = [INK, DIMC, AMBER];
        for (cw, ch) in [(80, 40), (288, 162), (400, 220), (800, 440), (1200, 300)] {
            for t in [0.0, 0.3, 1.0, 1.6, 7.7, 100.2] {
                let c = drawn(cw, ch, t);
                for y in 0..c.h as i64 {
                    for x in 0..c.w as i64 {
                        let a = c.get(x, y).unwrap();
                        if !inks.contains(&a) {
                            continue;
                        }
                        for dy in -3..=3i64 {
                            for dx in -3..=3i64 {
                                if let Some(b) = c.get(x + dx, y + dy) {
                                    assert!(
                                        a == b || !inks.contains(&b),
                                        "{a:06x} and {b:06x} meet at ({x},{y}) {cw}x{ch} t={t}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// The headline's ink, in the board's top-left corner, drawn the way the picture draws it.
    fn headline_dots(c: &Canvas, edge: Edge) -> (usize, usize) {
        let sheet = GlyphSheet::from_bytes(HEADLINE_SHEET).unwrap();
        let bz = (c.w as i64 / 40).max(5);
        let (bx, by) = (bz + 4, bz + 4);
        let mut want = Canvas::new(c.w, c.h, SCREEN);
        let faces = Faces {
            small: builtin_5x7(),
            headline: sheet.clone(),
            edge,
        };
        let (top, _) = faces.headline_rows("OCTAL8 STATUS");
        sheet.draw(&mut want, bx, by - top, "OCTAL8 STATUS", INK, edge);
        let (w, _) = sheet.measure("OCTAL8 STATUS");
        let mut inked = 0;
        let mut matching = 0;
        for y in by..by + 30 {
            for x in bx..bx + w as i64 {
                if want.get(x, y) != Some(SCREEN) {
                    inked += 1;
                    matching += (c.get(x, y) == want.get(x, y)) as usize;
                }
            }
        }
        (inked, matching)
    }

    #[test]
    fn a_wide_screen_draws_the_headline_from_jetbrains_mono() {
        let c = drawn(800, 440, 5.0);
        let (inked, matching) = headline_dots(&c, Edge::Hard);
        assert!(inked > 100, "the sheet's headline has {inked} dots");
        assert_eq!(inked, matching);
    }

    #[test]
    fn a_narrow_screen_keeps_the_small_face() {
        let c = drawn(288, 162, 5.0);
        // The small face is 7 dots tall, so the rule sits 11 dots below the board's top.
        let (bx, by) = (259 / 40 + 4, 259 / 40 + 4);
        assert_eq!(c.get(bx, by + 11), Some(DIMC));
        let (inked, matching) = headline_dots(&c, Edge::Hard);
        assert!(matching < inked);
    }

    #[test]
    fn the_wide_screen_cut_off_is_four_hundred_dots() {
        // A canvas of 480 gives a computer of 432 and a screen of 412; 440 gives 396 and 376.
        let big = |cw: usize, ch: usize| {
            let c = drawn(cw, ch, 5.0);
            let (inked, matching) = headline_dots(&c, Edge::Hard);
            inked > 0 && inked == matching
        };
        assert!(big(480, 300));
        assert!(!big(440, 280));
    }

    #[test]
    fn soft_edges_blend_only_the_headline() {
        let draw_with = |cw: usize, ch: usize, soft: bool| {
            let b = Bounce::new(cw, ch, soft);
            let (w, h) = b.size();
            let mut c = Canvas::new(w, h, 0);
            b.draw(5.0, &mut c);
            c
        };
        assert_eq!(draw_with(288, 162, true), draw_with(288, 162, false));
        let (soft, hard) = (draw_with(800, 440, true), draw_with(800, 440, false));
        assert_ne!(soft, hard);
        let (inked, matching) = headline_dots(&soft, Edge::Soft);
        assert!(inked > 0 && inked == matching);
        let changed = soft.dots.iter().zip(&hard.dots).filter(|(a, b)| a != b);
        assert!(changed.count() < 800);
    }

    #[test]
    fn fast_enough() {
        let b = Bounce::new(288, 162, false);
        let (w, h) = b.size();
        let mut c = Canvas::new(w, h, 0);
        let start = std::time::Instant::now();
        for i in 0..100 {
            b.draw(i as f64 * 0.016, &mut c);
        }
        // Reported by `cargo test --release`; debug builds are far slower, so only guard
        // against something absurd here.
        eprintln!("draw: {:?} each", start.elapsed() / 100);
        assert!(start.elapsed() / 100 < std::time::Duration::from_millis(20));
    }
}
