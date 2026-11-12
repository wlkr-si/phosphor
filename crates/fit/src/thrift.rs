//! Make a canvas cheaper to show by simplifying its colours.
//!
//! A **run** ends wherever, walking a row of cells left to right, the ink or the paper
//! differs from the cell before. A space's ink does not count. Each cell's ink and paper are
//! decided the way `exact::fit` decides them: on the half rung ink is the top dot and paper
//! the bottom one; on other rungs paper is the colour covering more dots, the darker on a
//! tie. A cell of one colour is a space with that paper.
use phosphor_kernel::{colour_distance, Canvas, ColourDepth, Ladder, Rgb};

/// The largest colour error there can be: the distance from black to white.
const WORST: f64 = 441.7;

/// The sixteen basic colours of the standard 256-colour table, as xterm draws them.
const BASIC: [Rgb; 16] = [
    0x000000, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080, 0xc0c0c0, 0x808080,
    0xff0000, 0x00ff00, 0xffff00, 0x0000ff, 0xff00ff, 0x00ffff, 0xffffff,
];

/// The six levels of each channel in the 6 x 6 x 6 colour cube.
const CUBE: [u32; 6] = [0, 95, 135, 175, 215, 255];

/// The colour nearest `colour` that exists at `depth`.
pub fn nearest(colour: Rgb, depth: ColourDepth) -> Rgb {
    match depth {
        ColourDepth::Full => colour,
        ColourDepth::Indexed8 => {
            let ch = |shift: u32| if colour >> shift & 255 >= 128 { 255 } else { 0 };
            ch(16) << 16 | ch(8) << 8 | ch(0)
        }
        ColourDepth::Indexed256 => {
            let level = |v: u32| {
                *CUBE
                    .iter()
                    .min_by_key(|l| (**l as i32 - v as i32).unsigned_abs())
                    .unwrap()
            };
            let (r, g, b) = (colour >> 16 & 255, colour >> 8 & 255, colour & 255);
            let cube = level(r) << 16 | level(g) << 8 | level(b);
            // The nearest grey is the one nearest the mean of the three channels.
            let mean = (r + g + b) as i32 / 3;
            let n = ((mean - 8 + 5) / 10).clamp(0, 23);
            let greys = [n - 1, n, n + 1]
                .into_iter()
                .filter(|n| (0..24).contains(n))
                .map(|n| (8 + 10 * n) as u32 * 0x010101);
            // Table order (basic, cube, greys) settles ties toward the lower index.
            BASIC
                .iter()
                .copied()
                .chain([cube])
                .chain(greys)
                .min_by_key(|c| colour_distance(colour, *c))
                .unwrap()
        }
    }
}

/// How dark a colour is, the way `exact::fit` breaks a tie between ink and paper.
fn darkness_key(c: Rgb) -> (u32, Rgb) {
    ((c >> 16 & 255) * 2 + (c >> 8 & 255) * 5 + (c & 255), c)
}

/// The rows and columns of cells that cover a canvas, counting part cells at the edges.
fn cells_of(canvas: &Canvas, ladder: Ladder) -> (usize, usize) {
    let (dw, dh) = ladder.dots();
    (canvas.w.div_ceil(dw), canvas.h.div_ceil(dh))
}

/// The indices into `canvas.dots` of one cell's dots, in reading order. Dots past the
/// canvas edge are left out.
fn cell_dots(canvas: &Canvas, ladder: Ladder, col: usize, row: usize, out: &mut Vec<usize>) {
    let (dw, dh) = ladder.dots();
    out.clear();
    for y in row * dh..((row + 1) * dh).min(canvas.h) {
        for x in col * dw..((col + 1) * dw).min(canvas.w) {
            out.push(y * canvas.w + x);
        }
    }
}

