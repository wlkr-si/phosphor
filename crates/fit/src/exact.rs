//! Fit a canvas into a grid, and turn a grid back into the dots it shows.
use phosphor_kernel::{colour_distance, Canvas, Cell, Grid, Ladder, Rgb};
use std::collections::HashMap;
use std::sync::OnceLock;

#[path = "tables.rs"]
mod tables;

/// The most dots one cell holds (the eighth rung).
const MAX_DOTS: usize = 8;

/// The block character for each pattern of ink dots. Bit i of the index is dot i, counting
/// left to right, top to bottom. Not used for the half rung, which is handled by hand.
fn table(ladder: Ladder) -> &'static [char] {
    match ladder {
        Ladder::Half => &[' ', '▀', '▄', '█'],
        Ladder::Quarter => &tables::QUAD,
        Ladder::Sixth => &tables::SEXT,
        Ladder::Eighth => &tables::OCT,
    }
}

/// Character to ink pattern, for `unfit`.
fn reverse(ladder: Ladder) -> &'static HashMap<char, usize> {
    static MAPS: OnceLock<[HashMap<char, usize>; 4]> = OnceLock::new();
    let maps = MAPS.get_or_init(|| {
        let build = |l: Ladder| {
            table(l)
                .iter()
                .enumerate()
                .map(|(bits, ch)| (*ch, bits))
                .collect::<HashMap<_, _>>()
        };
        [
            build(Ladder::Half),
            build(Ladder::Quarter),
            build(Ladder::Sixth),
            build(Ladder::Eighth),
        ]
    });
    match ladder {
        Ladder::Half => &maps[0],
        Ladder::Quarter => &maps[1],
        Ladder::Sixth => &maps[2],
        Ladder::Eighth => &maps[3],
    }
}

fn brightness(c: Rgb) -> u32 {
    (c >> 16 & 255) * 2 + (c >> 8 & 255) * 5 + (c & 255)
}

/// A cell showing one colour and nothing else.
fn plain(paper: Rgb) -> Cell {
    Cell {
        ch: ' ',
        ink: 0,
        paper,
    }
}

/// Fill `out` with the cells that best show `canvas` on this rung of the ladder.
/// `canvas` must be exactly `ladder.canvas_size(out.cols, out.rows)` dots.
/// `prev` is the grid shown last frame; use it to keep ink and paper from swapping.
pub fn fit(canvas: &Canvas, ladder: Ladder, prev: Option<&Grid>, out: &mut Grid) {
    let (want_w, want_h) = ladder.canvas_size(out.cols, out.rows);
    assert!(
        canvas.w == want_w && canvas.h == want_h && canvas.dots.len() == want_w * want_h,
        "fit: canvas is {}x{} dots, but {:?} on a {}x{} grid needs exactly {}x{}",
        canvas.w,
        canvas.h,
        ladder,
        out.cols,
        out.rows,
        want_w,
        want_h
    );
    assert!(
        out.cells.len() == out.cols * out.rows,
        "fit: grid holds {} cells, but is {}x{}",
        out.cells.len(),
        out.cols,
        out.rows
    );
    // A previous grid of another size says nothing about this one.
    let prev = prev.filter(|p| p.cols == out.cols && p.rows == out.rows);
    let (dw, dh) = ladder.dots();
    let n = dw * dh;
    for row in 0..out.rows {
        for col in 0..out.cols {
            let mut s = [0 as Rgb; MAX_DOTS];
            for j in 0..dh {
                for i in 0..dw {
                    s[j * dw + i] = canvas.dots[(row * dh + j) * canvas.w + col * dw + i];
                }
            }
            let before = prev.map(|p| p.get(col, row));
            out.set(col, row, fit_cell(&s[..n], ladder, before));
        }
    }
}

