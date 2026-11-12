//! `phosphor play bounce` and `phosphor play develop`: frames on the beat to a real
//! terminal, until time is up or the viewer quits.

use crate::develop::{self, Developed, Developer};
use crate::frame::{Look, Player};
use phosphor_bounce::ROOM;
use phosphor_foundry::picture::load_png;
use phosphor_govern::calibrate::{fit_prices, parse_csv};
use phosphor_govern::plan::{choose, Plan};
use phosphor_kernel::{Budget, Canvas, ColourDepth, Ladder, PriceList, Profile, Rgb, Tally, Wire};
use phosphor_surface::beat::{AckSummary, Acks, Beat, Clock, SystemClock};
use phosphor_surface::probe::probe;
use phosphor_surface::tty::Tty;
use std::cell::RefCell;
use std::io;
use std::time::Duration;

/// Which profile to draw with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Ask the surface at start-up.
    Probe,
    Apple,
    Cautious,
}

/// What to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    /// The bouncing computer.
    Bounce,
    /// A still picture that sharpens over time.
    Develop,
}

/// Everything `phosphor play` was asked to do.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub what: What,
    /// Develop only: a small white square glides across the picture.
    pub thing: bool,
    pub rung: Option<Ladder>,
    pub look: Look,
    pub seconds: Option<f64>,
    /// The beat asked for with `--fps`. `None` beats at the profile's paint rate.
    pub fps: Option<f64>,
    pub profile: Choice,
    pub report: Option<String>,
    /// Keep every frame inside the budget. Off, frames go out whole, as they did before
    /// the governor joined.
    pub govern: bool,
    /// The share of rows the plan expects to change on a frame, 0.0 to 1.0.
    pub moving_share: f64,
    /// A PNG: put behind the computer, or, to develop, the picture itself.
    pub picture: Option<String>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            what: What::Bounce,
            thing: false,
            rung: None,
            look: Look {
                speed: 10.0,
                strip: false,
                marks: false,
                freeze_ns: None,
            },
            seconds: None,
            fps: None,
            profile: Choice::Probe,
            report: None,
            govern: true,
            moving_share: 1.0,
            picture: None,
        }
    }
}

/// Give up asking for acks after this many frames in a row go unanswered.
const GIVE_UP_AFTER: u32 = 4;
/// A frame not answered in this time counts as unanswered.
const ACK_TIMEOUT: Duration = Duration::from_millis(300);
/// Asks the surface to say where its cursor is. Its answer is the ack.
const ASK: &[u8] = b"\x1b[6n";
/// Back to the surface's own colours.
const PLAIN_COLOURS: &[u8] = b"\x1b[0m";
/// Used when a terminal will not say its size.
const FALLBACK_SIZE: (usize, usize) = (80, 24);
/// Apple's Terminal 2.15, measured: the price sweep in docs/measurements.
const APPLE_PRICES: &str =
    include_str!("../../../docs/measurements/apple-terminal-2.15-prices.csv");

/// Where the beat came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeatFrom {
    /// The profile's paint rate: how often the surface has been seen to paint.
    PaintRate,
    /// `--fps` on the command line.
    Asked,
}

impl BeatFrom {
    fn name(self) -> &'static str {
        match self {
            BeatFrom::PaintRate => "paint rate",
            BeatFrom::Asked => "--fps",
        }
    }
}

/// The beat to play at, and where it came from: `--fps` when it was given, otherwise the
/// profile's paint rate, so no frame is sent faster than the surface is seen to paint.
pub fn choose_beat(asked: Option<f64>, profile: &Profile) -> (f64, BeatFrom) {
    match asked {
        Some(fps) => (fps, BeatFrom::Asked),
        None => (profile.paint_hz, BeatFrom::PaintRate),
    }
}

/// Why the run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Time,
    Quit,
}