/// A cell's ink and paper. Ink is `None` when the cell is a space.
fn cell_pair(colours: &[Rgb], ladder: Ladder) -> (Option<Rgb>, Rgb) {
    if ladder == Ladder::Half {
        return match colours {
            [top, bottom] if top != bottom => (Some(*top), *bottom),
            _ => (None, colours[0]),
        };
    }
    // The two most common colours; on equal counts, the one seen first leads.
    let mut seen: [(Rgb, usize); 8] = [(0, 0); 8];
    let mut kinds = 0;
    for &c in colours {
        match seen[..kinds].iter_mut().find(|(s, _)| *s == c) {
            Some(entry) => entry.1 += 1,
            None => {
                seen[kinds] = (c, 1);
                kinds += 1;
            }
        }
    }
    seen[..kinds].sort_by_key(|s| std::cmp::Reverse(s.1));
    if kinds == 1 {
        return (None, seen[0].0);
    }
    let (a, b) = (seen[0], seen[1]);
    if a.1 > b.1 || darkness_key(a.0) < darkness_key(b.0) {
        (Some(b.0), a.0)
    } else {
        (Some(a.0), b.0)
    }
}

/// Count runs in a row of cells, given each cell's ink and paper.
struct RunCounter {
    runs: usize,
    ink: Option<Rgb>,
    paper: Option<Rgb>,
}

impl RunCounter {
    fn new() -> RunCounter {
        RunCounter {
            runs: 0,
            ink: None,
            paper: None,
        }
    }
    /// Take the next cell. A space keeps the run going if its paper matches; the run's ink
    /// is not settled until a cell with ink arrives.
    fn push(&mut self, (ink, paper): (Option<Rgb>, Rgb)) {
        let same = self.paper == Some(paper)
            && match (ink, self.ink) {
                (None, _) | (_, None) => true,
                (Some(a), Some(b)) => a == b,
            };
        if same {
            self.ink = self.ink.or(ink);
        } else {
            self.runs += 1;
            self.ink = ink;
            self.paper = Some(paper);
        }
    }
}

/// The runs in each row of cells, once `canvas` is fitted on `ladder`.
pub fn count_runs(canvas: &Canvas, ladder: Ladder) -> Vec<usize> {
    let (cols, rows) = cells_of(canvas, ladder);
    let mut idx = Vec::with_capacity(8);
    let mut colours = Vec::with_capacity(8);
    (0..rows)
        .map(|row| {
            let mut counter = RunCounter::new();
            for col in 0..cols {
                cell_dots(canvas, ladder, col, row, &mut idx);
                colours.clear();
                colours.extend(idx.iter().map(|&i| canvas.dots[i]));
                counter.push(cell_pair(&colours, ladder));
            }
            counter.runs
        })
        .collect()
}

/// The dots that will share one colour, summed so that their mean and spread are cheap.
#[derive(Clone, Copy, Default)]
struct Group {
    n: f64,
    sum: [f64; 3],
    /// Sum over dots of the squared channel values.
    sq: f64,
}

impl Group {
    fn add(&mut self, c: Rgb) {
        let ch = [
            (c >> 16 & 255) as f64,
            (c >> 8 & 255) as f64,
            (c & 255) as f64,
        ];
        self.n += 1.0;
        for (sum, v) in self.sum.iter_mut().zip(ch) {
            *sum += v;
            self.sq += v * v;
        }
    }
    fn join(&self, other: &Group) -> Group {
        Group {
            n: self.n + other.n,
            sum: [
                self.sum[0] + other.sum[0],
                self.sum[1] + other.sum[1],
                self.sum[2] + other.sum[2],
            ],
            sq: self.sq + other.sq,
        }
    }
    /// The dot-weighted mean, snapped to `depth`. `None` for an empty group.
    fn colour(&self, depth: ColourDepth) -> Option<Rgb> {
        if self.n == 0.0 {
            return None;
        }
        let ch = |k: usize| (self.sum[k] / self.n).round().clamp(0.0, 255.0) as u32;
        Some(nearest(ch(0) << 16 | ch(1) << 8 | ch(2), depth))
    }
    /// Summed squared distance from every dot to the group's colour.
    fn error(&self, depth: ColourDepth) -> f64 {
        let Some(q) = self.colour(depth) else {
            return 0.0;
        };
        let q = [
            (q >> 16 & 255) as f64,
            (q >> 8 & 255) as f64,
            (q & 255) as f64,
        ];
        let mut e = self.sq;
        for (v, sum) in q.iter().zip(self.sum) {
            e += self.n * v * v - 2.0 * v * sum;
        }
        e.max(0.0)
    }
}

