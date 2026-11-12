//! Plans: which rung, grid, run limit and colour depth fit a surface's budget.
//!
//! A plan is chosen once, from the surface's profile, before any frame is drawn. The
//! governor then holds each frame to it.

use phosphor_kernel::{Budget, ColourDepth, Ladder, Profile, Tally};

/// Share of the budget a plan's worst frame may use. The rest is headroom for prices that
/// were measured a little low.
pub const PLAN_SHARE: f64 = 0.8;

/// Bytes a painted cell costs at most: one block character in UTF-8, plus a cursor step.
const CELL_BYTES: u64 = 4;
/// Bytes one colour change costs at most: ink and paper, each as full colour.
const RUN_BYTES: u64 = 38;

/// How rich a picture this surface carries: the rung, the grid, the most runs a row may
/// hold, and the colour depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub ladder: Ladder,
    pub cols: usize,
    pub rows: usize,
    pub max_runs: usize,
    pub depth: ColourDepth,
}

/// The tally of the worst frame this plan allows: `moving_share` of the rows change, every
/// cell on them is painted, and each holds `max_runs` runs.
pub fn worst_tally(plan: &Plan, moving_share: f64) -> Tally {
    let share = if moving_share.is_nan() {
        1.0
    } else {
        moving_share.clamp(0.0, 1.0)
    };
    let rows = ((plan.rows as f64 * share).ceil() as u64).min(plan.rows as u64);
    let runs_a_row = plan.max_runs as u64;
    let cells = rows * plan.cols as u64;
    let runs = rows * runs_a_row;
    Tally {
        bytes: cells * CELL_BYTES + runs * RUN_BYTES,
        cells,
        runs,
        crowding: rows * runs_a_row * runs_a_row,
    }
}

/// Colour depths no deeper than `most`, deepest first.
fn depths_down_from(most: ColourDepth) -> &'static [ColourDepth] {
    match most {
        ColourDepth::Full => &[
            ColourDepth::Full,
            ColourDepth::Indexed256,
            ColourDepth::Indexed8,
        ],
        ColourDepth::Indexed256 => &[ColourDepth::Indexed256, ColourDepth::Indexed8],
        ColourDepth::Indexed8 => &[ColourDepth::Indexed8],
    }
}

