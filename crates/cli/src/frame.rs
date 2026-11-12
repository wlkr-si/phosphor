//! One frame of `phosphor play bounce`, as plain calculation: compose, thrift, fit, mark,
//! encode, and, when governed, admit or hold.
//!
//! Nothing here touches a terminal, so the bench can replay every frame's bytes.

use phosphor_bounce::{Bounce, ROOM};
use phosphor_compose::{Layer, Motion, Placement, Scene};
use phosphor_encode::encode;
use phosphor_fit::exact::fit;
use phosphor_fit::thrift::thrift;
use phosphor_foundry::picture::scale_to_fit;
use phosphor_govern::developing::Developing;
use phosphor_govern::governor::{Governor, Verdict};
use phosphor_govern::plan::Plan;
use phosphor_kernel::{
    colour_distance, Budget, Canvas, Cell, ColourDepth, Grid, Ladder, Profile, Rgb, Tally,
};
use std::collections::HashMap;

/// The paper of a registration mark. See "Registration marks" in docs/architecture.md.
pub const MARK: Rgb = 0xFF00FF;
const WHITE: Rgb = 0xFFFFFF;
const BLACK: Rgb = 0x000000;

/// How the picture looks, apart from the surface it is drawn on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// How fast the computer moves, in dots a second.
    pub speed: f64,
    /// Paint the frame counter strip.
    pub strip: bool,
    /// Paint the registration marks.
    pub marks: bool,
    /// Hold the picture as it is at this instant.
    pub freeze_ns: Option<u64>,
}

/// The governor's share of making frames: the plan, the governor, and the rows developing.
struct Governing {
    plan: Plan,
    budget: Budget,
    governor: Governor,
    developing: Developing,
}

/// What became of the last frame made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Made {
    /// The run limit thrift used, when governed.
    pub max_runs: Option<usize>,
    /// The frame meant would have run over, so only part of it was sent.
    pub held: bool,
    /// On a held frame: the tally the whole frame meant would have had.
    pub whole: Option<Tally>,
    /// On a held frame: the rows that moved, which go whatever the budget.
    pub moving: Vec<usize>,
    /// On a held frame: every row sent, moving rows and still rows taken from developing.
    pub taken: Vec<usize>,
}

/// Makes frames of the bouncing computer for one grid size on one surface.
pub struct Player {
    profile: Profile,
    ladder: Ladder,
    look: Look,
    bounce: Bounce,
    scene: Scene,
    canvas: Canvas,
    /// The grid meant now, before the strip and marks.
    plain: Grid,
    /// The grid meant now, as it would be sent whole.
    grid: Grid,
    /// The plain grid the surface was last told to show.
    last_plain: Option<Grid>,
    /// The grid last sent: what the surface was told to show.
    last_sent: Option<Grid>,
    /// The instant of the frame last sent.
    last_t: Option<u64>,
    /// Frames sent so far. The next frame's number on the strip.
    sent: u64,
    /// Colours already brought down to the profile's colour depth.
    nearest: HashMap<Rgb, Rgb>,
    governing: Option<Governing>,
    made: Made,
}

impl Player {
    /// A player for a grid of `cols` × `rows` on `profile`, drawing on `ladder`.
    pub fn new(cols: usize, rows: usize, profile: &Profile, ladder: Ladder, look: Look) -> Player {
        let (w, h) = ladder.canvas_size(cols, rows);
        let bounce = Bounce::new(w, h, matches!(ladder, Ladder::Half));
        let (bw, bh) = bounce.size();
        let scene = Scene {
            w,
            h,
            background: ROOM,
            layers: vec![Layer {
                canvas: Canvas::new(bw, bh, ROOM),
                placement: Placement { x: 0, y: 0 },
                motion: Motion::Bounce {
                    dots_per_second: look.speed,
                },
                clear: None,
                visible: true,
            }],
        };
        Player {
            profile: profile.clone(),
            ladder,
            look,
            bounce,
            scene,
            canvas: Canvas::new(w, h, ROOM),
            plain: Grid::new(cols, rows),
            grid: Grid::new(cols, rows),
            last_plain: None,
            last_sent: None,
            last_t: None,
            sent: 0,
            nearest: HashMap::new(),
            governing: None,
            made: Made::default(),
        }
    }