/// A stretch of cells in one row that will share one ink and one paper.
#[derive(Clone, Copy)]
struct Run {
    start: usize,
    end: usize,
    ink: Group,
    paper: Group,
    error: f64,
}

impl Run {
    fn join(&self, next: &Run, depth: ColourDepth) -> Run {
        let ink = self.ink.join(&next.ink);
        let paper = self.paper.join(&next.paper);
        Run {
            start: self.start,
            end: next.end,
            ink,
            paper,
            error: ink.error(depth) + paper.error(depth),
        }
    }
}

/// The error a merge of two neighbouring runs adds.
fn merge_cost(a: &Run, b: &Run, depth: ColourDepth) -> f64 {
    a.join(b, depth).error - a.error - b.error
}

/// Paint one cell of a merged run with the run's ink and paper, so that `exact::fit` gives
/// it exactly that ink and paper (or makes it a space on that paper).
fn paint_cell(
    canvas: &mut Canvas,
    ladder: Ladder,
    idx: &[usize],
    ink: Option<Rgb>,
    paper: Rgb,
    is_ink: &mut Vec<bool>,
) {
    let Some(ink) = ink.filter(|i| *i != paper) else {
        for &i in idx {
            canvas.dots[i] = paper;
        }
        return;
    };
    // Each dot takes whichever of ink and paper its colour was nearer.
    is_ink.clear();
    is_ink.extend(
        idx.iter().map(|&i| {
            colour_distance(canvas.dots[i], ink) < colour_distance(canvas.dots[i], paper)
        }),
    );
    if ladder == Ladder::Half {
        // The bottom dot is always paper on the half rung, and a lone dot is a space.
        match is_ink.get_mut(1) {
            Some(bottom) => *bottom = false,
            None => is_ink.fill(false),
        }
    } else {
        // Paper must cover more dots than ink, or as many when it is the darker.
        let ink_wins_tie = darkness_key(ink) < darkness_key(paper);
        loop {
            let on = is_ink.iter().filter(|b| **b).count();
            let off = is_ink.len() - on;
            if on < off || (on == off && !ink_wins_tie) {
                break;
            }
            // Give paper the ink dot that loses least by the change.
            let k = (0..idx.len())
                .filter(|k| is_ink[*k])
                .min_by_key(|k| {
                    let c = canvas.dots[idx[*k]];
                    colour_distance(c, paper) as i64 - colour_distance(c, ink) as i64
                })
                .unwrap();
            is_ink[k] = false;
        }
    }
    for (k, &i) in idx.iter().enumerate() {
        canvas.dots[i] = if is_ink[k] { ink } else { paper };
    }
}