fn fit_cell(s: &[Rgb], ladder: Ladder, before: Option<Cell>) -> Cell {
    if ladder == Ladder::Half {
        return if s[0] == s[1] {
            plain(s[0])
        } else {
            Cell {
                ch: '▀',
                ink: s[0],
                paper: s[1],
            }
        };
    }
    let n = s.len();
    let first = s[0];
    // The dot that differs most from the first one.
    let (mut far, mut far_d) = (0, 0);
    for (k, &c) in s.iter().enumerate().skip(1) {
        let d = colour_distance(first, c);
        if d > far_d {
            far_d = d;
            far = k;
        }
    }
    if far_d == 0 {
        return plain(first);
    }
    let (mut a, mut b) = (first, s[far]);
    // Is every dot one of the two? Then the split is exact; otherwise refine by two-means.
    let exact = s.iter().all(|&c| c == a || c == b);
    if !exact {
        for _ in 0..2 {
            let mut sum = [[0u32; 3]; 2];
            let mut count = [0u32; 2];
            for &c in s {
                let w = (colour_distance(c, b) < colour_distance(c, a)) as usize;
                count[w] += 1;
                for (ch, shift) in [16u32, 8, 0].iter().enumerate() {
                    sum[w][ch] += c >> shift & 255;
                }
            }
            let mean = |w: usize, old: Rgb| {
                let part = |ch: usize| sum[w][ch].checked_div(count[w]);
                match (part(0), part(1), part(2)) {
                    (Some(r), Some(g), Some(b)) => (r << 16) | (g << 8) | b,
                    _ => old,
                }
            };
            a = mean(0, a);
            b = mean(1, b);
        }
    }
    // Each dot takes the nearer colour. Dots on `b` are set.
    let mut set = 0usize;
    for (k, &c) in s.iter().enumerate() {
        if colour_distance(c, b) < colour_distance(c, a) {
            set |= 1 << k;
        }
    }
    let full = (1usize << n) - 1;
    if set == 0 {
        return plain(a);
    }
    if set == full {
        return plain(b);
    }
    let (on, off) = (set.count_ones(), n as u32 - set.count_ones());
    // Which colour is paper? Keep last frame's choice when it showed these same two colours.
    let b_is_paper = match before {
        Some(p)
            if p.ch != ' '
                && p.ch != '█'
                && ((p.ink == a && p.paper == b) || (p.ink == b && p.paper == a)) =>
        {
            p.paper == b
        }
        // The colour covering more dots; on a tie, the darker; then the lower value.
        _ => on > off || (on == off && (brightness(b), b) < (brightness(a), a)),
    };
    let (ink, paper, ink_bits) = if b_is_paper {
        (a, b, !set & full)
    } else {
        (b, a, set)
    };
    let ch = table(ladder)[ink_bits];
    debug_assert!(ch != '█' && ch != ' ');
    Cell { ch, ink, paper }
}