    /// A governed player: the plan sets the rung and the grid, and colours are encoded at
    /// the plan's depth. `profile`'s price list is the one the governor judges frames by.
    pub fn governed(plan: Plan, profile: &Profile, budget: Budget, look: Look) -> Player {
        let surface = Profile {
            colour: plan.depth,
            ..profile.clone()
        };
        let mut p = Player::new(plan.cols, plan.rows, &surface, plan.ladder, look);
        p.governing = Some(Governing {
            plan,
            budget,
            governor: Governor::new(&surface, budget, plan),
            developing: Developing::new(plan.rows),
        });
        p
    }

    /// Put a still picture behind the computer, scaled to fit the scene and centred. The
    /// room around the computer becomes see-through, so the picture shows there.
    pub fn behind(&mut self, picture: &Canvas) {
        let (dw, dh) = self.ladder.dots();
        // A cell is about twice as tall as it is wide.
        let dot_aspect = 2.0 * dw as f64 / dh as f64;
        let canvas = scale_to_fit(picture, self.scene.w, self.scene.h, dot_aspect);
        let placement = Placement {
            x: (self.scene.w as i64 - canvas.w as i64) / 2,
            y: (self.scene.h as i64 - canvas.h as i64) / 2,
        };
        for layer in &mut self.scene.layers {
            layer.clear = Some(ROOM);
        }
        self.scene.layers.insert(
            0,
            Layer {
                canvas,
                placement,
                motion: Motion::Still,
                clear: None,
                visible: true,
            },
        );
    }

    /// The grid size, as (columns, rows).
    pub fn size(&self) -> (usize, usize) {
        (self.grid.cols, self.grid.rows)
    }

    /// The plan, when governed.
    pub fn plan(&self) -> Option<Plan> {
        self.governing.as_ref().map(|g| g.plan)
    }

    /// What became of the last frame made.
    pub fn made(&self) -> &Made {
        &self.made
    }

    /// The grid the surface was last told to show, if any frame has gone out.
    #[cfg(test)]
    pub fn shown(&self) -> Option<&Grid> {
        self.last_sent.as_ref()
    }

    /// The grid meant by the last frame made, whole, whether or not all of it went out.
    #[cfg(test)]
    pub fn meant(&self) -> &Grid {
        &self.grid
    }

    /// The budget the governor judges frames by, when governed.
    #[cfg(test)]
    pub fn budget(&self) -> Option<Budget> {
        self.governing.as_ref().map(|g| {
            debug_assert_eq!(g.budget, g.governor.budget());
            g.governor.budget()
        })
    }

    /// The run limit the next frame will be thrifted to, when governed.
    #[cfg(test)]
    pub fn max_runs(&self) -> Option<usize> {
        self.governing.as_ref().map(|g| g.governor.max_runs())
    }

    /// Tell the governor what a frame sent cost: its tally and, when it was answered, how
    /// long the answer took. `whole` is the frame meant when the frame was held.
    ///
    /// A held frame is judged as the whole frame meant, because that is what did not fit:
    /// its answer time is scaled up by the price of the whole over the price of the part
    /// sent. The governor learns the same ratio of answer to price either way, and frames
    /// that keep being held make it thriftier.
    pub fn record(&mut self, sent: &Tally, whole: Option<&Tally>, took_ns: Option<f64>) {
        let Some(g) = &mut self.governing else {
            return;
        };
        let prices = &self.profile.prices;
        match whole {
            Some(w) if prices.price(sent) > 0.0 => {
                let scale = prices.price(w) / prices.price(sent);
                g.governor.record(w, took_ns.map(|t| t * scale));
            }
            Some(w) => g.governor.record(w, None),
            None => g.governor.record(sent, took_ns),
        }
    }

