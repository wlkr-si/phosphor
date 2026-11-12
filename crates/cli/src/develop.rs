//! One frame of `phosphor play develop`: a still picture far larger than one frame can
//! carry arrives coarse and sharpens, while a small white square glides across it on the
//! beat.
//!
//! Every row of the grid has a level of detail: the run limit it was last sent with. A row
//! is first sent at one run. Each frame the moving rows go first, at their own level, and
//! what is left of the budget raises the rows furthest behind, doubling their run limit each
//! time, until every row reaches the plan's. Only the half rung is drawn.
//!
//! Plain calculation, like `frame`: nothing here touches a terminal.

use crate::frame::{paint_marks, paint_strip, Look};
use phosphor_encode::encode;
use phosphor_fit::exact::fit;
use phosphor_fit::thrift::thrift;
use phosphor_foundry::picture::scale_to_fit;
use phosphor_govern::developing::Developing;
use phosphor_govern::governor::{Governor, Verdict};
use phosphor_govern::plan::{choose, Plan};
use phosphor_kernel::{Budget, Canvas, Cell, Grid, Ladder, Profile, Rgb, Tally};
use std::collections::HashMap;

/// What the picture sits on where it does not fill the grid.
pub const BLACK: Rgb = 0x000000;
const WHITE: Rgb = 0xFFFFFF;
/// Stands for a cell whose look is unknown, so that encode paints over it.
const UNKNOWN: Cell = Cell {
    ch: '\0',
    ink: 0,
    paper: 0,
};
/// A frame built to the budget that still comes out dearer is built again to this share of
/// the budget it was built to.
const SHRINK: f64 = 0.7;
/// Builds of one frame before only the moving rows go.
const TRIES: usize = 8;

/// The side of the white square, in dots, on a grid of `cols` × `rows` on the half rung: a
/// twentieth of the picture's height, even, at least 4, and never more than the grid.
pub fn thing_size(cols: usize, rows: usize) -> usize {
    let h = rows * 2;
    (h / 20).max(4).min(cols).min(h) & !1
}

/// The plan for developing a picture on a grid of `cols` × `rows`: the half rung, and a run
/// limit at which the rows that move on a frame, and one more row raised to that limit, fit
/// the plan's share of the budget.
pub fn plan(
    profile: &Profile,
    budget: Budget,
    cols: usize,
    rows: usize,
    thing: bool,
    strip: bool,
) -> Plan {
    let half = Profile {
        ladders: vec![Ladder::Half],
        ..profile.clone()
    };
    let thing_rows = if thing { thing_size(cols, rows) / 2 } else { 0 };
    let moving = thing_rows + 1 + strip as usize;
    choose(
        &half,
        budget,
        cols,
        rows,
        moving as f64 / rows.max(1) as f64,
    )
}

/// The picture as the grid shows it: scaled to fit `cols` × `rows` on the half rung, keeping
/// its shape, and centred on black.
pub fn picture_for(src: &Canvas, cols: usize, rows: usize) -> Canvas {
    let (w, h) = Ladder::Half.canvas_size(cols, rows);
    let mut out = Canvas::new(w, h, BLACK);
    if w == 0 || h == 0 {
        return out;
    }
    // A half-rung dot is about as tall as it is wide.
    let scaled = scale_to_fit(src, w, h, 1.0);
    let (x0, y0) = ((w - scaled.w) / 2, (h - scaled.h) / 2);
    for y in 0..scaled.h {
        let d = (y0 + y) * w + x0;
        out.dots[d..d + scaled.w].copy_from_slice(&scaled.dots[y * scaled.w..(y + 1) * scaled.w]);
    }
    out
}

/// What became of the last frame made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Made {
    /// The rows sent and the level each went at, moving rows first.
    pub rows: Vec<(usize, usize)>,
    /// The rows that had to go because the square moved over them, or the strip changed.
    pub moving: Vec<usize>,
    /// What the governor expected the frame to cost.
    pub predicted_ns: f64,
}

