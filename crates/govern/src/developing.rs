//! Developing: the queue of still detail waiting for spare budget.
//!
//! Motion outranks detail. Every moving row goes on the frame it is marked; still rows wait
//! and go, longest-waiting first, when the budget has room left over.
//!
//! Each row also has a level of detail: the run limit it was last sent with. A large still
//! picture arrives coarse, one run a row, and sharpens as spare budget raises the rows
//! furthest behind, doubling their run limit each time, up to the plan's.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Settled,
    /// Detail waiting since this mark.
    Still(u64),
    Moving,
}

/// Which rows of the grid still owe the surface something.
#[derive(Clone, Debug)]
pub struct Developing {
    rows: Vec<Row>,
    /// The run limit each row was last sent with; 0 for a row never sent.
    levels: Vec<usize>,
    marks: u64,
}

/// The level a row goes up to next: one run for a row never sent, then twice its run limit,
/// never past `most`.
pub fn next_level(level: usize, most: usize) -> usize {
    let most = most.max(1);
    if level == 0 {
        1
    } else {
        level.saturating_mul(2).min(most).max(level)
    }
}

impl Developing {
    pub fn new(rows: usize) -> Developing {
        Developing {
            rows: vec![Row::Settled; rows],
            levels: vec![0; rows],
            marks: 0,
        }
    }

    /// The run limit this row was last sent with: 0 until it is first sent, and for rows
    /// outside the grid.
    pub fn level(&self, row: usize) -> usize {
        self.levels.get(row).copied().unwrap_or(0)
    }

    /// True when every row has been sent with a run limit of at least `most`.
    pub fn developed(&self, most: usize) -> bool {
        self.levels.iter().all(|&l| l >= most.max(1))
    }

    /// The rows to send on this frame and the level each goes at, as `(row, level)`, moving
    /// rows first. Nothing changes until [`Developing::sent`] is told what went.
    ///
    /// Every moving row goes, at the level it was last sent with (one run if never), whatever
    /// the budget. Then what is left of `budget_ns` raises the rows furthest behind, lowest
    /// level first and the top row first among equals, one step at a time: a row never sent
    /// to one run, then doubling, up to `most`. A row may be raised more than once on one
    /// frame. Raising stops at the first step that does not fit.
    ///
    /// `cost(row, level)` is what sending that row at that level would cost on its own.
    pub fn develop(
        &self,
        budget_ns: f64,
        most: usize,
        mut cost: impl FnMut(usize, usize) -> f64,
    ) -> Vec<(usize, usize)> {
        let most = most.max(1);
        let n = self.rows.len();
        let mut planned: Vec<Option<(usize, f64)>> = vec![None; n];
        let mut left = budget_ns;
        let mut moving = Vec::new();
        for r in (0..n).filter(|&r| self.rows[r] == Row::Moving) {
            let level = self.levels[r].max(1);
            let c = cost(r, level).max(0.0);
            planned[r] = Some((level, c));
            left -= c;
            moving.push(r);
        }

        let mut behind: BinaryHeap<Reverse<(usize, usize)>> = (0..n)
            .map(|r| (planned[r].map_or(self.levels[r], |p| p.0), r))
            .filter(|&(level, _)| level < most)
            .map(Reverse)
            .collect();
        while let Some(Reverse((level, r))) = behind.pop() {
            let to = next_level(level, most);
            let c = cost(r, to).max(0.0);
            let more = c - planned[r].map_or(0.0, |p| p.1);
            if more > left {
                break;
            }
            left -= more;
            planned[r] = Some((to, c));
            if to < most {
                behind.push(Reverse((to, r)));
            }
        }

        let mut out: Vec<(usize, usize)> = moving
            .iter()
            .map(|&r| (r, planned[r].map_or(1, |p| p.0)))
            .collect();
        out.extend(
            (0..n)
                .filter(|&r| self.rows[r] != Row::Moving)
                .filter_map(|r| planned[r].map(|p| (r, p.0))),
        );
        out
    }

    /// These rows went at these levels. They are settled and keep the level as theirs.
    /// Rows outside the grid are ignored.
    pub fn sent(&mut self, rows: &[(usize, usize)]) {
        for &(r, level) in rows {
            if r < self.rows.len() {
                self.rows[r] = Row::Settled;
                self.levels[r] = level;
            }
        }
    }