    /// Make the frame for instant `t_ns` and append its bytes to `out`. Returns its tally, or
    /// `None` when there is nothing to send. A returned frame counts as sent.
    ///
    /// Governed, the canvas is thrifted to the governor's run limit first, and a frame the
    /// governor would hold goes out in part: the rows that moved, then still rows from
    /// developing while the budget lasts. The rest wait for later frames.
    pub fn frame(&mut self, t_ns: u64, out: &mut Vec<u8>) -> Option<Tally> {
        let t_ns = self.look.freeze_ns.map_or(t_ns, |f| t_ns.min(f));
        self.made = Made::default();
        let computer = self.scene.layers.len() - 1;
        self.bounce
            .draw(t_ns as f64 / 1e9, &mut self.scene.layers[computer].canvas);
        self.scene.compose(t_ns, &mut self.canvas);
        if let Some(g) = &self.governing {
            let runs = g.governor.max_runs();
            thrift(&mut self.canvas, self.ladder, runs, g.plan.depth);
            self.made.max_runs = Some(runs);
        }
        self.bring_to_depth();
        fit(
            &self.canvas,
            self.ladder,
            self.last_plain.as_ref(),
            &mut self.plain,
        );
        if self
            .last_plain
            .as_ref()
            .is_some_and(|last| looks_alike(last, &self.plain))
        {
            return None;
        }
        self.grid.cells.copy_from_slice(&self.plain.cells);
        self.dress_grid();
        let start = out.len();
        let tally = encode(self.last_sent.as_ref(), &self.grid, &self.profile, out);
        if out.len() == start {
            return None;
        }
        let hold = self
            .governing
            .as_ref()
            .is_some_and(|g| g.governor.admit(&tally) == Verdict::Hold);
        let tally = if hold {
            out.truncate(start);
            self.hold(t_ns, &tally, out)?
        } else {
            if let Some(g) = &mut self.governing {
                // Everything meant goes out, so nothing is left developing.
                g.developing = Developing::new(self.grid.rows);
            }
            match (&mut self.last_plain, &mut self.last_sent) {
                (Some(p), Some(s)) => {
                    p.cells.copy_from_slice(&self.plain.cells);
                    s.cells.copy_from_slice(&self.grid.cells);
                }
                _ => {
                    self.last_plain = Some(self.plain.clone());
                    self.last_sent = Some(self.grid.clone());
                }
            }
            tally
        };
        self.sent += 1;
        self.last_t = Some(t_ns);
        Some(tally)
    }

    /// Send part of a frame the governor held: the rows that moved since the frame last
    /// sent, and still rows from developing while the budget lasts. `whole` is the tally the
    /// whole frame would have had.
    fn hold(&mut self, t_ns: u64, whole: &Tally, out: &mut Vec<u8>) -> Option<Tally> {
        let mut g = self.governing.take()?;
        let result = self.hold_with(&mut g, t_ns, whole, out);
        self.governing = Some(g);
        result
    }

    fn hold_with(
        &mut self,
        g: &mut Governing,
        t_ns: u64,
        whole: &Tally,
        out: &mut Vec<u8>,
    ) -> Option<Tally> {
        self.made.held = true;
        self.made.whole = Some(*whole);
        let (cols, rows) = (self.plain.cols, self.plain.rows);
        // What the surface shows now, without strip and marks. Before the first frame it is
        // unknown, so the room is painted first: one run a row, the cheapest whole picture.
        let base = self.last_plain.clone().unwrap_or_else(|| {
            let room = space(nearest_at(ROOM, self.profile.colour));
            Grid {
                cols,
                rows,
                cells: vec![room; cols * rows],
            }
        });
        let differs = |r: usize| {
            row_of(&base, r)
                .iter()
                .zip(row_of(&self.plain, r))
                .any(|(a, b)| !a.looks_like(b))
        };
        let damaged: Vec<usize> = (0..rows).filter(|&r| differs(r)).collect();
        let moving = self.moving_rows(t_ns);
        for &r in &damaged {
            if moving[r] {
                g.developing.mark_moving(r);
            } else {
                g.developing.mark_still(r);
            }
        }
        let row_ns = g.governor.predict(whole) / damaged.len().max(1) as f64;
        let mut taken = g.developing.take(g.budget.frame_ns, row_ns);
        let start = out.len();
        let (tally, part, dressed) = loop {
            let mut part = base.clone();
            for &r in &taken {
                part.cells[r * cols..(r + 1) * cols].copy_from_slice(row_of(&self.plain, r));
            }
            let mut dressed = part.clone();
            self.dress(&mut dressed);
            out.truncate(start);
            let tally = encode(self.last_sent.as_ref(), &dressed, &self.profile, out);
            let still: Vec<usize> = taken.iter().copied().filter(|&r| !moving[r]).collect();
            if g.governor.admit(&tally) == Verdict::Send || still.is_empty() {
                break (tally, part, dressed);
            }
            // The estimate was low: put back the later half of the still rows and try again.
            for &r in &still[still.len() / 2..] {
                taken.retain(|&t| t != r);
                g.developing.mark_still(r);
            }
        };
        self.made.moving = (0..rows)
            .filter(|&r| moving[r] && taken.contains(&r))
            .collect();
        self.made.taken = taken;
        if out.len() == start {
            // Nothing could go without running over. Tell the governor, so that frames that
            // keep failing like this make it thriftier.
            g.governor.record(whole, None);
            return None;
        }
        self.last_plain = Some(part);
        self.last_sent = Some(dressed);
        Some(tally)
    }