/// What a run did. Printed, and written with `--report`, as JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub profile: String,
    pub rung: Ladder,
    pub cols: usize,
    pub rows: usize,
    /// The beat played at, in frames a second.
    pub fps: f64,
    /// Where the beat came from.
    pub fps_from: BeatFrom,
    /// The budget for one frame: 1 / the beat.
    pub budget_ms: f64,
    /// Frames sent.
    pub frames: u64,
    pub seconds: f64,
    /// Frame bytes sent; the ack questions are not counted.
    pub bytes: u64,
    /// Every frame's tally, summed.
    pub tally: Tally,
    pub acks: AckSummary,
    /// Still asking for acks at the end.
    pub asking: bool,
    /// Beats that passed unused because the program was late.
    pub skipped_late: u64,
    /// Beats whose frame was held back waiting for the surface to answer.
    pub skipped_held: u64,
    /// Beats on which the picture had not changed, so nothing was sent.
    pub unchanged: u64,
    /// Times the window changed size and the picture started again.
    pub restarts: u64,
    /// Frames whose answer took longer than the budget.
    pub over_budget: u64,
    /// What the governor did; `None` with `--no-govern`.
    pub governor: Option<Governed>,
    /// How the picture developed; `None` unless developing.
    pub developing: Option<Developed>,
    pub stop: Stop,
}

/// What the governor did over a run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Governed {
    /// The plan chosen, for the grid size played last.
    pub plan: Plan,
    /// Frames the governor held: sent in part, the rest left developing.
    pub held: u64,
    /// The lowest and highest run limits frames were thrifted to.
    pub max_runs_low: usize,
    pub max_runs_high: usize,
}

impl Governed {
    fn new(plan: Plan) -> Governed {
        Governed {
            plan,
            held: 0,
            max_runs_low: plan.max_runs,
            max_runs_high: plan.max_runs,
        }
    }

    fn json(self) -> String {
        format!(
            concat!(
                "{{\"plan\":{{\"rung\":\"{}\",\"cols\":{},\"rows\":{},\"max_runs\":{},",
                "\"depth\":\"{}\"}},\"frames_held\":{},",
                "\"max_runs\":{{\"low\":{},\"high\":{}}}}}"
            ),
            rung_name(self.plan.ladder),
            self.plan.cols,
            self.plan.rows,
            self.plan.max_runs,
            depth_name(self.plan.depth),
            self.held,
            self.max_runs_low,
            self.max_runs_high,
        )
    }
}

pub fn depth_name(d: ColourDepth) -> &'static str {
    match d {
        ColourDepth::Full => "full",
        ColourDepth::Indexed256 => "256",
        ColourDepth::Indexed8 => "8",
    }
}

impl Report {
    pub fn to_json(&self) -> String {
        let ms = |ns: u64| ns as f64 / 1e6;
        let rate = if self.seconds > 0.0 {
            self.frames as f64 / self.seconds
        } else {
            0.0
        };
        format!(
            concat!(
                "{{\"profile\":\"{}\",\"rung\":\"{}\",\"cols\":{},\"rows\":{},\"fps\":{},",
                "\"fps_from\":\"{}\",\"budget_ms\":{:.3},",
                "\"frames\":{},\"seconds\":{:.3},\"frames_per_second\":{:.2},\"bytes\":{},",
                "\"tally\":{{\"bytes\":{},\"cells\":{},\"runs\":{},\"crowding\":{}}},",
                "\"acks\":{{\"count\":{},\"median_ms\":{:.3},\"p99_ms\":{:.3},\"worst_ms\":{:.3},",
                "\"missed\":{},\"asking\":{}}},",
                "\"skipped\":{{\"late\":{},\"held\":{}}},\"unchanged\":{},\"restarts\":{},",
                "\"over_budget\":{},\"governor\":{},\"developing\":{},\"stop\":\"{}\"}}"
            ),
            escape(&self.profile),
            rung_name(self.rung),
            self.cols,
            self.rows,
            self.fps,
            self.fps_from.name(),
            self.budget_ms,
            self.frames,
            self.seconds,
            rate,
            self.bytes,
            self.tally.bytes,
            self.tally.cells,
            self.tally.runs,
            self.tally.crowding,
            self.acks.count,
            ms(self.acks.median_ns),
            ms(self.acks.p99_ns),
            ms(self.acks.worst_ns),
            self.acks.missed,
            self.asking,
            self.skipped_late,
            self.skipped_held,
            self.unchanged,
            self.restarts,
            self.over_budget,
            self.governor
                .as_ref()
                .map_or_else(|| "null".to_string(), |g| g.json()),
            self.developing
                .as_ref()
                .map_or_else(|| "null".to_string(), developed_json),
            match self.stop {
                Stop::Time => "time",
                Stop::Quit => "quit",
            }
        )
    }
}