    /// This row has detail waiting. A row already waiting keeps its place in the queue;
    /// a moving row stays moving. Rows outside the grid are ignored.
    pub fn mark_still(&mut self, row: usize) {
        if let Some(r @ Row::Settled) = self.rows.get_mut(row) {
            *r = Row::Still(self.marks);
            self.marks += 1;
        }
    }

    /// This row must go now. Rows outside the grid are ignored.
    pub fn mark_moving(&mut self, row: usize) {
        if let Some(r) = self.rows.get_mut(row) {
            *r = Row::Moving;
        }
    }

    /// The rows to send on this frame: every moving row, then still rows, longest-waiting
    /// first, while each fits in what is left of `budget_ns` at `row_ns` a row. Rows taken
    /// are settled.
    pub fn take(&mut self, budget_ns: f64, row_ns: f64) -> Vec<usize> {
        let mut taken: Vec<usize> = (0..self.rows.len())
            .filter(|&i| self.rows[i] == Row::Moving)
            .collect();
        let row_ns = row_ns.max(0.0);
        let mut left = budget_ns - row_ns * taken.len() as f64;

        let mut waiting: Vec<(u64, usize)> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r {
                Row::Still(since) => Some((*since, i)),
                _ => None,
            })
            .collect();
        waiting.sort_unstable();
        for (_, i) in waiting {
            if row_ns > left {
                break;
            }
            left -= row_ns;
            taken.push(i);
        }