/// How developing went. Part of the report.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Developed {
    /// The run limit every row is raised to: the plan's.
    pub final_level: usize,
    /// Seconds from the first frame until every row reached the final level, if it has.
    pub seconds: Option<f64>,
    /// Frames sent whose predicted cost was over the budget.
    pub frames_over_budget: u64,
}

/// The picture's rows, thrifted to each level as they are asked for.
struct Rows {
    picture: Canvas,
    cols: usize,
    depth: phosphor_kernel::ColourDepth,
    cells: HashMap<(usize, usize), Vec<Cell>>,
}

impl Rows {
    /// Row `r` of the picture, thrifted to `level` runs and fitted.
    fn at(&mut self, r: usize, level: usize) -> &[Cell] {
        let (cols, depth, picture) = (self.cols, self.depth, &self.picture);
        self.cells.entry((r, level)).or_insert_with(|| {
            // Thrift treats each row of cells on its own, so one row thrifted alone is that
            // row of the whole picture thrifted.
            let w = picture.w;
            let mut row = Canvas {
                w,
                h: 2,
                dots: picture.dots[2 * r * w..(2 * r + 2) * w].to_vec(),
            };
            thrift(&mut row, Ladder::Half, level, depth);
            let mut g = Grid::new(cols, 1);
            fit(&row, Ladder::Half, None, &mut g);
            g.cells
        })
    }
}

/// Where the square is: its top-left cell, and its side in cells across and down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Square {
    col: usize,
    row: usize,
    cols: usize,
    rows: usize,
}

impl Square {
    fn covers(&self, r: usize) -> bool {
        (self.row..self.row + self.rows).contains(&r)
    }

    /// Paint the square on row `r`'s cells, if it covers that row.
    fn paint(&self, r: usize, cells: &mut [Cell]) {
        if self.covers(r) {
            let end = (self.col + self.cols).min(cells.len());
            for c in &mut cells[self.col.min(end)..end] {
                *c = Cell {
                    ch: ' ',
                    ink: WHITE,
                    paper: WHITE,
                };
            }
        }
    }
}

/// Triangle wave over `0..=room`: up, then back down.
fn triangle(d: u64, room: usize) -> usize {
    if room == 0 {
        return 0;
    }
    let room = room as u64;
    let m = d % (2 * room);
    (if m > room { 2 * room - m } else { m }) as usize
}

/// Makes the frames of one picture developing on one grid.
pub struct Developer {
    plan: Plan,
    /// The surface, at the plan's colour depth.
    profile: Profile,
    budget: Budget,
    look: Look,
    governor: Governor,
    developing: Developing,
    rows: Rows,
    /// The square's side in dots, when there is one.
    thing: Option<usize>,
    /// The grid the surface was last told to show, on the rows in `known`.
    shown: Grid,
    /// Rows the surface has been sent at least once. The others show who knows what.
    known: Vec<bool>,
    /// Where the square was on the frame last sent.
    last_square: Option<Square>,
    /// Frames sent so far. The next frame's number on the strip.
    sent: u64,
    /// The instant of the first frame made.
    began_ns: Option<u64>,
    /// The instant of the frame on which every row reached the final level.
    developed_ns: Option<u64>,
    over_budget: u64,
    made: Made,
}

impl Developer {
    /// Develop `src`, scaled to the plan's grid, on `profile`, keeping frames inside
    /// `budget` by `profile`'s price list. `thing` adds the gliding square.
    pub fn new(
        plan: Plan,
        profile: &Profile,
        budget: Budget,
        look: Look,
        src: &Canvas,
        thing: bool,
    ) -> Developer {
        let plan = Plan {
            ladder: Ladder::Half,
            ..plan
        };
        let surface = Profile {
            colour: plan.depth,
            ..profile.clone()
        };
        let side = thing_size(plan.cols, plan.rows);
        Developer {
            plan,
            budget,
            look,
            governor: Governor::new(&surface, budget, plan),
            developing: Developing::new(plan.rows),
            rows: Rows {
                picture: picture_for(src, plan.cols, plan.rows),
                cols: plan.cols,
                depth: plan.depth,
                cells: HashMap::new(),
            },
            thing: (thing && side > 0).then_some(side),
            shown: Grid::new(plan.cols, plan.rows),
            known: vec![false; plan.rows],
            last_square: None,
            sent: 0,
            began_ns: None,
            developed_ns: None,
            over_budget: 0,
            made: Made::default(),
            profile: surface,
        }
    }