fn developed_json(d: &Developed) -> String {
    format!(
        "{{\"final_level\":{},\"seconds\":{},\"frames_over_budget\":{}}}",
        d.final_level,
        d.seconds
            .map_or_else(|| "null".to_string(), |s| format!("{s:.3}")),
        d.frames_over_budget
    )
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn rung_name(l: Ladder) -> &'static str {
    match l {
        Ladder::Half => "half",
        Ladder::Quarter => "quarter",
        Ladder::Sixth => "sixth",
        Ladder::Eighth => "eighth",
    }
}

/// What has come back from the surface: acks and the viewer's keys.
#[derive(Default)]
struct Inbox {
    /// Bytes not yet understood: the start of a sequence still arriving.
    partial: Vec<u8>,
    /// When each ack arrived.
    acks: Vec<u64>,
    quit: bool,
    error: Option<io::Error>,
}

impl Inbox {
    /// Take in bytes that arrived at `now_ns`.
    fn take_in(&mut self, bytes: &[u8], now_ns: u64) {
        self.partial.extend_from_slice(bytes);
        let b = std::mem::take(&mut self.partial);
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'q' | b'Q' | 0x03 => {
                    self.quit = true;
                    i += 1;
                }
                0x1b => {
                    let Some(&next) = b.get(i + 1) else {
                        self.partial = b[i..].to_vec();
                        return;
                    };
                    if next != b'[' {
                        i += 1;
                        continue;
                    }
                    let Some(end) = b[i + 2..].iter().position(|c| (0x40..=0x7e).contains(c))
                    else {
                        self.partial = b[i..].to_vec();
                        return;
                    };
                    let fin = i + 2 + end;
                    if b[fin] == b'R' {
                        self.acks.push(now_ns);
                    }
                    i = fin + 1;
                }
                _ => i += 1,
            }
        }
    }
}

/// Real time that listens to the surface while it waits for the next beat.
struct Listener<'a> {
    tty: RefCell<&'a mut Tty>,
    inbox: RefCell<Inbox>,
}

impl Clock for Listener<'_> {
    fn now_ns(&self) -> u64 {
        SystemClock.now_ns()
    }

    fn sleep_until_ns(&self, t: u64) {
        loop {
            let now = self.now_ns();
            if now >= t {
                return;
            }
            {
                let inbox = self.inbox.borrow();
                if inbox.quit || inbox.error.is_some() {
                    // Leaving anyway; no need to wait for the beat.
                    return;
                }
            }
            let got = self
                .tty
                .borrow_mut()
                .recv_until(b'R', Duration::from_nanos(t - now));
            let mut inbox = self.inbox.borrow_mut();
            match got {
                Ok(bytes) => inbox.take_in(&bytes, self.now_ns()),
                Err(e) => inbox.error = Some(e),
            }
        }
    }
}

fn size_of(tty: &Tty) -> (usize, usize) {
    match tty.size() {
        Ok((c, r)) if c > 0 && r > 0 => (c, r),
        _ => FALLBACK_SIZE,
    }
}

/// The profile asked for.
pub fn choose_profile(
    choice: Choice,
    wire: &mut dyn Wire,
    env: &dyn Fn(&str) -> Option<String>,
) -> Profile {
    match choice {
        Choice::Probe => probe(wire, env),
        Choice::Apple => Profile::apple_terminal_2_15(),
        Choice::Cautious => Profile::cautious(),
    }
}

/// The rung asked for, or the finest the profile draws. A rung the profile has not proved
/// is refused.
pub fn choose_rung(asked: Option<Ladder>, profile: &Profile) -> Result<Ladder, String> {
    match asked {
        None => profile
            .ladders
            .last()
            .copied()
            .ok_or_else(|| format!("the {} profile draws no rung", profile.name)),
        Some(l) if profile.ladders.contains(&l) => Ok(l),
        Some(l) => Err(format!(
            "the {} profile has not proved the {} rung",
            profile.name,
            rung_name(l)
        )),
    }
}

/// The price list measured for a profile, when there is one. Apple's Terminal 2.15 gets a
/// price list fitted to its price sweep in place of the profile's first fit.
pub fn measured_prices(profile: &Profile) -> Option<PriceList> {
    if profile.name != Profile::apple_terminal_2_15().name {
        return None;
    }
    let samples = parse_csv(APPLE_PRICES).ok()?;
    Some(fit_prices(&samples))
}