        for &i in &taken {
            self.rows[i] = Row::Settled;
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_rows_go_even_with_no_budget() {
        let mut d = Developing::new(10);
        d.mark_still(1);
        d.mark_moving(7);
        d.mark_moving(3);
        assert_eq!(d.take(0.0, 1_000.0), vec![3, 7]);
        // The still row is still waiting.
        assert_eq!(d.take(1_000.0, 1_000.0), vec![1]);
        assert!(d.take(1e9, 1_000.0).is_empty());
    }

    #[test]
    fn still_rows_go_longest_waiting_first_while_they_fit() {
        let mut d = Developing::new(10);
        for row in [5, 2, 8, 0] {
            d.mark_still(row);
        }
        d.mark_still(5); // already waiting: keeps its place
        assert_eq!(d.take(2_500.0, 1_000.0), vec![5, 2]);
        d.mark_still(1);
        assert_eq!(d.take(2_000.0, 1_000.0), vec![8, 0]);
        assert_eq!(d.take(2_000.0, 1_000.0), vec![1]);
    }

    #[test]
    fn never_returns_still_rows_that_do_not_fit() {
        let mut d = Developing::new(6);
        for row in 0..6 {
            d.mark_still(row);
        }
        d.mark_moving(4);
        // One moving row uses 1,000 of 2,999; one still row fits, a second would not.
        assert_eq!(d.take(2_999.0, 1_000.0), vec![4, 0]);
        // Moving rows already overspend the budget: no still rows.
        let mut d = Developing::new(6);
        d.mark_still(0);
        d.mark_moving(1);
        d.mark_moving(2);
        assert_eq!(d.take(1_500.0, 1_000.0), vec![1, 2]);
        // A still row dearer than the whole budget never goes.
        assert!(d.take(999.0, 1_000.0).is_empty());
        assert_eq!(d.take(1_000.0, 1_000.0), vec![0]);
    }

    #[test]
    fn a_moving_row_outranks_its_waiting_detail() {
        let mut d = Developing::new(4);
        d.mark_still(2);
        d.mark_moving(2);
        d.mark_still(2);
        assert_eq!(d.take(0.0, 1_000.0), vec![2]);
        assert!(d.take(1e9, 1_000.0).is_empty());
    }

    #[test]
    fn rows_outside_the_grid_are_ignored() {
        let mut d = Developing::new(2);
        d.mark_moving(9);
        d.mark_still(9);
        assert!(d.take(1e9, 1.0).is_empty());
    }

    /// Every row costs `per_run` for each run of its level.
    fn by_runs(per_run: f64) -> impl FnMut(usize, usize) -> f64 {
        move |_, level| per_run * level as f64
    }

    #[test]
    fn levels_double_from_one_up_to_the_most() {
        assert_eq!(next_level(0, 40), 1);
        assert_eq!(next_level(1, 40), 2);
        assert_eq!(next_level(16, 40), 32);
        assert_eq!(next_level(32, 40), 40);
        assert_eq!(next_level(40, 40), 40);
        assert_eq!(next_level(0, 0), 1);
        assert_eq!(next_level(usize::MAX, usize::MAX), usize::MAX);
    }

    #[test]
    fn rows_arrive_at_one_run_and_double_until_developed() {
        let mut d = Developing::new(3);
        assert_eq!(d.level(0), 0);
        assert!(!d.developed(8));
        let mut frames = Vec::new();
        while !d.developed(8) {
            // Room for eight runs a frame: one row at the most.
            let pairs = d.develop(8.0, 8, by_runs(1.0));
            assert!(!pairs.is_empty(), "stuck after {frames:?}");
            d.sent(&pairs);
            frames.push(pairs);
        }
        assert_eq!(
            frames,
            vec![
                // All three at one run, all three at two, then the top row on to four.
                vec![(0, 4), (1, 2), (2, 2)],
                vec![(1, 4), (2, 4)],
                vec![(0, 8)],
                vec![(1, 8)],
                vec![(2, 8)],
            ]
        );
        assert!(d.developed(8));
        assert_eq!(d.level(2), 8);
        assert!(d.develop(1e9, 8, by_runs(1.0)).is_empty());
    }

    #[test]
    fn moving_rows_go_first_whatever_the_budget_at_their_own_level() {
        let mut d = Developing::new(4);
        d.sent(&[(0, 4), (1, 2), (2, 1)]);
        d.mark_moving(1);
        d.mark_moving(3);
        // No budget: the moving rows go, row 3 (never sent) at one run, and nothing is raised.
        assert_eq!(d.develop(0.0, 8, by_runs(1.0)), vec![(1, 2), (3, 1)]);
        // Nothing changes until told what went.
        assert_eq!(d.level(3), 0);
        // A moving row can be raised too, paying only the difference.
        assert_eq!(d.develop(3.0, 8, by_runs(1.0)), vec![(1, 2), (3, 1)]);
        assert_eq!(
            d.develop(6.0, 8, by_runs(1.0)),
            vec![(1, 2), (3, 2), (2, 2)]
        );
        d.sent(&[(1, 2), (3, 2), (2, 2)]);
        assert_eq!((d.level(1), d.level(2), d.level(3)), (2, 2, 2));
        assert!(d.develop(1e9, 8, by_runs(1.0)).iter().all(|&(_, l)| l == 8));
    }

    #[test]
    fn raising_stops_at_the_first_step_that_does_not_fit() {
        let mut d = Developing::new(3);
        d.sent(&[(0, 1), (1, 1), (2, 1)]);
        // Row 1 is dear: the step to it does not fit, so row 2 waits behind it.
        let cost = |r: usize, l: usize| if r == 1 { 100.0 } else { l as f64 };
        assert_eq!(d.develop(10.0, 8, cost), vec![(0, 2)]);
    }

    #[test]
    fn developing_rows_never_overspend_the_budget() {
        for budget in [0.0, 1.0, 7.5, 33.0, 1e9] {
            let mut d = Developing::new(17);
            for _ in 0..40 {
                let pairs = d.develop(budget, 64, by_runs(1.0));
                let spent: f64 = pairs.iter().map(|&(_, l)| l as f64).sum();
                assert!(spent <= budget, "{budget}: {pairs:?}");
                d.sent(&pairs);
            }
            assert_eq!(d.developed(64), budget >= 64.0, "{budget}");
        }
    }

    #[test]
    fn take_keeps_the_levels_as_they_were() {
        let mut d = Developing::new(2);
        d.sent(&[(0, 4)]);
        d.mark_still(0);
        assert_eq!(d.take(1e9, 1.0), vec![0]);
        assert_eq!(d.level(0), 4);
        d.sent(&[(9, 4)]);
        assert_eq!(d.level(9), 0);
    }
}