    pub fn plan(&self) -> Plan {
        self.plan
    }

    pub fn size(&self) -> (usize, usize) {
        (self.plan.cols, self.plan.rows)
    }

    #[cfg(test)]
    pub fn made(&self) -> &Made {
        &self.made
    }

    /// True once every row has reached the final level.
    pub fn developed(&self) -> bool {
        self.developed_ns.is_some()
    }

    /// How developing has gone so far.
    pub fn report(&self) -> Developed {
        Developed {
            final_level: self.plan.max_runs,
            seconds: self
                .developed_ns
                .zip(self.began_ns)
                .map(|(d, b)| d.saturating_sub(b) as f64 / 1e9),
            frames_over_budget: self.over_budget,
        }
    }

    /// The picture as it is scaled onto the grid.
    #[cfg(test)]
    pub fn picture(&self) -> &Canvas {
        &self.rows.picture
    }

    /// The grid the surface was last told to show.
    #[cfg(test)]
    pub fn shown(&self) -> &Grid {
        &self.shown
    }

    /// The level each row was last sent with.
    #[cfg(test)]
    pub fn level(&self, row: usize) -> usize {
        self.developing.level(row)
    }

    /// Tell the governor what a frame sent cost: its tally and, when it was answered, how
    /// long the answer took.
    pub fn record(&mut self, sent: &Tally, took_ns: Option<f64>) {
        self.governor.record(sent, took_ns);
    }

    /// Where the square is at `t_ns`: gliding to and fro across the middle of the picture,
    /// `look.speed` dots a second.
    fn square_at(&self, t_ns: u64) -> Option<Square> {
        let side = self.thing?;
        let (w, h) = Ladder::Half.canvas_size(self.plan.cols, self.plan.rows);
        let d = (t_ns as f64 * self.look.speed / 1e9).floor().max(0.0) as u64;
        Some(Square {
            col: triangle(d, w - side),
            row: (h - side) / 4,
            cols: side,
            rows: side / 2,
        })
    }

    /// The learnt ratio of what frames really take to what the price list says.
    fn ratio(&self) -> f64 {
        let probe = Tally {
            bytes: 1,
            cells: 1,
            runs: 1,
            crowding: 1,
        };
        let listed = self.profile.prices.price(&probe);
        if listed > 0.0 {
            self.governor.predict(&probe) / listed
        } else {
            1.0
        }
    }