/// The profile the governor judges frames by: the measured prices when there are some.
pub fn priced(profile: &Profile) -> Profile {
    Profile {
        prices: measured_prices(profile).unwrap_or(profile.prices),
        ..profile.clone()
    }
}

/// Read a PNG. See-through parts are blended over `matte`.
pub fn load_picture(path: &str, matte: Rgb) -> Result<Canvas, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("reading {path}: {e}"))?;
    load_png(&bytes, matte).map_err(|e| format!("{path}: {e}"))
}

/// What is being played on one grid.
pub enum Playing {
    Bounce(Box<Player>),
    Develop(Box<Developer>),
}

impl Playing {
    pub fn frame(&mut self, t_ns: u64, out: &mut Vec<u8>) -> Option<Tally> {
        match self {
            Playing::Bounce(p) => p.frame(t_ns, out),
            Playing::Develop(d) => d.frame(t_ns, out),
        }
    }

    pub fn record(&mut self, sent: &Tally, whole: Option<&Tally>, took_ns: Option<f64>) {
        match self {
            Playing::Bounce(p) => p.record(sent, whole, took_ns),
            Playing::Develop(d) => d.record(sent, took_ns),
        }
    }

    pub fn size(&self) -> (usize, usize) {
        match self {
            Playing::Bounce(p) => p.size(),
            Playing::Develop(d) => d.size(),
        }
    }

    pub fn plan(&self) -> Option<Plan> {
        match self {
            Playing::Bounce(p) => p.plan(),
            Playing::Develop(d) => Some(d.plan()),
        }
    }

    /// The run limit the last frame was thrifted to, when one limit held for all of it.
    fn max_runs(&self) -> Option<usize> {
        match self {
            Playing::Bounce(p) => p.made().max_runs,
            Playing::Develop(_) => None,
        }
    }

    /// The last frame went out in part, the rest left developing.
    fn held(&self) -> bool {
        match self {
            Playing::Bounce(p) => p.made().held,
            Playing::Develop(d) => !d.developed(),
        }
    }

    /// On a held frame of the bouncing computer: the tally the whole frame would have had.
    fn whole(&self) -> Option<Tally> {
        match self {
            Playing::Bounce(p) => p.made().whole,
            Playing::Develop(_) => None,
        }
    }

    fn developing(&self) -> Option<Developed> {
        match self {
            Playing::Bounce(_) => None,
            Playing::Develop(d) => Some(d.report()),
        }
    }
}

/// How frames are made for each grid size.
pub struct Maker<'a> {
    o: &'a Options,
    profile: Profile,
    rung: Ladder,
    fps: f64,
    fps_from: BeatFrom,
    budget: Budget,
    picture: Option<Canvas>,
}

impl Maker<'_> {
    pub fn new<'a>(
        o: &'a Options,
        profile: &Profile,
        rung: Ladder,
        picture: Option<Canvas>,
    ) -> Maker<'a> {
        let (fps, fps_from) = choose_beat(o.fps, profile);
        let profile = if o.govern {
            priced(profile)
        } else {
            profile.clone()
        };
        Maker {
            o,
            profile,
            rung,
            fps,
            fps_from,
            // The time for one frame is one beat.
            budget: Budget::per_second(fps),
            picture,
        }
    }

    /// The budget every frame is governed by: 1 / the beat in use.
    pub fn budget(&self) -> Budget {
        self.budget
    }

    /// The beat in use, in frames a second, and where it came from.
    pub fn beat(&self) -> (f64, BeatFrom) {
        (self.fps, self.fps_from)
    }

    /// What to play on a grid of `cols` × `rows`.
    pub fn playing(&self, cols: usize, rows: usize) -> Playing {
        match self.o.what {
            What::Bounce => Playing::Bounce(Box::new(self.player(cols, rows))),
            What::Develop => {
                let plan = develop::plan(
                    &self.profile,
                    self.budget,
                    cols,
                    rows,
                    self.o.thing,
                    self.o.look.strip,
                );
                let blank = Canvas::new(1, 1, develop::BLACK);
                Playing::Develop(Box::new(Developer::new(
                    plan,
                    &self.profile,
                    self.budget,
                    self.o.look,
                    self.picture.as_ref().unwrap_or(&blank),
                    self.o.thing,
                )))
            }
        }
    }

    /// A player of the bouncing computer for a grid of `cols` × `rows`. Governed, the plan is
    /// chosen here, on the rung asked for or, when none was, on any rung the profile draws.
    pub fn player(&self, cols: usize, rows: usize) -> Player {
        let mut player = if self.o.govern {
            let ladders = match self.o.rung {
                Some(l) => vec![l],
                None => self.profile.ladders.clone(),
            };
            let scope = Profile {
                ladders,
                ..self.profile.clone()
            };
            let plan = choose(&scope, self.budget, cols, rows, self.o.moving_share);
            Player::governed(plan, &self.profile, self.budget, self.o.look)
        } else {
            Player::new(cols, rows, &self.profile, self.rung, self.o.look)
        };
        if let Some(p) = &self.picture {
            player.behind(p);
        }
        player
    }
}

