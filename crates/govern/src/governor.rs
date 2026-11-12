//! The governor: admits or holds each frame, and tightens or relaxes thrift as it learns.
//!
//! It prices every tally with the surface's price list, scaled by what the surface has
//! really taken so far, and holds any frame that would run over the budget. When frames
//! keep running over it lowers the run limit; when they keep coming in well under, it
//! raises the limit again, never past the plan's.

use phosphor_kernel::{Budget, PriceList, Profile, Tally};

use crate::plan::Plan;

/// Frames over budget in a row before the run limit is cut.
const OVER_STREAK: u32 = 3;
/// Comfortable frames in a row before the run limit is raised.
const COMFORT_STREAK: u32 = 60;
/// A frame is comfortable when it costs less than this share of the budget.
const COMFORT_SHARE: f64 = 0.6;
/// How much weight each measured frame carries in the running ratio.
const LEARN_RATE: f64 = 0.125;

/// What to do with a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// It fits the budget: send it on the beat.
    Send,
    /// It would run over: send the moving part now and leave the rest to develop.
    Hold,
}

/// Keeps every frame inside the surface's budget.
#[derive(Clone, Debug)]
pub struct Governor {
    prices: PriceList,
    budget: Budget,
    plan_runs: usize,
    max_runs: usize,
    /// Running ratio of measured to listed time.
    ratio: f64,
    over: u32,
    comfortable: u32,
}

impl Governor {
    pub fn new(profile: &Profile, budget: Budget, plan: Plan) -> Governor {
        let plan_runs = plan.max_runs.max(1);
        Governor {
            prices: profile.prices,
            budget,
            plan_runs,
            max_runs: plan_runs,
            ratio: 1.0,
            over: 0,
            comfortable: 0,
        }
    }

    /// The time this tally is expected to take, from the price list and what has been learnt.
    pub fn predict(&self, tally: &Tally) -> f64 {
        self.prices.price(tally) * self.ratio
    }

    /// Send when the predicted cost fits the budget; hold when it would run over.
    pub fn admit(&self, tally: &Tally) -> Verdict {
        if self.predict(tally) > self.budget.frame_ns {
            Verdict::Hold
        } else {
            Verdict::Send
        }
    }

    /// Learn from a frame that was sent. `took_ns` is the time until its ack, when there
    /// was one; without it the prediction stands in for the measurement.
    pub fn record(&mut self, tally: &Tally, took_ns: Option<f64>) {
        let listed = self.prices.price(tally);
        if let Some(took) = took_ns.filter(|t| t.is_finite() && *t >= 0.0) {
            if listed > 0.0 {
                self.ratio += LEARN_RATE * (took / listed - self.ratio);
            }
        }
        let cost = match took_ns.filter(|t| t.is_finite() && *t >= 0.0) {
            Some(took) => took,
            None => listed * self.ratio,
        };

        if cost > self.budget.frame_ns {
            self.comfortable = 0;
            self.over += 1;
            if self.over >= OVER_STREAK {
                self.over = 0;
                self.max_runs = (self.max_runs * 3 / 4).max(1);
            }
        } else if cost < self.budget.frame_ns * COMFORT_SHARE {
            self.over = 0;
            self.comfortable += 1;
            if self.comfortable >= COMFORT_STREAK {
                self.comfortable = 0;
                self.max_runs = (self.max_runs * 11).div_ceil(10).min(self.plan_runs);
            }
        } else {
            self.over = 0;
            self.comfortable = 0;
        }
    }

    /// The run limit to use for the next frame.
    pub fn max_runs(&self) -> usize {
        self.max_runs
    }