    /// Make the frame for instant `t_ns` and append its bytes to `out`. Returns its tally, or
    /// `None` when there is nothing to send. A returned frame counts as sent.
    pub fn frame(&mut self, t_ns: u64, out: &mut Vec<u8>) -> Option<Tally> {
        let t_ns = self.look.freeze_ns.map_or(t_ns, |f| t_ns.min(f));
        self.began_ns.get_or_insert(t_ns);
        self.made = Made::default();
        let rows = self.plan.rows;
        let most = self.plan.max_runs.max(1);

        // The rows the square leaves and enters must go now. Before the first frame nothing
        // is on screen, so its rows go first.
        let square = self.square_at(t_ns);
        let mut moving = vec![false; rows];
        if self.last_square != square || self.sent == 0 {
            for s in [self.last_square, square].into_iter().flatten() {
                for (r, m) in moving.iter_mut().enumerate() {
                    *m |= s.covers(r);
                }
            }
        }
        if !moving.contains(&true) && self.developing.developed(most) {
            return None;
        }
        if self.look.strip && rows > 0 {
            // The strip's number changes on every frame sent.
            moving[0] = true;
        }
        for r in (0..rows).filter(|&r| moving[r]) {
            self.developing.mark_moving(r);
        }

        let ratio = self.ratio();
        let fixed = ratio * self.profile.prices.frame_ns;
        let mut budget_ns = self.budget.frame_ns - fixed;
        let start = out.len();
        let mut tries = 0;
        let (pairs, next, tally) = loop {
            tries += 1;
            if tries == TRIES {
                // Nothing left to try: only the moving rows go.
                budget_ns = f64::NEG_INFINITY;
            }
            let pairs = {
                let (rows_at, shown, known, profile) =
                    (&mut self.rows, &self.shown, &self.known, &self.profile);
                self.developing.develop(budget_ns, most, |r, level| {
                    let mut next = Grid {
                        cols: shown.cols,
                        rows: 1,
                        cells: rows_at.at(r, level).to_vec(),
                    };
                    if let Some(s) = square {
                        s.paint(r, &mut next.cells);
                    }
                    let prev = Grid {
                        cols: shown.cols,
                        rows: 1,
                        cells: if known[r] {
                            shown.cells[r * shown.cols..(r + 1) * shown.cols].to_vec()
                        } else {
                            vec![UNKNOWN; shown.cols]
                        },
                    };
                    let t = encode(Some(&prev), &next, profile, &mut Vec::new());
                    if t.bytes == 0 {
                        0.0
                    } else {
                        ratio * (profile.prices.price(&t) - profile.prices.frame_ns)
                    }
                })
            };
            let next = self.next_grid(&pairs, square);
            let prev = self.prev_grid(&pairs, &next);
            out.truncate(start);
            let tally = encode(Some(&prev), &next, &self.profile, out);
            let only_moving = pairs
                .iter()
                .all(|&(r, l)| moving[r] && l == self.developing.level(r).max(1));
            if self.governor.admit(&tally) == Verdict::Send || only_moving || tries >= TRIES {
                break (pairs, next, tally);
            }
            budget_ns *= SHRINK;
        };

        self.developing.sent(&pairs);
        for &(r, _) in &pairs {
            self.known[r] = true;
        }
        self.shown = next;
        self.last_square = square;
        if self.developed_ns.is_none() && self.developing.developed(most) {
            self.developed_ns = Some(t_ns);
        }
        self.made = Made {
            moving: (0..rows).filter(|&r| moving[r]).collect(),
            rows: pairs,
            predicted_ns: self.governor.predict(&tally),
        };
        if out.len() == start {
            return None;
        }
        if self.made.predicted_ns > self.budget.frame_ns {
            self.over_budget += 1;
        }
        self.sent += 1;
        Some(tally)
    }

    /// The grid meant: what is shown, with the rows sent at their levels, the square on
    /// top, and the strip and marks when asked for.
    fn next_grid(&mut self, pairs: &[(usize, usize)], square: Option<Square>) -> Grid {
        let cols = self.plan.cols;
        let mut next = self.shown.clone();
        for &(r, level) in pairs {
            let row = &mut next.cells[r * cols..(r + 1) * cols];
            row.copy_from_slice(self.rows.at(r, level));
            if let Some(s) = square {
                s.paint(r, row);
            }
        }
        if self.look.marks {
            paint_marks(&mut next);
        }
        if self.look.strip {
            paint_strip(&mut next, self.sent);
        }
        next
    }

