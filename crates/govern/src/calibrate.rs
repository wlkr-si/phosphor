//! Calibration: fitting a price list to measurements of a real surface.

use phosphor_kernel::{PriceList, Tally};

/// One measured frame: what it contained and how long the surface took to accept it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub tally: Tally,
    pub frame_ns: f64,
}

const COLUMNS: [&str; 5] = ["bytes", "cells", "runs", "crowding", "median_ms"];

/// Read a table of measurements. The first row names the columns; `bytes`, `cells`, `runs`,
/// `crowding` and `median_ms` are used and any others are ignored.
pub fn parse_csv(text: &str) -> Result<Vec<Sample>, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty());
    let (_, header) = lines.next().ok_or("the table is empty")?;
    let names: Vec<&str> = header.split(',').map(str::trim).collect();
    let mut at = [0usize; 5];
    for (slot, col) in at.iter_mut().zip(COLUMNS) {
        *slot = names
            .iter()
            .position(|n| *n == col)
            .ok_or_else(|| format!("missing column `{col}`"))?;
    }
    let mut out = Vec::new();
    for (n, line) in lines {
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let mut v = [0f64; 5];
        for ((slot, &i), col) in v.iter_mut().zip(&at).zip(COLUMNS) {
            let f = fields
                .get(i)
                .ok_or_else(|| format!("line {n}: no value for `{col}`"))?;
            *slot = f
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() && *x >= 0.0)
                .ok_or_else(|| format!("line {n}: `{f}` is not a valid `{col}`"))?;
        }
        out.push(Sample {
            tally: Tally {
                bytes: v[0].round() as u64,
                cells: v[1].round() as u64,
                runs: v[2].round() as u64,
                crowding: v[3].round() as u64,
            },
            frame_ns: v[4] * 1e6,
        });
    }
    Ok(out)
}

fn terms(t: &Tally) -> [f64; 4] {
    [
        t.bytes as f64,
        t.cells as f64,
        t.runs as f64,
        t.crowding as f64,
    ]
}

/// Weighted least squares over the prices still free. Prices not in `free` stay zero.
fn solve(samples: &[Sample], free: &[usize]) -> [f64; 4] {
    let n = free.len();
    let mut out = [0.0; 4];
    if n == 0 {
        return out;
    }
    // Each sample counts by 1 / measured time, and each column is scaled to about 1.
    let mut scale = [1.0f64; 4];
    for &j in free {
        let m = samples
            .iter()
            .map(|s| terms(&s.tally)[j] / s.frame_ns)
            .fold(0.0, f64::max);
        if m > 0.0 {
            scale[j] = m;
        }
    }
    let mut a = vec![vec![0.0; n + 1]; n];
    for s in samples {
        if s.frame_ns <= 0.0 {
            continue;
        }
        let t = terms(&s.tally);
        let row: Vec<f64> = free.iter().map(|&j| t[j] / s.frame_ns / scale[j]).collect();
        for r in 0..n {
            for c in 0..n {
                a[r][c] += row[r] * row[c];
            }
            a[r][n] += row[r]; // target is 1: measured / measured
        }
    }
    // Gaussian elimination with partial pivoting.
    let mut dead = vec![false; n];
    for col in 0..n {
        let p = (col..n)
            .max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))
            .unwrap();
        if a[p][col].abs() < 1e-12 {
            dead[col] = true; // nothing in the data speaks to this price
            continue;
        }
        a.swap(col, p);
        let pivot = a[col].clone();
        for (r, row) in a.iter_mut().enumerate() {
            if r != col {
                let f = row[col] / pivot[col];
                for (x, &y) in row.iter_mut().zip(&pivot).skip(col) {
                    *x -= f * y;
                }
            }
        }
    }
    for (i, &j) in free.iter().enumerate() {
        if !dead[i] {
            out[j] = a[i][n] / a[i][i] / scale[j];
        }
    }
    out
}

/// Least-squares fit of `frame_ns = byte_ns × bytes + cell_ns × cells + run_ns × runs +
/// crowding_ns × crowding`, each sample weighted by 1 / measured time. No price is below zero:
/// one that comes out negative is fixed at zero and the rest are fitted again.
pub fn fit_prices(samples: &[Sample]) -> PriceList {
    // How much of a typical (weighted) sample each price accounts for, per nanosecond of price.
    let weight: Vec<f64> = (0..4)
        .map(|j| {
            samples
                .iter()
                .filter(|s| s.frame_ns > 0.0)
                .map(|s| terms(&s.tally)[j] / s.frame_ns)
                .fold(0.0, f64::max)
        })
        .collect();
    let mut free = vec![0, 1, 2, 3];
    let p = loop {
        let p = solve(samples, &free);
        // Fix the most negative price at zero (by what it would take off a frame) and fit again.
        let worst = free
            .iter()
            .copied()
            .filter(|&j| p[j] < 0.0)
            .min_by(|&a, &b| (p[a] * weight[a]).total_cmp(&(p[b] * weight[b])));
        match worst {
            Some(j) => free.retain(|&k| k != j),
            None => break p,
        }
    };
    PriceList {
        frame_ns: 0.0,
        byte_ns: p[0],
        cell_ns: p[1],
        run_ns: p[2],
        crowding_ns: p[3],
    }
}