/// Play on `tty` until time is up or the viewer quits. The terminal is restored when this
/// returns, however it returns.
pub fn play(mut tty: Tty, o: &Options, env: &dyn Fn(&str) -> Option<String>) -> io::Result<Report> {
    let matte = match o.what {
        What::Bounce => ROOM,
        What::Develop => develop::BLACK,
    };
    let picture = match &o.picture {
        Some(path) => Some(load_picture(path, matte).map_err(io::Error::other)?),
        None => None,
    };
    let profile = choose_profile(o.profile, &mut tty, env);
    let asked = match o.what {
        What::Bounce => o.rung,
        What::Develop => Some(Ladder::Half),
    };
    let rung = choose_rung(asked, &profile).map_err(io::Error::other)?;
    let maker = Maker::new(o, &profile, rung, picture);
    let result = run(&mut tty, o, &maker);
    let _ = tty.send(PLAIN_COLOURS);
    drop(tty);
    result
}

fn run(tty: &mut Tty, o: &Options, maker: &Maker) -> io::Result<Report> {
    let (cols, rows) = size_of(tty);
    let mut player = maker.playing(cols, rows);
    let budget_ns = maker.budget().frame_ns;
    let (fps, fps_from) = maker.beat();
    let profile = &maker.profile;
    let mut acks = Acks::new(GIVE_UP_AFTER, ACK_TIMEOUT.as_nanos() as u64);
    let mut report = Report {
        profile: profile.name.clone(),
        rung: player.plan().map_or(maker.rung, |p| p.ladder),
        cols,
        rows,
        fps,
        fps_from,
        budget_ms: budget_ns / 1e6,
        frames: 0,
        seconds: 0.0,
        bytes: 0,
        tally: Tally::default(),
        acks: AckSummary::default(),
        asking: true,
        skipped_late: 0,
        skipped_held: 0,
        unchanged: 0,
        restarts: 0,
        over_budget: 0,
        governor: player.plan().map(Governed::new),
        developing: player.developing(),
        stop: Stop::Time,
    };
    // The frame waiting for its answer: its tally, the whole frame meant when it was held,
    // and when it went out.
    let mut pending: Option<(Tally, Option<Tally>, u64)> = None;
    let listener = Listener {
        tty: RefCell::new(tty),
        inbox: RefCell::new(Inbox::default()),
    };
    // The beat's own period, so the picture's time falls exactly on the beat.
    let period = (1e9 / fps).round().max(1.0) as u64;
    let limit_ns = o.seconds.map(|s| (s.max(0.0) * 1e9) as u64);
    let mut beat = Beat::new(fps, &listener);
    let started = listener.now_ns();
    let mut out = Vec::new();
    loop {
        let tick = beat.next(&listener);
        report.skipped_late += tick.skipped;
        {
            let mut inbox = listener.inbox.borrow_mut();
            for at in inbox.acks.drain(..) {
                acks.answered(at);
                if let Some((tally, whole, sent_at)) = pending.take() {
                    let took = at.saturating_sub(sent_at);
                    if took > ACK_TIMEOUT.as_nanos() as u64 {
                        player.record(&tally, whole.as_ref(), None);
                    } else {
                        if took as f64 > budget_ns {
                            report.over_budget += 1;
                        }
                        player.record(&tally, whole.as_ref(), Some(took as f64));
                    }
                }
            }
            if let Some(e) = inbox.error.take() {
                return Err(e);
            }
            if inbox.quit {
                report.stop = Stop::Quit;
                break;
            }
        }
        let t_ns = tick.index * period;
        if limit_ns.is_some_and(|l| t_ns >= l) {
            break;
        }
        if listener.tty.borrow().resized() {
            let (c, r) = size_of(&listener.tty.borrow());
            if (c, r) != player.size() {
                player = maker.playing(c, r);
                (report.cols, report.rows) = (c, r);
                report.restarts += 1;
                pending = None;
                if let (Some(g), Some(plan)) = (&mut report.governor, player.plan()) {
                    g.plan = plan;
                    report.rung = plan.ladder;
                }
            }
        }
        let now = listener.now_ns();
        if !acks.may_send(now) {
            report.skipped_held += 1;
            continue;
        }
        out.clear();
        let made = player.frame(t_ns, &mut out);
        if let (Some(g), Some(runs)) = (&mut report.governor, player.max_runs()) {
            g.max_runs_low = g.max_runs_low.min(runs);
            g.max_runs_high = g.max_runs_high.max(runs);
        }
        let Some(tally) = made else {
            report.unchanged += 1;
            continue;
        };
        if let Some(g) = &mut report.governor {
            g.held += player.held() as u64;
        }
        let asking = acks.asking();
        if asking {
            out.extend_from_slice(ASK);
        }
        listener.tty.borrow_mut().send(&out)?;
        // Answers are timed from when the frame went out, not from when it was begun: making
        // a governed frame takes a few milliseconds of its own.
        let sent_at = listener.now_ns();
        if asking {
            acks.sent(sent_at);
        }
        // A frame still waiting went unanswered.
        if let Some((t, whole, _)) = pending.take() {
            player.record(&t, whole.as_ref(), None);
        }
        let whole = player.whole();
        if asking {
            pending = Some((tally, whole, sent_at));
        } else {
            player.record(&tally, whole.as_ref(), None);
        }
        report.frames += 1;
        report.bytes += tally.bytes;
        report.tally.add(&tally);
    }
    report.developing = player.developing();
    report.seconds = (listener.now_ns() - started) as f64 / 1e9;
    report.acks = acks.summary();
    report.asking = acks.asking();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inbox_finds_acks_and_keys_across_reads() {
        let mut inbox = Inbox::default();
        inbox.take_in(b"\x1b[12;3", 5);
        assert!(inbox.acks.is_empty());
        inbox.take_in(b"4R\x1b[A", 7);
        assert_eq!(inbox.acks, vec![7]);
        assert!(!inbox.quit);
        inbox.take_in(b"x\x1b", 8);
        assert!(!inbox.quit);
        inbox.take_in(b"[1;1Rq", 9);
        assert_eq!(inbox.acks, vec![7, 9]);
        assert!(inbox.quit);
        let mut ctrl_c = Inbox::default();
        ctrl_c.take_in(&[0x03], 1);
        assert!(ctrl_c.quit);
    }

    #[test]
    fn the_rung_is_the_finest_proved() {
        let apple = Profile::apple_terminal_2_15();
        assert_eq!(choose_rung(None, &apple), Ok(Ladder::Quarter));
        assert_eq!(choose_rung(Some(Ladder::Half), &apple), Ok(Ladder::Half));
        assert!(choose_rung(Some(Ladder::Eighth), &apple).is_err());
        assert_eq!(choose_rung(None, &Profile::cautious()), Ok(Ladder::Half));
    }

    #[test]
    fn without_fps_the_beat_is_the_paint_rate() {
        let apple = Profile::apple_terminal_2_15();
        assert_eq!(
            choose_beat(None, &apple),
            (apple.paint_hz, BeatFrom::PaintRate)
        );
        assert_eq!(choose_beat(Some(60.0), &apple), (60.0, BeatFrom::Asked));
        let cautious = Profile::cautious();
        assert_eq!(choose_beat(None, &cautious).0, cautious.paint_hz);
    }

    #[test]
    fn the_governors_budget_follows_the_beat_in_use() {
        let apple = Profile::apple_terminal_2_15();
        let ladder = Ladder::Quarter;
        for (asked, fps) in [
            (None, apple.paint_hz),
            (Some(60.0), 60.0),
            (Some(15.0), 15.0),
        ] {
            let o = Options {
                fps: asked,
                ..Options::default()
            };
            let maker = Maker::new(&o, &apple, ladder, None);
            assert_eq!(maker.beat().0, fps);
            assert_eq!(maker.budget(), Budget::per_second(fps));
            // The player's governor judges frames by that budget and plans for it.
            let player = maker.player(160, 45);
            assert_eq!(player.budget(), Some(Budget::per_second(fps)));
            let planned = choose(&priced(&apple), Budget::per_second(fps), 160, 45, 1.0);
            assert_eq!(player.plan(), Some(planned));
        }
        // A slower beat gives each frame more time, so the plan allows more runs a row.
        let at = |fps| {
            let o = Options {
                fps: Some(fps),
                ..Options::default()
            };
            let maker = Maker::new(&o, &apple, ladder, None);
            maker.player(160, 45).plan().unwrap().max_runs
        };
        assert!(at(apple.paint_hz) > at(60.0));
    }

    #[test]
    fn the_report_is_json() {
        let r = Report {
            profile: "a\"b".into(),
            rung: Ladder::Quarter,
            cols: 160,
            rows: 45,
            fps: 60.0,
            fps_from: BeatFrom::Asked,
            budget_ms: 1000.0 / 60.0,
            frames: 300,
            seconds: 5.0,
            bytes: 1000,
            tally: Tally {
                bytes: 1000,
                cells: 20,
                runs: 3,
                crowding: 9,
            },
            acks: AckSummary {
                count: 290,
                median_ns: 1_500_000,
                p99_ns: 4_000_000,
                worst_ns: 9_000_000,
                missed: 2,
            },
            asking: true,
            skipped_late: 1,
            skipped_held: 4,
            unchanged: 0,
            restarts: 0,
            over_budget: 7,
            governor: None,
            developing: None,
            stop: Stop::Time,
        };
        let j = r.to_json();
        assert!(j.starts_with("{\"profile\":\"a\\\"b\",\"rung\":\"quarter\""));
        assert!(j.contains("\"fps\":60,\"fps_from\":\"--fps\",\"budget_ms\":16.667,"));
        assert!(j.contains("\"frames\":300,"));
        assert!(j.contains("\"frames_per_second\":60.00,"));
        assert!(j.contains("\"median_ms\":1.500,"));
        assert!(j.contains("\"skipped\":{\"late\":1,\"held\":4}"));
        assert!(j.ends_with("\"stop\":\"time\"}"));
        assert_eq!(j.matches('{').count(), j.matches('}').count());
        assert!(j.contains("\"over_budget\":7,\"governor\":null,\"developing\":null,"));
        let governed = Report {
            governor: Some(Governed {
                plan: Plan {
                    ladder: Ladder::Quarter,
                    cols: 160,
                    rows: 45,
                    max_runs: 23,
                    depth: ColourDepth::Full,
                },
                held: 12,
                max_runs_low: 17,
                max_runs_high: 23,
            }),
            ..r
        };
        let j = governed.to_json();
        assert!(j.contains(concat!(
            "\"governor\":{\"plan\":{\"rung\":\"quarter\",\"cols\":160,\"rows\":45,",
            "\"max_runs\":23,\"depth\":\"full\"},\"frames_held\":12,",
            "\"max_runs\":{\"low\":17,\"high\":23}},\"developing\":null,\"stop\""
        )));
        assert_eq!(j.matches('{').count(), j.matches('}').count());
    }
}

#[cfg(test)]
mod prices {
    use super::*;
    use phosphor_govern::calibrate::worst_error;

    #[test]
    fn apple_gets_prices_fitted_to_its_sweep() {
        let apple = Profile::apple_terminal_2_15();
        let fitted = measured_prices(&apple).expect("the sweep parses");
        assert_ne!(fitted, apple.prices);
        let samples = parse_csv(APPLE_PRICES).unwrap();
        println!(
            "fitted {fitted:?}, worst error {:.2}, first fit worst error {:.2}",
            worst_error(&fitted, &samples),
            worst_error(&apple.prices, &samples)
        );
        let budget = Budget::per_second(60.0);
        for (c, r) in [(160, 45), (80, 24), (320, 90)] {
            let plan = choose(&priced(&apple), budget, c, r, 1.0);
            println!("{c}x{r}: {plan:?}");
        }
        assert_eq!(measured_prices(&Profile::cautious()), None);
        assert_eq!(priced(&Profile::cautious()), Profile::cautious());
    }
}
