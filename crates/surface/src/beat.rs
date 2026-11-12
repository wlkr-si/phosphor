//! The beat: even frame timing, and keeping track of acks.
//!
//! A beat is a fixed instant: beat `n` is at `start + n × period`. Frames go out on those
//! instants and never in a burst to catch up. An ack means the surface has read a frame; it
//! does not mean the frame was painted. Some surfaces never answer, so [`Acks`] gives up
//! asking after a run of frames that went unanswered.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Where time comes from, so tests can fake it.
pub trait Clock {
    /// Nanoseconds since some fixed point. Never goes backwards.
    fn now_ns(&self) -> u64;
    /// Return once `now_ns() >= t`.
    fn sleep_until_ns(&self, t: u64);
}

/// Real time. It sleeps; it never spins.
pub struct SystemClock;

fn origin() -> Instant {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    *ORIGIN.get_or_init(Instant::now)
}

impl Clock for SystemClock {
    fn now_ns(&self) -> u64 {
        origin().elapsed().as_nanos() as u64
    }

    fn sleep_until_ns(&self, t: u64) {
        loop {
            let now = self.now_ns();
            if now >= t {
                return;
            }
            std::thread::sleep(Duration::from_nanos(t - now));
        }
    }
}

/// One beat arriving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// Which beat this is, counted from the first (0). Skipped beats still count.
    pub index: u64,
    /// How long after the beat's instant `next` returned. 0 when on time.
    pub late_ns: u64,
    /// Beats that passed unused since the previous tick.
    pub skipped: u64,
}

/// The even timing of frames.
pub struct Beat {
    start_ns: u64,
    period_ns: u64,
    next_index: u64,
}

impl Beat {
    /// Beat 0 is at the instant of this call.
    pub fn new(frames_per_second: f64, clock: &dyn Clock) -> Beat {
        let period = (1e9 / frames_per_second).round();
        let period_ns = if period.is_finite() && period >= 1.0 {
            period as u64
        } else {
            1
        };
        Beat {
            start_ns: clock.now_ns(),
            period_ns,
            next_index: 0,
        }
    }

    /// Wait for the next beat that has not passed. Beats that have already passed are
    /// skipped and counted.
    pub fn next(&mut self, clock: &dyn Clock) -> Tick {
        let elapsed = clock.now_ns().saturating_sub(self.start_ns);
        // The first beat at or after now, and never one already handed out.
        let target = elapsed.div_ceil(self.period_ns).max(self.next_index);
        let at = self.start_ns + target * self.period_ns;
        clock.sleep_until_ns(at);
        let tick = Tick {
            index: target,
            late_ns: clock.now_ns().saturating_sub(at),
            skipped: target - self.next_index,
        };
        self.next_index = target + 1;
        tick
    }
}

/// How quickly the surface has been answering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AckSummary {
    /// Frames answered.
    pub count: u64,
    /// Median time to answer. With an even count, the mean of the two middle times.
    pub median_ns: u64,
    /// 99th percentile (nearest rank).
    pub p99_ns: u64,
    /// Slowest answer.
    pub worst_ns: u64,
    /// Frames that went unanswered.
    pub missed: u64,
}

/// Tracks acks. Only one frame is outstanding at a time.
pub struct Acks {
    give_up_after: u32,
    timeout_ns: u64,
    pending: Option<u64>,
    missed_in_a_row: u32,
    missed: u64,
    times: Vec<u64>,
}

impl Acks {
    /// Stop asking after `give_up_after` frames in a row go unanswered. A frame is
    /// unanswered once `timeout_ns` pass, or once another is sent over it.
    pub fn new(give_up_after: u32, timeout_ns: u64) -> Acks {
        Acks {
            give_up_after,
            timeout_ns,
            pending: None,
            missed_in_a_row: 0,
            missed: 0,
            times: Vec::new(),
        }
    }

    fn miss(&mut self) {
        self.missed += 1;
        self.missed_in_a_row = self.missed_in_a_row.saturating_add(1);
    }

    /// A frame went out at `now_ns`. A frame still outstanding is counted as missed.
    pub fn sent(&mut self, now_ns: u64) {
        if self.pending.take().is_some() {
            self.miss();
        }
        self.pending = Some(now_ns);
    }

    /// The surface read the outstanding frame. An answer with nothing outstanding (one
    /// already counted as missed) is ignored.
    pub fn answered(&mut self, now_ns: u64) {
        if let Some(sent) = self.pending.take() {
            self.times.push(now_ns.saturating_sub(sent));
            self.missed_in_a_row = 0;
        }
    }

    /// False while the last frame is unanswered and not yet timed out. Always true once
    /// `asking` is false.
    pub fn may_send(&mut self, now_ns: u64) -> bool {
        if let Some(sent) = self.pending {
            if now_ns.saturating_sub(sent) >= self.timeout_ns {
                self.pending = None;
                self.miss();
            }
        }
        self.pending.is_none() || !self.asking()
    }

    /// False once `give_up_after` frames in a row went unanswered. An answer starts it again.
    pub fn asking(&self) -> bool {
        self.missed_in_a_row < self.give_up_after
    }