    /// Which rows of cells the moving layers covered at the frame last sent or cover now.
    fn moving_rows(&self, t_ns: u64) -> Vec<bool> {
        let mut rows = vec![false; self.plain.rows];
        let Some(rect) = self.last_t.and_then(|t0| self.scene.moved(t0, t_ns)) else {
            return rows;
        };
        let (_, dh) = self.ladder.dots();
        let first = (rect.y.max(0) as usize) / dh;
        let last = ((rect.y + rect.h - 1).max(0) as usize) / dh;
        for r in rows.iter_mut().take(last + 1).skip(first) {
            *r = true;
        }
        rows
    }

    /// Paint the strip and the marks on the grid meant.
    fn dress_grid(&mut self) {
        let mut grid = std::mem::replace(&mut self.grid, Grid::new(0, 0));
        self.dress(&mut grid);
        self.grid = grid;
    }

    /// Paint the strip and the marks, when asked for, on `grid`.
    fn dress(&self, grid: &mut Grid) {
        if self.look.marks {
            paint_marks(grid);
        }
        if self.look.strip {
            paint_strip(grid, self.sent);
        }
    }

    /// Bring every dot to a colour the surface can be asked for, so the grid meant is the
    /// grid it can show.
    fn bring_to_depth(&mut self) {
        let depth = self.profile.colour;
        if depth == ColourDepth::Full {
            return;
        }
        for d in &mut self.canvas.dots {
            *d = *self
                .nearest
                .entry(*d)
                .or_insert_with(|| nearest_at(*d, depth));
        }
    }
}

/// Row `r` of a grid's cells.
fn row_of(grid: &Grid, r: usize) -> &[Cell] {
    &grid.cells[r * grid.cols..(r + 1) * grid.cols]
}

/// True when every cell of `a` looks like the same cell of `b`.
pub fn looks_alike(a: &Grid, b: &Grid) -> bool {
    a.cols == b.cols
        && a.rows == b.rows
        && a.cells.iter().zip(&b.cells).all(|(x, y)| x.looks_like(y))
}

/// The nearest colour that exists at `depth`. At 256 colours only the cube and the greys
/// are used: the sixteen basic colours differ from terminal to terminal.
pub fn nearest_at(c: Rgb, depth: ColourDepth) -> Rgb {
    const CUBE: [u32; 6] = [0, 95, 135, 175, 215, 255];
    let channel = |shift: u32| c >> shift & 255;
    match depth {
        ColourDepth::Full => c,
        ColourDepth::Indexed8 => {
            let ch = |shift: u32| if channel(shift) >= 128 { 255 } else { 0 };
            ch(16) << 16 | ch(8) << 8 | ch(0)
        }
        ColourDepth::Indexed256 => {
            let level = |v: u32| {
                *CUBE
                    .iter()
                    .min_by_key(|l| (**l as i32 - v as i32).unsigned_abs())
                    .unwrap_or(&0)
            };
            let cube = level(channel(16)) << 16 | level(channel(8)) << 8 | level(channel(0));
            let greys = (0..24u32).map(|n| (8 + 10 * n) * 0x010101);
            std::iter::once(cube)
                .chain(greys)
                .min_by_key(|x| colour_distance(c, *x))
                .unwrap_or(cube)
        }
    }
}

fn space(paper: Rgb) -> Cell {
    Cell {
        ch: ' ',
        ink: paper,
        paper,
    }
}