    /// What to encode against: the grid shown, except that a row never sent is unknown when
    /// it goes now, and is left alone when it does not.
    fn prev_grid(&self, pairs: &[(usize, usize)], next: &Grid) -> Grid {
        let cols = self.plan.cols;
        let mut prev = self.shown.clone();
        for r in (0..self.plan.rows).filter(|&r| !self.known[r]) {
            let row = &mut prev.cells[r * cols..(r + 1) * cols];
            if pairs.iter().any(|&(p, _)| p == r) {
                row.fill(UNKNOWN);
            } else {
                row.copy_from_slice(&next.cells[r * cols..(r + 1) * cols]);
            }
        }
        prev
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::looks_alike;
    use crate::play::priced;
    use phosphor_bench::{replay, StrictTerminal};
    use phosphor_kernel::{test_card, Capabilities, ColourDepth, PriceList};

    const BEAT_NS: u64 = 16_666_667;
    const COLS: usize = 120;
    const ROWS: usize = 40;

    fn sixty() -> Budget {
        Budget::per_second(60.0)
    }

    /// A made-up surface: runs are dear, crowding dearer, and every frame costs 0.1 ms.
    fn made_up() -> Profile {
        Profile {
            name: "made-up".into(),
            ladders: vec![Ladder::Half],
            colour: ColourDepth::Full,
            can: Capabilities {
                repeat_blocks: false,
                erase: true,
                relative_moves: true,
                atomic_frames: false,
            },
            prices: PriceList {
                frame_ns: 100_000.0,
                byte_ns: 10.0,
                cell_ns: 100.0,
                run_ns: 60_000.0,
                crowding_ns: 200.0,
            },
            paint_hz: 60.0,
        }
    }

    fn look(strip: bool, marks: bool) -> Look {
        Look {
            speed: 60.0,
            strip,
            marks,
            freeze_ns: None,
        }
    }

    /// The picture: a test card, which is busy in every row.
    fn card() -> Canvas {
        test_card(2 * COLS, 4 * ROWS, 0)
    }

    /// The whole picture at the plan's run limit, as fit leaves it.
    fn fully_thrifted(d: &Developer) -> Grid {
        let plan = d.plan();
        let mut picture = d.picture().clone();
        thrift(&mut picture, Ladder::Half, plan.max_runs, plan.depth);
        let mut g = Grid::new(plan.cols, plan.rows);
        fit(&picture, Ladder::Half, None, &mut g);
        g
    }

    /// One frame's worth of what happened.
    struct Beat {
        index: u64,
        sent: bool,
        made: Made,
        developed: bool,
    }

    /// Play beats until every row is developed and two more seconds pass, feeding every frame
    /// sent to a strict terminal and telling the governor it took what `surface` charges.
    /// Returns every beat and every frame's bytes.
    fn play(
        d: &mut Developer,
        surface: &Profile,
        term: &mut StrictTerminal,
    ) -> (Vec<Beat>, Vec<Vec<u8>>) {
        let (mut beats, mut frames) = (Vec::new(), Vec::new());
        let mut done_at = None;
        for i in 0..6_000u64 {
            if done_at.is_some_and(|d| i > d + 120) {
                break;
            }
            let mut out = Vec::new();
            let made = d.frame(i * BEAT_NS, &mut out);
            if let Some(tally) = made {
                assert_eq!(tally.bytes, out.len() as u64);
                term.feed(&out)
                    .unwrap_or_else(|v| panic!("frame {i}: {}", v.0));
                d.record(&tally, Some(surface.prices.price(&tally)));
                frames.push(out);
            } else {
                assert!(out.is_empty(), "beat {i} wrote bytes it did not send");
            }
            // Every row sent shows on the terminal as meant.
            for r in (0..d.plan().rows).filter(|&r| d.level(r) > 0) {
                let cols = d.plan().cols;
                let row = r * cols..(r + 1) * cols;
                let alike = term.grid().cells[row.clone()]
                    .iter()
                    .zip(&d.shown().cells[row])
                    .all(|(a, b)| a.looks_like(b));
                assert!(alike, "beat {i}: row {r} shows another picture");
            }
            if d.developed() && done_at.is_none() {
                done_at = Some(i);
            }
            beats.push(Beat {
                index: i,
                sent: made.is_some(),
                made: d.made().clone(),
                developed: d.developed(),
            });
        }
        assert!(done_at.is_some(), "never finished developing");
        (beats, frames)
    }

    #[test]
    fn the_picture_sharpens_while_the_square_moves_on_every_frame() {
        let profile = made_up();
        let plan = plan(&profile, sixty(), COLS, ROWS, true, false);
        let mut d = Developer::new(plan, &profile, sixty(), look(false, false), &card(), true);
        // The finished picture is far more than one frame can carry.
        let whole = encode(None, &fully_thrifted(&d), &profile, &mut Vec::new());
        let frames_whole = profile.prices.price(&whole) / sixty().frame_ns;
        assert!(frames_whole > 8.0, "{frames_whole}");

        let mut term = StrictTerminal::new(COLS, ROWS, &profile);
        let (beats, _) = play(&mut d, &profile, &mut term);
        let developing: Vec<&Beat> = beats.iter().take_while(|b| !b.developed).collect();
        let mut worst: f64 = 0.0;
        for b in beats.iter().take(developing.len() + 1) {
            // The square moves a dot every beat, so a frame goes out on every beat, carrying
            // every row the square left or entered.
            assert!(b.sent, "beat {}: nothing sent", b.index);
            assert!(!b.made.moving.is_empty(), "beat {}", b.index);
            for r in &b.made.moving {
                assert!(
                    b.made.rows.iter().any(|(s, _)| s == r),
                    "beat {}: moving row {r} did not go",
                    b.index
                );
            }
            assert!(
                b.made.predicted_ns <= sixty().frame_ns,
                "beat {}: predicted {:.2} ms",
                b.index,
                b.made.predicted_ns / 1e6
            );
            worst = worst.max(b.made.predicted_ns);
        }
        for r in 0..ROWS {
            assert_eq!(d.level(r), plan.max_runs, "row {r}");
        }
        let report = d.report();
        assert_eq!(report.frames_over_budget, 0);
        assert_eq!(report.final_level, plan.max_runs);
        let seconds = report.seconds.unwrap();
        assert!((seconds - developing.len() as f64 / 60.0).abs() < 0.02);
        // The square kept moving after the picture finished.
        assert!(beats.iter().filter(|b| b.developed).all(|b| b.sent));
        println!(
            "made-up {COLS}x{ROWS}, final level {} runs: the whole picture would take {:.1} \
             frames; developed in {} frames, {seconds:.3} s; worst frame predicted {:.2} ms of \
             {:.2} ms; {} frames over budget",
            plan.max_runs,
            frames_whole,
            developing.len() + 1,
            worst / 1e6,
            sixty().frame_ns / 1e6,
            report.frames_over_budget
        );
    }

    #[test]
    fn a_bigger_budget_finishes_sooner() {
        let profile = made_up();
        let smallest = Budget { frame_ns: 4e6 };
        // One plan, so that every budget develops the same picture to the same level.
        let plan = plan(&profile, smallest, COLS, ROWS, true, false);
        let mut last = f64::INFINITY;
        for ms in [4.0, 8.0, 12.0, 16.7] {
            let budget = Budget { frame_ns: ms * 1e6 };
            let mut d = Developer::new(plan, &profile, budget, look(false, false), &card(), true);
            let mut term = StrictTerminal::new(COLS, ROWS, &profile);
            play(&mut d, &profile, &mut term);
            let r = d.report();
            let seconds = r.seconds.unwrap();
            println!(
                "budget {ms} ms: developed to {} runs in {seconds:.3} s, {} frames over budget",
                r.final_level, r.frames_over_budget
            );
            assert_eq!(r.frames_over_budget, 0);
            assert!(
                seconds < last,
                "{ms} ms took {seconds} s, not under {last} s"
            );
            last = seconds;
        }
    }

    #[test]
    fn the_final_grid_is_the_fully_thrifted_picture() {
        for profile in [made_up(), priced(&Profile::apple_terminal_2_15())] {
            let plan = plan(&profile, sixty(), COLS, ROWS, false, false);
            let mut d = Developer::new(plan, &profile, sixty(), look(false, false), &card(), false);
            let mut term = StrictTerminal::new(COLS, ROWS, &profile);
            let (beats, frames) = play(&mut d, &profile, &mut term);
            let meant = fully_thrifted(&d);
            assert!(looks_alike(term.grid(), &meant), "{}", profile.name);
            let end = replay(&profile, COLS, ROWS, &frames).unwrap();
            assert!(looks_alike(&end, &meant), "{}", profile.name);
            // Once developed, with nothing moving, nothing more is sent.
            assert!(beats
                .iter()
                .filter(|b| b.developed)
                .skip(1)
                .all(|b| !b.sent));
            println!(
                "{}: {} frames to the final level of {} runs",
                profile.name,
                frames.len(),
                plan.max_runs
            );
        }
    }

    #[test]
    fn with_the_square_strip_and_marks_the_rest_is_the_fully_thrifted_picture() {
        let profile = made_up();
        let plan = plan(&profile, sixty(), COLS, ROWS, true, true);
        let mut d = Developer::new(plan, &profile, sixty(), look(true, true), &card(), true);
        let mut term = StrictTerminal::new(COLS, ROWS, &profile);
        let (beats, frames) = play(&mut d, &profile, &mut term);
        assert_eq!(d.report().frames_over_budget, 0);
        let meant = fully_thrifted(&d);
        let end = replay(&profile, COLS, ROWS, &frames).unwrap();
        let square = d.square_at(beats.last().unwrap().index * BEAT_NS).unwrap();
        for r in (1..ROWS - 1).filter(|&r| !square.covers(r)) {
            let row = r * COLS..(r + 1) * COLS;
            let alike = end.cells[row.clone()]
                .iter()
                .zip(&meant.cells[row])
                .all(|(a, b)| a.looks_like(b));
            assert!(alike, "row {r}");
        }
        for r in square.row..square.row + square.rows {
            for c in square.col..square.col + square.cols {
                assert_eq!(end.get(c, r).paper, WHITE);
            }
        }
        assert_eq!(end.get(0, ROWS - 1).paper, crate::frame::MARK);
    }

    #[test]
    fn apples_terminal_develops_inside_the_budget() {
        let apple = priced(&Profile::apple_terminal_2_15());
        let (cols, rows) = (160, 45);
        let plan = plan(&apple, sixty(), cols, rows, true, false);
        let src = test_card(2 * cols, 4 * rows, 0);
        let mut d = Developer::new(plan, &apple, sixty(), look(false, false), &src, true);
        let whole = encode(None, &fully_thrifted(&d), &apple, &mut Vec::new());
        let mut term = StrictTerminal::new(cols, rows, &apple);
        play(&mut d, &apple, &mut term);
        let r = d.report();
        println!(
            "apple {cols}x{rows} (fitted prices): final level {} runs, the whole picture would \
             take {:.1} ms; developed in {:.3} s, {} frames over budget",
            r.final_level,
            apple.prices.price(&whole) / 1e6,
            r.seconds.unwrap(),
            r.frames_over_budget
        );
        assert_eq!(r.frames_over_budget, 0);
    }

    #[test]
    fn tiny_grids_develop() {
        let profile = made_up();
        for (c, r) in [(1, 1), (2, 1), (5, 3), (17, 2)] {
            for thing in [false, true] {
                let plan = plan(&profile, sixty(), c, r, thing, true);
                let mut d =
                    Developer::new(plan, &profile, sixty(), look(true, true), &card(), thing);
                let mut term = StrictTerminal::new(c, r, &profile);
                play(&mut d, &profile, &mut term);
            }
        }
    }

    #[test]
    fn the_picture_is_centred_on_black_keeping_its_shape() {
        // Twice as wide as tall, onto a square of dots: black bands above and below.
        let src = Canvas::new(20, 10, 0x336699);
        let p = picture_for(&src, 10, 5);
        assert_eq!((p.w, p.h), (10, 10));
        assert_eq!(p.get(5, 0), Some(BLACK));
        assert_eq!(p.get(5, 5), Some(0x336699));
        assert_eq!(p.get(5, 9), Some(BLACK));
        assert_eq!(thing_size(160, 45), 4);
        assert_eq!(thing_size(400, 120), 12);
        assert_eq!(thing_size(1, 1), 0);
        assert_eq!(thing_size(3, 40), 2);
    }
}