    pub fn summary(&self) -> AckSummary {
        let mut sorted = self.times.clone();
        sorted.sort_unstable();
        let n = sorted.len();
        if n == 0 {
            return AckSummary {
                missed: self.missed,
                ..AckSummary::default()
            };
        }
        let median_ns = if n % 2 == 1 {
            sorted[n / 2]
        } else {
            let (a, b) = (sorted[n / 2 - 1], sorted[n / 2]);
            a / 2 + b / 2 + (a % 2 + b % 2) / 2
        };
        let rank = (n * 99).div_ceil(100).max(1);
        AckSummary {
            count: n as u64,
            median_ns,
            p99_ns: sorted[rank - 1],
            worst_ns: sorted[n - 1],
            missed: self.missed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Time that moves only when told to, or when asked to sleep.
    struct FakeClock {
        now: Cell<u64>,
    }

    impl FakeClock {
        fn new(now: u64) -> FakeClock {
            FakeClock {
                now: Cell::new(now),
            }
        }
        fn advance(&self, ns: u64) {
            self.now.set(self.now.get() + ns);
        }
    }

    impl Clock for FakeClock {
        fn now_ns(&self) -> u64 {
            self.now.get()
        }
        fn sleep_until_ns(&self, t: u64) {
            if t > self.now.get() {
                self.now.set(t);
            }
        }
    }

    const P: u64 = 10_000_000; // 100 frames a second

    #[test]
    fn no_delays_means_even_ticks() {
        let clock = FakeClock::new(5_000);
        let mut beat = Beat::new(100.0, &clock);
        let mut last = None;
        for i in 0..10 {
            let tick = beat.next(&clock);
            assert_eq!(
                tick,
                Tick {
                    index: i,
                    late_ns: 0,
                    skipped: 0
                }
            );
            if let Some(prev) = last {
                assert_eq!(clock.now_ns() - prev, P);
            }
            last = Some(clock.now_ns());
        }
    }

    #[test]
    fn a_stall_skips_beats_and_lands_on_a_beat() {
        let clock = FakeClock::new(1_234);
        let mut beat = Beat::new(100.0, &clock);
        assert_eq!(beat.next(&clock).index, 0);
        assert_eq!(beat.next(&clock).index, 1);
        let at_beat_1 = clock.now_ns();
        clock.advance(P * 5 / 2);
        let tick = beat.next(&clock);
        assert_eq!(
            tick,
            Tick {
                index: 4,
                late_ns: 0,
                skipped: 2
            }
        );
        assert_eq!(clock.now_ns(), at_beat_1 + 3 * P);
        // And the beat carries on from there.
        assert_eq!(
            beat.next(&clock),
            Tick {
                index: 5,
                late_ns: 0,
                skipped: 0
            }
        );
    }

    #[test]
    fn a_wait_of_one_period_is_not_a_skip() {
        let clock = FakeClock::new(0);
        let mut beat = Beat::new(100.0, &clock);
        beat.next(&clock);
        clock.advance(P); // exactly on beat 1
        assert_eq!(
            beat.next(&clock),
            Tick {
                index: 1,
                late_ns: 0,
                skipped: 0
            }
        );
    }

    #[test]
    fn a_stall_exactly_on_a_beat_uses_it() {
        let clock = FakeClock::new(0);
        let mut beat = Beat::new(100.0, &clock);
        beat.next(&clock);
        clock.advance(3 * P);
        assert_eq!(
            beat.next(&clock),
            Tick {
                index: 3,
                late_ns: 0,
                skipped: 2
            }
        );
    }

    #[test]
    fn may_send_follows_the_ack() {
        let mut acks = Acks::new(3, 100);
        assert!(acks.may_send(0));
        acks.sent(0);
        assert!(!acks.may_send(50));
        acks.answered(60);
        assert!(acks.may_send(60));

        acks.sent(1_000);
        assert!(!acks.may_send(1_099));
        assert!(acks.may_send(1_100)); // timed out
        assert_eq!(acks.summary().missed, 1);
    }

    #[test]
    fn gives_up_after_enough_timeouts_in_a_row() {
        let mut acks = Acks::new(3, 100);
        for i in 0..3 {
            assert!(acks.asking());
            let t = i * 1_000;
            acks.sent(t);
            assert!(!acks.may_send(t + 1));
            assert!(acks.may_send(t + 100));
        }
        assert!(!acks.asking());
        // Once it has given up, it always may send, even straight after sending.
        acks.sent(10_000);
        assert!(acks.may_send(10_001));
        assert_eq!(acks.summary().missed, 3);
    }

    #[test]
    fn an_answer_breaks_the_run_of_misses() {
        let mut acks = Acks::new(2, 100);
        acks.sent(0);
        assert!(acks.may_send(100));
        acks.sent(200);
        acks.answered(210);
        acks.sent(300);
        assert!(acks.may_send(400));
        assert!(acks.asking());
    }

    #[test]
    fn summary_of_known_answer_times() {
        let mut acks = Acks::new(3, 1_000_000_000);
        let mut t = 0;
        // Answer times 1 ms, 2 ms ... 100 ms, sent out of order.
        for ms in (1..=100u64).rev() {
            acks.sent(t);
            acks.answered(t + ms * 1_000_000);
            t += 1_000_000_000;
        }
        let s = acks.summary();
        assert_eq!(s.count, 100);
        assert_eq!(s.median_ns, 50_500_000);
        assert_eq!(s.p99_ns, 99_000_000);
        assert_eq!(s.worst_ns, 100_000_000);
        assert_eq!(s.missed, 0);
    }

    #[test]
    fn summary_with_an_odd_count_and_with_none() {
        let mut acks = Acks::new(3, 1_000);
        assert_eq!(acks.summary(), AckSummary::default());
        for (i, d) in [30u64, 10, 20].iter().enumerate() {
            let t = i as u64 * 1_000;
            acks.sent(t);
            acks.answered(t + d);
        }
        let s = acks.summary();
        assert_eq!((s.median_ns, s.p99_ns, s.worst_ns), (20, 30, 30));
    }

    #[test]
    fn the_system_clock_sleeps_to_the_instant() {
        let clock = SystemClock;
        let target = clock.now_ns() + 2_000_000;
        clock.sleep_until_ns(target);
        assert!(clock.now_ns() >= target);
    }
}