/// The four corner cells, as spaces on the mark's paper.
pub fn paint_marks(g: &mut Grid) {
    if g.cols == 0 || g.rows == 0 {
        return;
    }
    for (c, r) in [
        (0, 0),
        (g.cols - 1, 0),
        (0, g.rows - 1),
        (g.cols - 1, g.rows - 1),
    ] {
        g.set(c, r, space(MARK));
    }
}

/// The frame counter strip: the frame number, modulo 65,536, most significant bit first in
/// the first 16 cells of the top row, white for 1 and black for 0; then a white cell and a
/// black one. Cells past the grid's edge are left out.
pub fn paint_strip(g: &mut Grid, frame: u64) {
    if g.rows == 0 {
        return;
    }
    let n = (frame % 65_536) as u16;
    for i in 0..18.min(g.cols) {
        let white = match i {
            0..=15 => n >> (15 - i) & 1 == 1,
            16 => true,
            _ => false,
        };
        g.set(i, 0, space(if white { WHITE } else { BLACK }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::{priced, Maker, Options};
    use phosphor_bench::{replay, StrictTerminal};
    use phosphor_govern::plan::choose;
    use phosphor_kernel::{test_card, PriceList};

    const BEAT_NS: u64 = 16_666_667;

    fn look(strip: bool, marks: bool) -> Look {
        Look {
            speed: 10.0,
            strip,
            marks,
            freeze_ns: None,
        }
    }

    fn finest(p: &Profile) -> Ladder {
        *p.ladders.last().unwrap()
    }

    /// The bytes of every frame, fed to a strict terminal with the same profile, leave it
    /// showing exactly the grid meant.
    #[test]
    fn every_frame_replays_to_the_grid_meant() {
        for profile in [Profile::apple_terminal_2_15(), Profile::cautious()] {
            for (strip, marks) in [(false, false), (true, true)] {
                let (cols, rows) = (160, 45);
                let mut player =
                    Player::new(cols, rows, &profile, finest(&profile), look(strip, marks));
                let mut term = StrictTerminal::new(cols, rows, &profile);
                let mut frames = Vec::new();
                for i in 0..120u64 {
                    let mut out = Vec::new();
                    let Some(tally) = player.frame(i * BEAT_NS, &mut out) else {
                        continue;
                    };
                    assert_eq!(tally.bytes, out.len() as u64);
                    term.feed(&out)
                        .unwrap_or_else(|v| panic!("{} frame {i}: {}", profile.name, v.0));
                    let meant = player.shown().unwrap();
                    assert!(
                        looks_alike(term.grid(), meant),
                        "{} frame {i}: the terminal shows another picture",
                        profile.name
                    );
                    frames.push(out);
                }
                assert!(
                    frames.len() > 60,
                    "{}: {} frames",
                    profile.name,
                    frames.len()
                );
                // Every frame also ends cleanly, outside any sequence.
                let end = replay(&profile, cols, rows, &frames).unwrap();
                assert!(looks_alike(&end, player.shown().unwrap()));
            }
        }
    }

    #[test]
    fn nothing_is_sent_while_the_picture_holds() {
        let profile = Profile::apple_terminal_2_15();
        let mut l = look(true, true);
        l.freeze_ns = Some(BEAT_NS * 10);
        let mut player = Player::new(80, 24, &profile, Ladder::Quarter, l);
        let mut sent = 0;
        for i in 0..40u64 {
            let mut out = Vec::new();
            match player.frame(i * BEAT_NS, &mut out) {
                Some(_) => {
                    assert!(i <= 10, "frame {i} was sent after the freeze");
                    assert!(!out.is_empty());
                    sent += 1;
                }
                None => assert!(out.is_empty(), "frame {i} wrote bytes it did not send"),
            }
        }
        assert!(sent >= 1);
    }

    #[test]
    fn the_first_frame_paints_every_cell() {
        let profile = Profile::cautious();
        let mut player = Player::new(40, 12, &profile, Ladder::Half, look(false, false));
        let mut out = Vec::new();
        let tally = player.frame(0, &mut out).unwrap();
        assert_eq!(tally.cells, 40 * 12);
    }

    #[test]
    fn the_strip_counts_frames_sent() {
        let profile = Profile::apple_terminal_2_15();
        let mut player = Player::new(60, 20, &profile, Ladder::Half, look(true, false));
        let mut numbers = Vec::new();
        for i in 0..30u64 {
            let mut out = Vec::new();
            if player.frame(i * BEAT_NS * 3, &mut out).is_some() {
                let g = player.shown().unwrap();
                let n = (0..16).fold(0u32, |n, c| n << 1 | (g.get(c, 0).paper == WHITE) as u32);
                assert_eq!(g.get(16, 0).paper, WHITE);
                assert_eq!(g.get(17, 0).paper, BLACK);
                numbers.push(n);
            }
        }
        assert!(numbers.len() > 5);
        assert!(numbers.iter().enumerate().all(|(i, n)| *n == i as u32));
    }

    #[test]
    fn strip_wraps_and_marks_sit_in_the_corners() {
        let mut g = Grid::new(20, 5);
        paint_strip(&mut g, 65_536 + 0b1000_0000_0000_0011);
        let bits: Vec<bool> = (0..16).map(|c| g.get(c, 0).paper == WHITE).collect();
        assert!(bits[0] && bits[14] && bits[15]);
        assert_eq!(bits.iter().filter(|b| **b).count(), 3);
        paint_marks(&mut g);
        for (c, r) in [(0, 0), (19, 0), (0, 4), (19, 4)] {
            assert_eq!(g.get(c, r).paper, MARK);
            assert_eq!(g.get(c, r).ch, ' ');
        }
        // Tiny grids do not panic.
        paint_strip(&mut Grid::new(3, 1), 7);
        paint_marks(&mut Grid::new(1, 1));
        paint_marks(&mut Grid::new(0, 0));
    }

    #[test]
    fn colours_come_down_to_the_table() {
        assert_eq!(nearest_at(0x123456, ColourDepth::Full), 0x123456);
        assert_eq!(nearest_at(0xFF00FF, ColourDepth::Indexed256), 0xFF00FF);
        assert_eq!(nearest_at(0x08090B, ColourDepth::Indexed256), 0x080808);
        assert_eq!(nearest_at(0x33FF66, ColourDepth::Indexed256), 0x5FFF5F);
        assert_eq!(nearest_at(0x80FF7F, ColourDepth::Indexed8), 0xFFFF00);
    }

    /// A hash of every frame's bytes and whether it was sent, over 240 beats at 160 × 45,
    /// for each built-in profile and rung and three looks.
    fn fingerprint(make: &dyn Fn(&Profile, Ladder, Look) -> Player) -> Vec<(u64, u64)> {
        let mut out = Vec::new();
        for profile in [Profile::apple_terminal_2_15(), Profile::cautious()] {
            for &ladder in &profile.ladders {
                for (strip, marks, speed) in [
                    (false, false, 10.0),
                    (true, false, 40.0),
                    (false, true, 25.0),
                ] {
                    let look = Look {
                        speed,
                        strip,
                        marks,
                        freeze_ns: None,
                    };
                    let mut p = make(&profile, ladder, look);
                    let (mut len, mut h) = (0u64, 0xcbf2_9ce4_8422_2325_u64);
                    for i in 0..240u64 {
                        let mut bytes = Vec::new();
                        let t = p.frame(i * BEAT_NS, &mut bytes);
                        let tag = t.map_or(0, |t| t.runs + 1);
                        for b in bytes.iter().copied().chain(tag.to_le_bytes()) {
                            h ^= b as u64;
                            h = h.wrapping_mul(0x100_0000_01b3);
                        }
                        len += bytes.len() as u64;
                    }
                    out.push((len, h));
                }
            }
        }
        out
    }

    /// The same fingerprint, taken from the program as it was before the governor joined
    /// (42d11d3).
    const BEFORE: [(u64, u64); 9] = [
        (603483, 12926984624150741138),
        (2382336, 13181727003260668779),
        (1494307, 4400253366655365224),
        (422748, 14582560369731920195),
        (1623421, 12248243313103606850),
        (1022821, 12555154148859019005),
        (512034, 16123272113871259045),
        (2002771, 13776962363268292380),
        (1257994, 13380976804590920510),
    ];

    #[test]
    fn without_the_governor_the_bytes_are_as_before() {
        let made = fingerprint(&|profile, ladder, look| {
            let o = Options {
                govern: false,
                rung: Some(ladder),
                look,
                ..Options::default()
            };
            Maker::new(&o, profile, ladder, None).player(160, 45)
        });
        assert_eq!(made, BEFORE);
    }

    /// Apple's Terminal with the price list fitted to its sweep.
    fn apple() -> Profile {
        priced(&Profile::apple_terminal_2_15())
    }

    /// A profile like `p` with every price multiplied by `by`.
    fn dearer(p: &Profile, by: f64) -> Profile {
        let l = p.prices;
        Profile {
            prices: PriceList {
                frame_ns: l.frame_ns * by,
                byte_ns: l.byte_ns * by,
                cell_ns: l.cell_ns * by,
                run_ns: l.run_ns * by,
                crowding_ns: l.crowding_ns * by,
            },
            ..p.clone()
        }
    }

    fn sixty() -> Budget {
        Budget::per_second(60.0)
    }

    /// A governed player at 160 × 45 with the plan Apple's Terminal gets, judging frames by
    /// `prices`, with the test card behind the computer.
    fn governed(prices: &Profile, look: Look) -> Player {
        let plan = choose(&apple(), sixty(), 160, 45, 1.0);
        let mut p = Player::governed(plan, prices, sixty(), look);
        let (w, h) = plan.ladder.canvas_size(160, 45);
        p.behind(&test_card(w, h, 0));
        p
    }

    /// What one run of `frames` beats did: frames held, and the run limits used.
    struct Run {
        sent: u64,
        held: u64,
        runs: Vec<usize>,
    }

    /// Play `frames` beats, telling the governor that each frame sent took what `surface`
    /// would charge for it, and feeding every frame to a strict terminal.
    fn play(p: &mut Player, surface: &Profile, frames: u64, term: &mut StrictTerminal) -> Run {
        let mut run = Run {
            sent: 0,
            held: 0,
            runs: Vec::new(),
        };
        for i in 0..frames {
            let mut out = Vec::new();
            let made = p.frame(i * BEAT_NS, &mut out);
            run.runs.extend(p.made().max_runs);
            if let Some(tally) = made {
                term.feed(&out)
                    .unwrap_or_else(|v| panic!("frame {i}: {}", v.0));
                run.sent += 1;
                run.held += p.made().held as u64;
                let whole = p.made().whole;
                p.record(&tally, whole.as_ref(), Some(surface.prices.price(&tally)));
            }
        }
        run
    }

    #[test]
    fn a_dear_surface_has_frames_held_and_runs_cut() {
        let dear = dearer(&apple(), 20.0);
        let mut p = governed(&dear, look(false, false));
        let plan = p.plan().unwrap();
        let mut term = StrictTerminal::new(160, 45, &dear);
        let run = play(&mut p, &dear, 240, &mut term);
        let lowest = *run.runs.iter().min().unwrap();
        println!(
            "dear: {} sent, {} held, runs {} -> {lowest} (plan {})",
            run.sent, run.held, run.runs[0], plan.max_runs
        );
        assert!(run.held > 0);
        assert_eq!(run.runs[0], plan.max_runs);
        assert!(lowest < plan.max_runs, "the run limit never fell");
        assert!(p.max_runs().unwrap() < plan.max_runs);
    }

    #[test]
    fn a_cheap_surface_has_nothing_held() {
        let cheap = dearer(&apple(), 0.25);
        let mut p = governed(&cheap, look(true, true));
        let plan = p.plan().unwrap();
        let mut term = StrictTerminal::new(160, 45, &cheap);
        let run = play(&mut p, &cheap, 240, &mut term);
        println!("cheap: {} sent, {} held", run.sent, run.held);
        assert!(run.sent > 30);
        assert_eq!(run.held, 0);
        assert!(run.runs.iter().all(|r| *r == plan.max_runs));
        assert!(looks_alike(term.grid(), p.meant()));
    }

    #[test]
    fn a_held_frame_touches_only_rows_that_moved_or_were_taken() {
        let dear = dearer(&apple(), 20.0);
        // Fast, so that the computer moves on most beats.
        let mut l = look(false, false);
        l.speed = 40.0;
        let mut p = governed(&dear, l);
        let mut term = StrictTerminal::new(160, 45, &dear);
        let (mut held, mut partial) = (0, 0);
        for i in 0..240u64 {
            let before = term.grid().clone();
            let mut out = Vec::new();
            let Some(tally) = p.frame(i * BEAT_NS, &mut out) else {
                continue;
            };
            term.feed(&out)
                .unwrap_or_else(|v| panic!("frame {i}: {}", v.0));
            let whole = p.made().whole;
            p.record(&tally, whole.as_ref(), Some(dear.prices.price(&tally)));
            let made = p.made();
            // Before the first frame nothing is known to be on screen, so the first frame
            // paints the room everywhere it does not paint the picture.
            if !made.held || i == 0 {
                continue;
            }
            held += 1;
            assert!(made.moving.iter().all(|r| made.taken.contains(r)));
            let after = term.grid();
            for r in 0..45 {
                if row_of(&before, r) != row_of(after, r) {
                    assert!(
                        made.taken.contains(&r),
                        "frame {i}: row {r} changed, but only {:?} were sent",
                        made.taken
                    );
                }
            }
            partial += (made.taken.len() < 45) as u32;
        }
        println!("{held} held frames, {partial} of them sent part of the grid");
        assert!(held > 0 && partial > 0);
    }

    #[test]
    fn governed_frames_replay_to_the_grid_meant_once_developing_catches_up() {
        // Frozen early, while still detail is owed, so developing finishes with nothing moving.
        for (by, strip, marks, freeze) in [
            (10.0, false, true, 60),
            (20.0, true, false, 1),
            (40.0, false, false, 0),
            (40.0, false, false, 30),
        ] {
            let dear = dearer(&apple(), by);
            let mut l = look(strip, marks);
            l.speed = 40.0;
            l.freeze_ns = Some(freeze * BEAT_NS);
            let mut p = governed(&dear, l);
            let mut term = StrictTerminal::new(160, 45, &dear);
            let mut frames = Vec::new();
            let mut last_sent = 0;
            let (mut held, mut held_after) = (0, 0);
            for i in 0..3_000u64 {
                if i > last_sent + 120 {
                    // Two seconds with nothing to send: developing has caught up.
                    break;
                }
                let mut out = Vec::new();
                let Some(tally) = p.frame(i * BEAT_NS, &mut out) else {
                    continue;
                };
                term.feed(&out)
                    .unwrap_or_else(|v| panic!("frame {i}: {}", v.0));
                assert!(looks_alike(term.grid(), p.shown().unwrap()), "frame {i}");
                let whole = p.made().whole;
                p.record(&tally, whole.as_ref(), Some(dear.prices.price(&tally)));
                held += p.made().held as u32;
                held_after += (p.made().held && i > freeze) as u32;
                frames.push(out);
                last_sent = i;
            }
            println!(
                "{by} times dearer, frozen at beat {freeze}: {} frames, {held} held \
                 ({held_after} after the freeze), the last at beat {last_sent}",
                frames.len()
            );
            assert!(held > 0);
            if freeze < 2 {
                assert!(held_after > 0, "nothing was left to develop");
            }
            assert!(last_sent < 2_900, "still developing at the end");
            assert!(looks_alike(term.grid(), p.meant()));
            let end = replay(&dear, 160, 45, &frames).unwrap();
            assert!(looks_alike(&end, p.meant()));
        }
    }

    #[test]
    fn every_size_draws() {
        let profile = Profile::apple_terminal_2_15();
        for (c, r) in [(1, 1), (2, 1), (17, 3), (300, 100)] {
            for ladder in [Ladder::Half, Ladder::Quarter] {
                let mut p = Player::new(c, r, &profile, ladder, look(true, true));
                let mut out = Vec::new();
                assert!(p.frame(0, &mut out).is_some());
                let plan = choose(&apple(), sixty(), c, r, 1.0);
                let mut g =
                    Player::governed(plan, &dearer(&apple(), 50.0), sixty(), look(true, true));
                g.behind(&test_card(7, 3, 1));
                let mut out = Vec::new();
                assert!(g.frame(0, &mut out).is_some());
            }
        }
    }
}