/// The dots a terminal would show for this grid: the picture meant.
pub fn unfit(grid: &Grid, ladder: Ladder) -> Canvas {
    let (dw, dh) = ladder.dots();
    let (w, h) = ladder.canvas_size(grid.cols, grid.rows);
    let mut canvas = Canvas::new(w, h, 0);
    let back = reverse(ladder);
    let full = (1usize << (dw * dh)) - 1;
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let cell = grid.get(col, row);
            // A character that is not a block of this rung shows only paper.
            let ink_bits = back.get(&cell.ch).copied().unwrap_or(0) & full;
            for j in 0..dh {
                for i in 0..dw {
                    let colour = if ink_bits >> (j * dw + i) & 1 == 1 {
                        cell.ink
                    } else {
                        cell.paper
                    };
                    canvas.dots[(row * dh + j) * w + col * dw + i] = colour;
                }
            }
        }
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use phosphor_kernel::test_card;

    const RUNGS: [Ladder; 4] = [Ladder::Half, Ladder::Quarter, Ladder::Sixth, Ladder::Eighth];

    /// A small seeded generator, so the tests need no dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn colour(&mut self) -> Rgb {
            (self.next() >> 20) as u32 & 0xffffff
        }
    }

    fn fitted(c: &Canvas, l: Ladder, prev: Option<&Grid>) -> Grid {
        let (dw, dh) = l.dots();
        let mut g = Grid::new(c.w / dw, c.h / dh);
        fit(c, l, prev, &mut g);
        g
    }

    #[test]
    fn tables_are_whole() {
        for l in RUNGS {
            let t = table(l);
            let (dw, dh) = l.dots();
            assert_eq!(t.len(), 1 << (dw * dh));
            assert_eq!(t[0], ' ');
            assert_eq!(*t.last().unwrap(), '█');
            assert_eq!(reverse(l).len(), t.len(), "characters repeat on {l:?}");
        }
    }

    #[test]
    fn two_colours_a_cell_round_trip_on_every_rung() {
        for l in RUNGS {
            for seed in 1..=20u64 {
                let mut rng = Rng(seed * 7919);
                let (cols, rows) = (13, 7);
                let (w, h) = l.canvas_size(cols, rows);
                let (dw, dh) = l.dots();
                let mut c = Canvas::new(w, h, 0);
                for row in 0..rows {
                    for col in 0..cols {
                        let (p, q) = (rng.colour(), rng.colour());
                        for j in 0..dh {
                            for i in 0..dw {
                                let pick = rng.next() % 5;
                                let colour = match pick {
                                    0 => p,
                                    1 | 2 => q,
                                    _ => {
                                        if rng.next() & 1 == 0 {
                                            p
                                        } else {
                                            q
                                        }
                                    }
                                };
                                c.set((col * dw + i) as i64, (row * dh + j) as i64, colour);
                            }
                        }
                    }
                }
                let g = fitted(&c, l, None);
                assert!(g.cells.iter().all(|x| x.ch != '█'), "{l:?} seed {seed}");
                assert_eq!(unfit(&g, l), c, "{l:?} seed {seed}");
                // And again with last frame known.
                let g2 = fitted(&c, l, Some(&g));
                assert_eq!(unfit(&g2, l), c, "{l:?} seed {seed} with prev");
            }
        }
    }

    #[test]
    fn half_rung_round_trips_the_test_card() {
        for s in 0..8 {
            let c = test_card(160, 90, s);
            let g = fitted(&c, Ladder::Half, None);
            assert_eq!(unfit(&g, Ladder::Half), c, "shift {s}");
        }
    }

    #[test]
    fn half_rung_uses_top_dot_as_ink() {
        let mut c = Canvas::new(2, 2, 0x111111);
        c.set(0, 0, 0xaa0000);
        let g = fitted(&c, Ladder::Half, None);
        assert_eq!(
            g.get(0, 0),
            Cell {
                ch: '▀',
                ink: 0xaa0000,
                paper: 0x111111
            }
        );
        assert!(g.get(1, 0).looks_like(&plain(0x111111)));
    }

    #[test]
    fn all_ink_and_all_paper_are_spaces() {
        for l in RUNGS {
            let (w, h) = l.canvas_size(1, 1);
            let g = fitted(&Canvas::new(w, h, 0x336699), l, None);
            assert_eq!(g.get(0, 0), plain(0x336699));
        }
    }

    #[test]
    fn paper_is_the_colour_covering_more_dots_then_the_darker() {
        // Quarter: three dots light, one dark: the light colour is paper.
        let mut c = Canvas::new(2, 2, 0xcccccc);
        c.set(1, 1, 0x222222);
        let g = fitted(&c, Ladder::Quarter, None);
        assert_eq!(g.get(0, 0).paper, 0xcccccc);
        assert_eq!(g.get(0, 0).ink, 0x222222);
        // Tie: the darker is paper.
        let mut c = Canvas::new(2, 2, 0xcccccc);
        c.set(0, 0, 0x222222);
        c.set(1, 0, 0x222222);
        let g = fitted(&c, Ladder::Quarter, None);
        assert_eq!(g.get(0, 0).paper, 0x222222);
        assert_eq!(g.get(0, 0).ink, 0xcccccc);
    }

    #[test]
    fn a_sliding_edge_never_swaps_ink_and_paper() {
        let (a, b) = (0x203040, 0xe0d0c0);
        for l in [Ladder::Quarter, Ladder::Sixth, Ladder::Eighth] {
            let (cols, rows) = (20, 2);
            let (w, h) = l.canvas_size(cols, rows);
            assert_eq!(w, 40);
            let frame = |edge: usize| {
                let mut c = Canvas::new(w, h, a);
                c.fill_rect(edge as i64, 0, (w - edge) as i64, h as i64, b);
                c
            };
            let mut prev = fitted(&frame(0), l, None);
            for edge in 1..=w {
                let g = fitted(&frame(edge), l, Some(&prev));
                for i in 0..g.cells.len() {
                    let (p, q) = (prev.cells[i], g.cells[i]);
                    if p.ch != ' ' && q.ch != ' ' {
                        // Both held the same two colours.
                        assert_eq!(p.paper, q.paper, "{l:?} edge {edge} cell {i}");
                        assert_eq!(p.ink, q.ink, "{l:?} edge {edge} cell {i}");
                    }
                }
                assert_eq!(unfit(&g, l), frame(edge));
                prev = g;
            }
        }
    }

    #[test]
    fn a_sliding_edge_that_stays_inside_a_wide_cell_keeps_its_choice() {
        // Sixth rung: the edge moves across the cell's 2 columns; counts change 2:4, 4:2.
        let (a, b) = (0x203040, 0xe0d0c0);
        let l = Ladder::Sixth;
        let mut c = Canvas::new(2, 3, a);
        c.set(1, 0, b);
        c.set(1, 1, b);
        let g1 = fitted(&c, l, None);
        // Now three of six dots are b: a tie, where the rule alone would pick the darker.
        c.set(1, 2, b);
        c.set(0, 0, b);
        let g2 = fitted(&c, l, Some(&g1));
        assert_eq!(g1.get(0, 0).paper, g2.get(0, 0).paper);
    }

    #[test]
    fn three_colours_come_out_as_two() {
        let mut c = Canvas::new(2, 4, 0xff0000);
        c.set(0, 0, 0x00ff00);
        c.set(1, 0, 0x00ff00);
        c.set(0, 1, 0x0000ff);
        c.set(1, 1, 0x0000ff);
        let g = fitted(&c, Ladder::Eighth, None);
        let cell = g.get(0, 0);
        assert!(cell.ch != ' ' && cell.ch != '█');
        let shown = unfit(&g, Ladder::Eighth);
        let mut colours: Vec<Rgb> = shown.dots.clone();
        colours.sort_unstable();
        colours.dedup();
        assert_eq!(colours.len(), 2);
        // Each dot has the nearer of the two colours.
        for (k, &d) in shown.dots.iter().enumerate() {
            let other = if d == colours[0] {
                colours[1]
            } else {
                colours[0]
            };
            assert!(colour_distance(c.dots[k], d) <= colour_distance(c.dots[k], other));
        }
    }

    #[test]
    fn no_cell_is_a_full_block() {
        let mut rng = Rng(42);
        for l in RUNGS {
            let (w, h) = l.canvas_size(30, 10);
            let mut c = Canvas::new(w, h, 0);
            for d in c.dots.iter_mut() {
                *d = rng.colour() & 0xe0e0e0;
            }
            assert!(fitted(&c, l, None).cells.iter().all(|x| x.ch != '█'));
        }
    }

    #[test]
    #[should_panic(expected = "needs exactly")]
    fn wrong_canvas_size_panics_clearly() {
        let c = Canvas::new(3, 3, 0);
        let mut g = Grid::new(2, 2);
        fit(&c, Ladder::Quarter, None, &mut g);
    }
}