/// The richest plan whose worst frame costs at most 80% of the budget.
///
/// Richer means, in order: a finer rung from the profile, then more runs a row (up to
/// `cols`), then deeper colour (up to the profile's). When nothing fits, the floor is the
/// coarsest rung with one run and the shallowest colour.
pub fn choose(
    profile: &Profile,
    budget: Budget,
    cols: usize,
    rows: usize,
    moving_share: f64,
) -> Plan {
    let limit = budget.frame_ns * PLAN_SHARE;
    let fits = |plan: &Plan| profile.prices.price(&worst_tally(plan, moving_share)) <= limit;
    let ladders: &[Ladder] = if profile.ladders.is_empty() {
        &[Ladder::Half]
    } else {
        &profile.ladders
    };
    let depths = depths_down_from(profile.colour);
    for &ladder in ladders.iter().rev() {
        for max_runs in (1..=cols.max(1)).rev() {
            for &depth in depths {
                let plan = Plan {
                    ladder,
                    cols,
                    rows,
                    max_runs,
                    depth,
                };
                if fits(&plan) {
                    return plan;
                }
            }
        }
    }
    Plan {
        ladder: ladders[0],
        cols,
        rows,
        max_runs: 1,
        depth: ColourDepth::Indexed8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phosphor_kernel::PriceList;

    fn sixty() -> Budget {
        Budget::per_second(60.0)
    }

    fn cheaper(profile: &Profile, by: f64) -> Profile {
        let p = profile.prices;
        Profile {
            prices: PriceList {
                frame_ns: p.frame_ns / by,
                byte_ns: p.byte_ns / by,
                cell_ns: p.cell_ns / by,
                run_ns: p.run_ns / by,
                crowding_ns: p.crowding_ns / by,
            },
            ..profile.clone()
        }
    }

    /// A made-up surface: draws every rung, 256 colours, runs are dear and crowding dearer.
    fn made_up() -> Profile {
        Profile {
            name: "made-up".into(),
            ladders: vec![Ladder::Half, Ladder::Quarter, Ladder::Sixth, Ladder::Eighth],
            colour: ColourDepth::Indexed256,
            prices: PriceList {
                frame_ns: 200_000.0,
                byte_ns: 10.0,
                cell_ns: 5.0,
                run_ns: 3_000.0,
                crowding_ns: 400.0,
            },
            ..Profile::cautious()
        }
    }

    #[test]
    fn worst_tally_follows_the_formula() {
        let plan = Plan {
            ladder: Ladder::Half,
            cols: 160,
            rows: 45,
            max_runs: 20,
            depth: ColourDepth::Full,
        };
        let t = worst_tally(&plan, 0.25);
        // ceil(45 × 0.25) = 12 rows.
        assert_eq!(t.cells, 12 * 160);
        assert_eq!(t.runs, 12 * 20);
        assert_eq!(t.crowding, 12 * 20 * 20);
        assert_eq!(t.bytes, 12 * 160 * 4 + 12 * 20 * 38);
        assert_eq!(worst_tally(&plan, 0.0), Tally::default());
    }

    #[test]
    fn apple_at_160_by_45_all_moving_picks_between_8_and_80_runs() {
        let apple = Profile::apple_terminal_2_15();
        let plan = choose(&apple, sixty(), 160, 45, 1.0);
        assert!((8..=80).contains(&plan.max_runs), "{plan:?}");
        let cost = apple.prices.price(&worst_tally(&plan, 1.0));
        assert!(cost <= sixty().frame_ns * PLAN_SHARE, "{cost}");
        // One more run would not have fitted.
        let richer = Plan {
            max_runs: plan.max_runs + 1,
            ..plan
        };
        assert!(apple.prices.price(&worst_tally(&richer, 1.0)) > sixty().frame_ns * PLAN_SHARE);
        // Apple draws half and quarter; quarter is finer and comes first.
        assert_eq!(plan.ladder, Ladder::Quarter);
        assert_eq!(plan.depth, ColourDepth::Full);
        println!(
            "apple 160×45, all moving: {plan:?}, worst frame {:.2} ms of {:.2} ms",
            cost / 1e6,
            sixty().frame_ns / 1e6
        );
    }

    #[test]
    fn a_cheaper_surface_gets_more_runs() {
        for profile in [Profile::apple_terminal_2_15(), made_up()] {
            let dear = choose(&profile, sixty(), 160, 45, 1.0);
            let cheap = choose(&cheaper(&profile, 10.0), sixty(), 160, 45, 1.0);
            assert!(cheap.max_runs > dear.max_runs, "{dear:?} {cheap:?}");
            println!(
                "{}: {} runs; ten times cheaper: {} runs",
                profile.name, dear.max_runs, cheap.max_runs
            );
        }
    }

    #[test]
    fn fewer_moving_rows_get_more_runs() {
        for profile in [Profile::apple_terminal_2_15(), made_up()] {
            let all = choose(&profile, sixty(), 160, 45, 1.0);
            let quarter = choose(&profile, sixty(), 160, 45, 0.25);
            assert!(quarter.max_runs > all.max_runs, "{all:?} {quarter:?}");
            println!(
                "{}: all rows moving {} runs; a quarter moving {} runs",
                profile.name, all.max_runs, quarter.max_runs
            );
        }
    }

    #[test]
    fn worst_tally_never_falls_as_runs_or_movement_rise() {
        let shares = [0.0, 0.01, 0.1, 0.25, 0.33, 0.5, 0.77, 0.99, 1.0];
        for (cols, rows) in [(160, 45), (80, 24), (1, 1), (1708, 516)] {
            let mut last_by_runs: Option<Tally> = None;
            for max_runs in 1..=cols.min(400) {
                let plan = Plan {
                    ladder: Ladder::Half,
                    cols,
                    rows,
                    max_runs,
                    depth: ColourDepth::Full,
                };
                let mut last_by_share: Option<Tally> = None;
                for share in shares {
                    let t = worst_tally(&plan, share);
                    if let Some(prev) = last_by_share {
                        assert!(no_less(&t, &prev), "share {share}: {prev:?} -> {t:?}");
                    }
                    last_by_share = Some(t);
                }
                let t = worst_tally(&plan, 0.5);
                if let Some(prev) = last_by_runs {
                    assert!(no_less(&t, &prev), "runs {max_runs}: {prev:?} -> {t:?}");
                }
                last_by_runs = Some(t);
            }
        }
    }

    fn no_less(a: &Tally, b: &Tally) -> bool {
        a.bytes >= b.bytes && a.cells >= b.cells && a.runs >= b.runs && a.crowding >= b.crowding
    }

    #[test]
    fn a_made_up_surface_with_every_rung_takes_the_finest() {
        let plan = choose(&made_up(), sixty(), 160, 45, 1.0);
        assert_eq!(plan.ladder, Ladder::Eighth);
        assert_eq!(plan.depth, ColourDepth::Indexed256);
        assert!(plan.max_runs >= 1);
        let cost = made_up().prices.price(&worst_tally(&plan, 1.0));
        assert!(cost <= sixty().frame_ns * PLAN_SHARE);
    }

    #[test]
    fn runs_never_exceed_the_columns() {
        let free = cheaper(&Profile::apple_terminal_2_15(), 1e9);
        let plan = choose(&free, sixty(), 30, 10, 1.0);
        assert_eq!(plan.max_runs, 30);
    }

    #[test]
    fn nothing_fits_gives_the_floor() {
        let dear = cheaper(&made_up(), 1e-6);
        let plan = choose(&dear, sixty(), 160, 45, 1.0);
        assert_eq!(
            plan,
            Plan {
                ladder: Ladder::Half,
                cols: 160,
                rows: 45,
                max_runs: 1,
                depth: ColourDepth::Indexed8,
            }
        );
    }
}