/// The largest |predicted − measured| / measured over the samples.
pub fn worst_error(prices: &PriceList, samples: &[Sample]) -> f64 {
    samples
        .iter()
        .map(|s| (prices.price(&s.tally) - s.frame_ns).abs() / s.frame_ns)
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/apple-terminal-first.csv");

    /// A small seeded generator, so the tests give the same numbers everywhere.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        fn between(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next()
        }
    }

    fn tally(rng: &mut Rng) -> Tally {
        // 45 rows, each with its own number of runs, so runs and crowding vary independently.
        let busy = rng.between(2.0, 160.0);
        let (mut runs, mut crowding) = (0.0, 0.0);
        for _ in 0..45 {
            let r = (busy * rng.between(0.0, 2.0)).round().min(160.0);
            runs += r;
            crowding += r * r;
        }
        Tally {
            bytes: rng.between(20_000.0, 250_000.0) as u64,
            cells: rng.between(500.0, 7_200.0) as u64,
            runs: runs as u64,
            crowding: crowding as u64,
        }
    }

    #[test]
    fn noisy_samples_give_back_their_prices() {
        let truth = PriceList {
            frame_ns: 0.0,
            byte_ns: 46.0,
            cell_ns: 300.0,
            run_ns: 1_700.0,
            crowding_ns: 93.0,
        };
        for seed in [0x9E37_79B9_7F4A_7C15, 7, 42, 2026, 99_991] {
            let mut rng = Rng(seed);
            let samples: Vec<Sample> = (0..200)
                .map(|_| {
                    let t = tally(&mut rng);
                    Sample {
                        tally: t,
                        frame_ns: truth.price(&t) * rng.between(0.95, 1.05),
                    }
                })
                .collect();
            let got = fit_prices(&samples);
            for (name, g, w) in [
                ("byte", got.byte_ns, truth.byte_ns),
                ("cell", got.cell_ns, truth.cell_ns),
                ("run", got.run_ns, truth.run_ns),
                ("crowding", got.crowding_ns, truth.crowding_ns),
            ] {
                assert!(
                    (g - w).abs() / w < 0.10,
                    "seed {seed}, {name}_ns: got {g}, wanted {w}"
                );
            }
            assert_eq!(got.frame_ns, 0.0);
        }
    }

    #[test]
    fn fixture_gives_a_sensible_price_list() {
        let samples = parse_csv(FIXTURE).unwrap();
        assert_eq!(samples.len(), 7);
        let prices = fit_prices(&samples);
        assert!(
            (500.0..=5_000.0).contains(&prices.run_ns),
            "run_ns {}",
            prices.run_ns
        );
        let worst = worst_error(&prices, &samples);
        assert!(worst < 0.6, "worst error {worst}");
    }

    #[test]
    fn a_price_that_would_be_negative_is_zero() {
        // Time falls as runs rise: the true run price would be -20.
        let mut rng = Rng(12345);
        let samples: Vec<Sample> = (0..50)
            .map(|_| {
                let t = tally(&mut rng);
                Sample {
                    tally: t,
                    frame_ns: 100.0 * t.bytes as f64 - 20.0 * t.runs as f64,
                }
            })
            .collect();
        let got = fit_prices(&samples);
        for p in [got.byte_ns, got.cell_ns, got.run_ns, got.crowding_ns] {
            assert!(p >= 0.0);
        }
        assert_eq!(got.run_ns, 0.0);
        assert!(got.byte_ns > 0.0);
    }

    #[test]
    fn a_missing_column_is_named() {
        let err = parse_csv("bytes,cells,runs,median_ms\n1,2,3,4\n").unwrap_err();
        assert!(err.contains("crowding"), "{err}");
        let err = parse_csv("bytes,cells,runs,crowding\n1,2,3,4\n").unwrap_err();
        assert!(err.contains("median_ms"), "{err}");
    }

    #[test]
    fn bad_values_are_errors() {
        assert!(parse_csv("bytes,cells,runs,crowding,median_ms\n1,2,x,4,5\n").is_err());
        assert!(parse_csv("bytes,cells,runs,crowding,median_ms\n1,2,3\n").is_err());
        assert!(parse_csv("").is_err());
    }

    #[test]
    fn extra_columns_and_order_do_not_matter() {
        let s = parse_csv("note,median_ms,crowding,runs,cells,bytes\nhi,2,4,3,2,1\n").unwrap();
        assert_eq!(s[0].tally.bytes, 1);
        assert_eq!(s[0].tally.crowding, 4);
        assert_eq!(s[0].frame_ns, 2e6);
    }
}