/// Change colours in place so that, once fitted on `ladder`, no row of cells needs more than
/// `max_runs` colour changes, and every colour exists at `depth`. Shapes are left alone.
/// Returns the mean colour error introduced, 0.0 to 1.0.
pub fn thrift(canvas: &mut Canvas, ladder: Ladder, max_runs: usize, depth: ColourDepth) -> f64 {
    let max_runs = max_runs.max(1);
    let original = canvas.dots.clone();
    let (cols, rows) = cells_of(canvas, ladder);
    let mut idx = Vec::with_capacity(8);
    let mut colours = Vec::with_capacity(8);
    let mut is_ink = Vec::with_capacity(8);
    let mut runs: Vec<Run> = Vec::with_capacity(cols);
    let mut costs: Vec<f64> = Vec::with_capacity(cols);
    for row in 0..rows {
        // One run per cell, its dots split into ink and paper as `fit` would split them.
        runs.clear();
        for col in 0..cols {
            cell_dots(canvas, ladder, col, row, &mut idx);
            colours.clear();
            colours.extend(idx.iter().map(|&i| canvas.dots[i]));
            let (ink, paper) = cell_pair(&colours, ladder);
            let (mut ink_group, mut paper_group) = (Group::default(), Group::default());
            for (k, &c) in colours.iter().enumerate() {
                let to_ink = match ink {
                    None => false,
                    Some(_) if ladder == Ladder::Half => k == 0,
                    Some(i) => c == i || colour_distance(c, i) < colour_distance(c, paper),
                };
                if to_ink {
                    ink_group.add(c)
                } else {
                    paper_group.add(c)
                }
            }
            runs.push(Run {
                start: col,
                end: col + 1,
                ink: ink_group,
                paper: paper_group,
                error: ink_group.error(depth) + paper_group.error(depth),
            });
        }
        // Merge the neighbours that add least error until the row is cheap enough.
        costs.clear();
        costs.extend(runs.windows(2).map(|w| merge_cost(&w[0], &w[1], depth)));
        while runs.len() > max_runs {
            let i = (0..costs.len())
                .min_by(|a, b| costs[*a].total_cmp(&costs[*b]))
                .unwrap();
            runs[i] = runs[i].join(&runs[i + 1], depth);
            runs.remove(i + 1);
            costs.remove(i);
            if i > 0 {
                costs[i - 1] = merge_cost(&runs[i - 1], &runs[i], depth);
            }
            if i < costs.len() {
                costs[i] = merge_cost(&runs[i], &runs[i + 1], depth);
            }
        }
        // Write the result back.
        for run in &runs {
            if run.end - run.start == 1 {
                // A cell left on its own keeps its dots, only brought within `depth`.
                cell_dots(canvas, ladder, run.start, row, &mut idx);
                for &i in &idx {
                    canvas.dots[i] = nearest(canvas.dots[i], depth);
                }
                continue;
            }
            let ink = run.ink.colour(depth);
            let paper = run.paper.colour(depth).or(ink).unwrap_or(0);
            for col in run.start..run.end {
                cell_dots(canvas, ladder, col, row, &mut idx);
                paint_cell(canvas, ladder, &idx, ink, paper, &mut is_ink);
            }
        }
    }
    if original.is_empty() {
        return 0.0;
    }
    let total: f64 = original
        .iter()
        .zip(&canvas.dots)
        .map(|(a, b)| colour_distance(*a, *b) as f64)
        .sum();
    (total / original.len() as f64).sqrt() / WORST
}

#[cfg(test)]
mod tests {
    use super::*;
    use phosphor_kernel::test_card;
    use std::time::Instant;

    const LIMITS: [usize; 4] = [1, 4, 20, 160];

    fn cards() -> [(Canvas, Ladder); 2] {
        [
            (test_card(160, 90, 0), Ladder::Half),
            (test_card(320, 90, 0), Ladder::Quarter),
        ]
    }

    #[test]
    fn nearest_keeps_full_colour() {
        assert_eq!(nearest(0x123456, ColourDepth::Full), 0x123456);
    }

    #[test]
    fn nearest_knows_the_standard_tables() {
        use ColourDepth::*;
        assert_eq!(nearest(0x7f8081, Indexed8), 0x00ffff);
        assert_eq!(nearest(0xf0100f, Indexed8), 0xff0000);
        // Cube entries and greys are their own nearest.
        assert_eq!(nearest(0x5f87af, Indexed256), 0x5f87af);
        assert_eq!(nearest(0x080808, Indexed256), 0x080808);
        assert_eq!(nearest(0xeeeeee, Indexed256), 0xeeeeee);
        // A grey between cube levels lands on the grey ramp.
        assert_eq!(nearest(0x4f4f4f, Indexed256), 0x4e4e4e);
        assert_eq!(nearest(0x5d87b1, Indexed256), 0x5f87af);
        // Every answer is in the table, and the table is closed under `nearest`.
        let mut table: Vec<Rgb> = BASIC.to_vec();
        for r in CUBE {
            for g in CUBE {
                for b in CUBE {
                    table.push(r << 16 | g << 8 | b);
                }
            }
        }
        table.extend((0..24).map(|n| (8 + 10 * n) * 0x010101));
        assert_eq!(table.len(), 256);
        for &c in &table {
            assert_eq!(nearest(c, Indexed256), c, "{c:06x}");
        }
        for c in (0..0x1000000).step_by(9973) {
            let n = nearest(c, Indexed256);
            let best = table.iter().map(|t| colour_distance(c, *t)).min().unwrap();
            assert_eq!(colour_distance(c, n), best, "{c:06x}");
        }
    }