    /// The budget every frame is judged by.
    pub fn budget(&self) -> Budget {
        self.budget
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{choose, worst_tally};
    use phosphor_kernel::{ColourDepth, Ladder};

    fn sixty() -> Budget {
        Budget::per_second(60.0)
    }

    fn apple_governor() -> (Governor, Plan) {
        let apple = Profile::apple_terminal_2_15();
        let plan = choose(&apple, sixty(), 160, 45, 1.0);
        (Governor::new(&apple, sixty(), plan), plan)
    }

    /// A made-up surface where one run costs exactly 1 ms and nothing else costs anything.
    fn a_millisecond_a_run() -> Profile {
        Profile {
            name: "made-up".into(),
            prices: PriceList {
                frame_ns: 0.0,
                byte_ns: 0.0,
                cell_ns: 0.0,
                run_ns: 1e6,
                crowding_ns: 0.0,
            },
            ..Profile::cautious()
        }
    }

    fn runs(n: u64) -> Tally {
        Tally {
            bytes: n.max(1),
            cells: n,
            runs: n,
            crowding: n,
        }
    }

    #[test]
    fn admits_the_plans_worst_frame_and_holds_a_dearer_one() {
        let (g, plan) = apple_governor();
        assert_eq!(g.admit(&worst_tally(&plan, 1.0)), Verdict::Send);
        let every_cell = Tally {
            bytes: 250_000,
            cells: 7_200,
            runs: 7_200,
            crowding: 45 * 160 * 160,
        };
        assert_eq!(g.admit(&every_cell), Verdict::Hold);
        assert_eq!(g.admit(&Tally::default()), Verdict::Send);
    }

    #[test]
    fn cuts_runs_after_three_frames_over_budget() {
        let (mut g, plan) = apple_governor();
        let fits = worst_tally(&plan, 1.0);
        g.record(&fits, Some(20e6));
        g.record(&fits, Some(20e6));
        assert_eq!(g.max_runs(), plan.max_runs);
        g.record(&fits, Some(20e6));
        assert_eq!(g.max_runs(), plan.max_runs * 3 / 4);
        assert!(g.max_runs() < plan.max_runs);
    }

    #[test]
    fn an_on_budget_frame_breaks_the_over_streak() {
        let (mut g, plan) = apple_governor();
        let fits = worst_tally(&plan, 1.0);
        g.record(&fits, Some(20e6));
        g.record(&fits, Some(20e6));
        g.record(&fits, Some(14e6));
        g.record(&fits, Some(20e6));
        g.record(&fits, Some(20e6));
        assert_eq!(g.max_runs(), plan.max_runs);
    }

    #[test]
    fn raises_runs_after_sixty_comfortable_frames_but_never_past_the_plan() {
        let (mut g, plan) = apple_governor();
        let fits = worst_tally(&plan, 1.0);
        for _ in 0..3 {
            g.record(&fits, Some(20e6));
        }
        let cut = g.max_runs();
        assert!(cut < plan.max_runs);
        for _ in 0..59 {
            g.record(&Tally::default(), Some(1e6));
        }
        assert_eq!(g.max_runs(), cut);
        g.record(&Tally::default(), Some(1e6));
        assert!(g.max_runs() > cut, "{} -> {}", cut, g.max_runs());
        assert_eq!(g.max_runs(), (cut * 11).div_ceil(10));
        for _ in 0..60 * 20 {
            g.record(&Tally::default(), Some(1e6));
        }
        assert_eq!(g.max_runs(), plan.max_runs);
    }

    #[test]
    fn never_cuts_below_one_run() {
        let profile = a_millisecond_a_run();
        let plan = Plan {
            ladder: Ladder::Half,
            cols: 80,
            rows: 24,
            max_runs: 2,
            depth: ColourDepth::Indexed256,
        };
        let mut g = Governor::new(&profile, sixty(), plan);
        for _ in 0..30 {
            g.record(&runs(40), None);
        }
        assert_eq!(g.max_runs(), 1);
        // From one run, a tenth more is still one more.
        for _ in 0..60 {
            g.record(&runs(1), None);
        }
        assert_eq!(g.max_runs(), 2);
    }

    #[test]
    fn learns_a_surface_slower_than_its_price_list() {
        let profile = a_millisecond_a_run();
        let plan = choose(&profile, sixty(), 80, 1, 1.0);
        let mut g = Governor::new(&profile, sixty(), plan);
        // 12 runs: listed at 12 ms, inside a 16.7 ms budget.
        assert_eq!(g.admit(&runs(12)), Verdict::Send);
        // The surface really takes twice as long.
        for _ in 0..60 {
            g.record(&runs(5), Some(10e6));
        }
        assert!((g.predict(&runs(12)) / 1e6 - 24.0).abs() < 0.5);
        assert_eq!(g.admit(&runs(12)), Verdict::Hold);
        assert_eq!(g.admit(&runs(8)), Verdict::Send);
    }

    #[test]
    fn without_acks_the_prediction_judges_the_frame() {
        let profile = a_millisecond_a_run();
        let plan = Plan {
            ladder: Ladder::Half,
            cols: 80,
            rows: 24,
            max_runs: 40,
            depth: ColourDepth::Indexed256,
        };
        let mut g = Governor::new(&profile, sixty(), plan);
        for _ in 0..3 {
            g.record(&runs(20), None);
        }
        assert_eq!(g.max_runs(), 30);
    }
}