    #[test]
    fn runs_follow_ink_and_paper() {
        // Half rung: (top, bottom). Spaces ignore ink.
        let mut c = Canvas::new(6, 2, 0);
        let tops = [1, 1, 5, 1, 2, 2];
        let bottoms = [9, 9, 9, 9, 9, 2];
        for x in 0..6 {
            c.set(x as i64, 0, tops[x]);
            c.set(x as i64, 1, bottoms[x]);
        }
        // (1,9) (1,9) | (5,9) | (1,9) | (2,9) | space 2
        assert_eq!(count_runs(&c, Ladder::Half), vec![5]);
        // A space on the run's paper does not break it, before or after the ink is known.
        let mut c = Canvas::new(4, 2, 9);
        c.set(1, 0, 1);
        c.set(3, 0, 1);
        assert_eq!(count_runs(&c, Ladder::Half), vec![1]);
        // A flat canvas is one run a row.
        assert_eq!(
            count_runs(&Canvas::new(8, 8, 3), Ladder::Quarter),
            vec![1; 4]
        );
        // The test card's checks are a new run in every cell on the half rung.
        let runs = count_runs(&test_card(160, 90, 0), Ladder::Half);
        assert_eq!(runs.len(), 45);
        assert!(runs.iter().all(|r| *r > 20), "{runs:?}");
    }

    #[test]
    fn thrift_meets_the_limit() {
        for (card, ladder) in cards() {
            for max in LIMITS {
                let mut c = card.clone();
                let err = thrift(&mut c, ladder, max, ColourDepth::Full);
                let runs = count_runs(&c, ladder);
                assert!(
                    runs.iter().all(|r| *r <= max),
                    "{ladder:?} max {max}: {runs:?}"
                );
                assert!((0.0..=1.0).contains(&err));
                println!("{ladder:?} max_runs {max:>3}: error {err:.4}, runs {runs:?}");
            }
        }
    }

    #[test]
    fn a_generous_limit_changes_nothing() {
        for (card, ladder) in cards() {
            let mut c = card.clone();
            let cols = cells_of(&c, ladder).0;
            assert_eq!(thrift(&mut c, ladder, cols, ColourDepth::Full), 0.0);
            assert_eq!(c, card);
        }
    }

    #[test]
    fn two_flat_halves_are_left_alone() {
        for ladder in [Ladder::Half, Ladder::Quarter] {
            for split_across in [false, true] {
                let mut card = Canvas::new(160, 90, 0x3366ff);
                if split_across {
                    card.fill_rect(0, 45, 160, 45, 0xffb000);
                } else {
                    card.fill_rect(80, 0, 80, 90, 0xffb000);
                }
                for max in [2, 3, 20, 160] {
                    let mut c = card.clone();
                    assert_eq!(thrift(&mut c, ladder, max, ColourDepth::Full), 0.0);
                    assert_eq!(c, card, "{ladder:?} max {max}");
                }
            }
        }
    }

    #[test]
    fn more_runs_never_mean_more_error() {
        for (card, ladder) in cards() {
            let mut last = f64::INFINITY;
            for max in 1..=160 {
                let err = thrift(&mut card.clone(), ladder, max, ColourDepth::Full);
                assert!(err <= last, "{ladder:?}: {max} runs {err} > {last}");
                last = err;
            }
        }
    }

    #[test]
    fn indexed_colours_stay_in_the_table() {
        for depth in [ColourDepth::Indexed256, ColourDepth::Indexed8] {
            for (card, ladder) in cards() {
                for max in LIMITS {
                    let mut c = card.clone();
                    thrift(&mut c, ladder, max, depth);
                    assert!(c.dots.iter().all(|d| nearest(*d, depth) == *d));
                    assert!(count_runs(&c, ladder).iter().all(|r| *r <= max));
                }
            }
        }
    }

    #[test]
    fn a_frame_takes_under_ten_milliseconds() {
        let card = test_card(160, 90, 0);
        let mut worst = 0.0f64;
        for depth in [ColourDepth::Full, ColourDepth::Indexed256] {
            for max in LIMITS {
                let mut c = card.clone();
                let t = Instant::now();
                thrift(&mut c, Ladder::Half, max, depth);
                let ms = t.elapsed().as_secs_f64() * 1e3;
                println!("thrift 160 x 90, {depth:?}, max_runs {max:>3}: {ms:.3} ms");
                worst = worst.max(ms);
            }
        }
        if !cfg!(debug_assertions) {
            assert!(worst < 10.0, "{worst} ms");
        }
    }
}
