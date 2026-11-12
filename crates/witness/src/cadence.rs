//! Cadence: film the test window and work out when each frame really appeared.
//!
//! The program under test paints the frame counter strip (see `docs/architecture.md`) on
//! every frame. A film of the window is read strip by strip: each film frame gives a frame
//! number, or nothing when the strip is torn or unreadable. The numbers in order say how many
//! frames were shown, repeated or skipped, and how long each one stayed on screen.
//!
//! Filming opens a test window in Apple's Terminal or Ghostty and records the screen with
//! `ffmpeg`. The recording is cropped to the test window inside the same `ffmpeg` command and
//! piped out as raw frames; no frame of the whole screen is ever stored.

use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// When each frame really appeared, from a film of the window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cadence {
    /// Film frames read.
    pub film_frames: u64,
    /// Distinct frame numbers seen, counted in order.
    pub shown: u64,
    /// Film frames showing the same number as the one before.
    pub repeats: u64,
    /// Frame numbers never seen: the sum of every jump of more than one, less one.
    pub skips: u64,
    /// Film frames whose strip was torn or unreadable.
    pub torn: u64,
    /// The longest time one number stayed on screen.
    pub longest_hold_ms: f64,
    /// How long each number shown stayed on screen, in order.
    pub holds_ms: Vec<f64>,
}

/// White or black paper, read from one pixel. `None` for anything in between.
fn paper(rgb: &[u8], w: usize, x: usize, y: usize) -> Option<bool> {
    let i = (y * w + x) * 3;
    let (r, g, b) = (rgb[i], rgb[i + 1], rgb[i + 2]);
    let (lo, hi) = (r.min(g).min(b), r.max(g).max(b));
    if lo >= 160 {
        Some(true)
    } else if hi <= 95 {
        Some(false)
    } else {
        None
    }
}

/// Reads the frame number from one film frame.
///
/// `rgb` is `w` × `h` pixels, three bytes each. `strip` is `(x, y, width, height)` in pixels
/// and covers cells 1 to 18 of the top row. Cells 17 and 18 must read white and black; the
/// 16 cells before them are the frame number, most significant bit first. Each cell is read
/// at its centre and a quarter of a cell above and below it. `None` when any reading is
/// neither white nor black, the three readings disagree (a torn frame), or the marks are
/// wrong.
pub fn read_strip(
    rgb: &[u8],
    w: usize,
    h: usize,
    strip: (usize, usize, usize, usize),
) -> Option<u16> {
    let (sx, sy, sw, sh) = strip;
    if sw < 18 || sh == 0 || sx + sw > w || sy + sh > h || rgb.len() < w * h * 3 {
        return None;
    }
    let cw = sw as f64 / 18.0;
    let ys = [0.25, 0.5, 0.75].map(|f| sy + ((sh as f64 * f) as usize).min(sh - 1));
    let cell = |i: usize| -> Option<bool> {
        let x = (sx as f64 + (i as f64 + 0.5) * cw) as usize;
        let first = paper(rgb, w, x, ys[0])?;
        for &y in &ys[1..] {
            if paper(rgb, w, x, y)? != first {
                return None;
            }
        }
        Some(first)
    };
    if !cell(16)? || cell(17)? {
        return None;
    }
    let mut n = 0u16;
    for i in 0..16 {
        n = n << 1 | cell(i)? as u16;
    }
    Some(n)
}

/// Counts what a film shows, given the number read from each film frame in order.
///
/// A number counts as shown the first time it appears. A torn frame belongs to the hold of
/// the number before it. A jump forward of more than one adds its gap to the skips; counting
/// wraps from 65,535 to 0, so that is a step of one. A jump backwards is shown but skips
/// nothing.
pub fn cadence(numbers: &[Option<u16>], film_fps: f64) -> Cadence {
    let ms = 1000.0 / film_fps;
    let mut c = Cadence {
        film_frames: numbers.len() as u64,
        ..Cadence::default()
    };
    let mut last: Option<u16> = None;
    let mut hold = 0u64;
    for n in numbers {
        match (*n, last) {
            (None, _) => {
                c.torn += 1;
                if last.is_some() {
                    hold += 1;
                }
            }
            (Some(n), Some(l)) if n == l => {
                c.repeats += 1;
                hold += 1;
            }
            (Some(n), l) => {
                if let Some(l) = l {
                    c.holds_ms.push(hold as f64 * ms);
                    let step = n.wrapping_sub(l);
                    if step > 1 && step < 0x8000 {
                        c.skips += step as u64 - 1;
                    }
                }
                c.shown += 1;
                last = Some(n);
                hold = 1;
            }
        }
    }
    if last.is_some() {
        c.holds_ms.push(hold as f64 * ms);
    }
    c.longest_hold_ms = c.holds_ms.iter().cloned().fold(0.0, f64::max);
    c
}

/// Finds the frame counter strip in a film: `(x, y, width, height)` covering cells 1 to 18
/// of the top row, or `None` when the film never shows one.
///
/// Every band of pixel rows that changes between white and black during the film is a
/// candidate, top first: the window's title bar can be one, when its white title changes.
/// In each band, every left edge and width of the strip is tried on a sample of the film's
/// frames, read near the band's top. The strip is where the numbers read most often step a
/// little forward from one sampled frame to the next, as a frame counter does: a still
/// pattern that happens to look like the marks, such as text or a picture beside the strip,
/// reads the same number or none. A place that still reads a counter but not this one (a
/// little too wide, missing the lowest bit; or a cell too far right, counting twice as fast)
/// loses on two further tests: the right place has edges on its cell borders and none inside
/// its cells, and the smallest steps. Of the places that do equally well, the middle is taken.
/// The strip goes down as far as each row of pixels on its own reads the same numbers in the
/// frames that show the strip.
pub fn locate_strip(
    frames: &[Vec<u8>],
    w: usize,
    h: usize,
) -> Option<(usize, usize, usize, usize)> {
    let size = w * h * 3;
    let frames: Vec<&Vec<u8>> = frames.iter().filter(|f| f.len() >= size).collect();
    if frames.is_empty() || w < 18 {
        return None;
    }
    // pixels that are white in some frame and black in another
    let mut seen = vec![0u8; w * h];
    for f in &frames {
        for (p, s) in seen.iter_mut().enumerate() {
            match paper(f, w, p % w, p / w) {
                Some(true) => *s |= 1,
                Some(false) => *s |= 2,
                None => {}
            }
        }
    }
    let busy = |y: usize| seen[y * w..(y + 1) * w].iter().filter(|&&s| s == 3).count() >= 3;
    let mut bands = Vec::new();
    let mut y = 0;
    while y < h {
        if busy(y) {
            let y0 = y;
            while y < h && busy(y) {
                y += 1;
            }
            bands.push((y0, y - y0));
        } else {
            y += 1;
        }
    }
    let step = (frames.len() / 64).max(1);
    // the farthest the number may move between two sampled frames: the program may paint
    // up to 16 frames for each film frame
    let reach = (16 * step).min(0x7fff) as u16;
    let sample: Vec<&Vec<u8>> = frames.iter().step_by(step).copied().collect();
    let numbers = |strip: (usize, usize, usize, usize)| -> Vec<Option<u16>> {
        sample.iter().map(|f| read_strip(f, w, h, strip)).collect()
    };
    let mut best: Option<(usize, (usize, usize, usize, usize))> = None;
    for (y0, band) in bands {
        let probe = band.min(4);
        // how often each column differs from the one to its left, along the band's top
        let ym = y0 + probe / 2;
        let mut edges = vec![0u64; w];
        for f in &sample {
            let lum = |x: usize| {
                let i = (ym * w + x) * 3;
                (f[i] as u32 + f[i + 1] as u32 + f[i + 2] as u32) >= 384
            };
            for (x, e) in edges.iter_mut().enumerate().skip(1) {
                if lum(x) != lum(x - 1) {
                    *e += 1;
                }
            }
        }
        let mut most: Option<(usize, i64, std::cmp::Reverse<u64>)> = None;
        let mut tied: Vec<(usize, usize)> = Vec::new();
        for sw in 36..=w {
            for x0 in 0..=w - sw {
                let read: Vec<u16> = numbers((x0, y0, sw, probe)).into_iter().flatten().collect();
                let steps: Vec<u64> = read
                    .windows(2)
                    .map(|p| p[1].wrapping_sub(p[0]))
                    .filter(|d| (1..=reach).contains(d))
                    .map(u64::from)
                    .collect();
                if steps.len() < 2 || most.is_some_and(|m| steps.len() < m.0) {
                    continue;
                }
                // more steps forward is better; then edges on the cell borders and not inside
                // the cells; then smaller steps, since reading the strip a cell to the right also counts
                // forward, twice as fast
                let border = |k: usize| (x0 as f64 + k as f64 * sw as f64 / 18.0).round() as usize;
                let on_borders = (1..18)
                    .map(|k| {
                        let x = border(k);
                        (x.saturating_sub(1)..=(x + 1).min(w - 1))
                            .map(|x| edges[x])
                            .max()
                            .unwrap_or(0)
                    })
                    .sum::<u64>();
                let inside = (0..18)
                    .map(|k| {
                        let (a, b) = (border(k) + 2, border(k + 1).saturating_sub(1));
                        (a..b.min(w)).map(|x| edges[x]).sum::<u64>()
                    })
                    .sum::<u64>();
                let score = (
                    steps.len(),
                    on_borders as i64 - inside as i64,
                    std::cmp::Reverse(steps.iter().sum::<u64>()),
                );
                if most.is_none_or(|m| score > m) {
                    most = Some(score);
                    tied.clear();
                }
                if most == Some(score) {
                    tied.push((x0, sw));
                }
            }
        }
        let Some(most) = most else { continue };
        if best.is_some_and(|(b, _)| b >= most.0) {
            continue;
        }
        let n = tied.len() as f64;
        let x0 = (tied.iter().map(|t| t.0 as f64).sum::<f64>() / n).round() as usize;
        let sw = (tied.iter().map(|t| t.1 as f64).sum::<f64>() / n).round() as usize;
        let want = numbers((x0, y0, sw, probe));
        // the strip goes down as long as each row on its own reads the same numbers, in the
        // frames that show it
        let tall = (0..band)
            .take_while(|&k| {
                let row = numbers((x0, y0 + k, sw, 1));
                want.iter().zip(&row).all(|(a, b)| a.is_none() || a == b)
            })
            .count()
            .max(1);
        best = Some((most.0, (x0, y0, sw, tall)));
    }
    best.map(|b| b.1)
}

/// The cadence as one line of JSON.
pub fn to_json(c: &Cadence) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "{{\"film_frames\":{},\"shown\":{},\"repeats\":{},\"skips\":{},\"torn\":{},\"longest_hold_ms\":{:.1},\"holds_ms\":[",
        c.film_frames, c.shown, c.repeats, c.skips, c.torn, c.longest_hold_ms
    );
    for (i, h) in c.holds_ms.iter().enumerate() {
        let _ = write!(s, "{}{:.1}", if i > 0 { "," } else { "" }, h);
    }
    s.push_str("]}");
    s
}

/// The terminal the test window opens in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Terminal {
    /// Apple's Terminal, the reference surface.
    #[default]
    Apple,
    /// Ghostty, at `/Applications/Ghostty.app`.
    Ghostty,
}

impl Terminal {
    /// The name `--terminal` takes.
    pub fn parse(name: &str) -> Option<Terminal> {
        match name {
            "apple" => Some(Terminal::Apple),
            "ghostty" => Some(Terminal::Ghostty),
            _ => None,
        }
    }

    /// The owner name its windows carry on screen.
    pub fn owner(self) -> &'static str {
        match self {
            Terminal::Apple => "Terminal",
            Terminal::Ghostty => "Ghostty",
        }
    }
}

/// What to film.
#[derive(Clone, Debug)]
pub struct FilmOptions {
    /// The terminal to open the test window in.
    pub terminal: Terminal,
    /// Font size of the test window, in points (Menlo in Apple's Terminal, Ghostty's own
    /// font in Ghostty).
    pub font: f64,
    pub cols: usize,
    pub rows: usize,
    /// How long to record.
    pub seconds: f64,
    /// The film rate asked of `ffmpeg`.
    pub fps: f64,
    /// The program to run in the window, and its arguments.
    pub command: Vec<String>,
}

/// A film of the test window, read.
#[derive(Clone, Debug)]
pub struct Film {
    /// The film rate the cadence is counted at.
    pub film_fps: f64,
    /// Frames the screen recording really delivered each second; the rest are copies
    /// `ffmpeg` made to keep the film's rate even.
    pub captured_fps: f64,
    /// Film frames that were copies of the one before.
    pub copied: u64,
    /// Where the strip was found, in pixels of the filmed corner of the window.
    pub strip: (usize, usize, usize, usize),
    /// The cadence, from the first film frame with a readable strip to the last.
    pub cadence: Cadence,
}

impl Film {
    pub fn to_json(&self) -> String {
        let (x, y, w, h) = self.strip;
        format!(
            "{{\"film_fps\":{:.1},\"captured_fps\":{:.1},\"copied\":{},\"strip\":[{x},{y},{w},{h}],\"cadence\":{}}}",
            self.film_fps,
            self.captured_fps,
            self.copied,
            to_json(&self.cadence)
        )
    }
}

/// The folder holding the Apple Terminal scripts.
pub fn rig_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rigs/apple-terminal")
}

/// The folder holding the Ghostty scripts.
pub fn ghostty_rig_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rigs/ghostty")
}

fn winrect(terminal: Terminal, args: &[String]) -> Result<String, String> {
    let out = Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg(rig_dir().join("winrect.js"))
        .arg("--owner")
        .arg(terminal.owner())
        .args(args)
        .output()
        .map_err(|e| format!("osascript: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Opens a test window running `opts.command`, films it, closes it, and reads the film.
///
/// Call it through `rigs/apple-terminal/with-screen.sh`, which keeps other test windows away.
pub fn film(opts: &FilmOptions) -> Result<Film, String> {
    if opts.command.is_empty() {
        return Err("no command to film".into());
    }
    let dir = std::env::temp_dir().join(format!("phosphor-film-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let res = film_in(opts, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    res
}

fn film_in(opts: &FilmOptions, dir: &Path) -> Result<Film, String> {
    // the window runs a script that waits until filming has begun, then becomes the command
    let script = dir.join("run.sh");
    let words: Vec<String> = opts.command.iter().map(|a| quote(a)).collect();
    let d = quote(&dir.to_string_lossy());
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho $$ > {d}/pid\nwhile [ ! -f {d}/go ]; do sleep 0.02; done\nexec {}\n",
            words.join(" ")
        ),
    )
    .map_err(|e| e.to_string())?;
    Command::new("chmod")
        .arg("+x")
        .arg(&script)
        .status()
        .map_err(|e| e.to_string())?;
    let t = opts.terminal;
    let before: Vec<String> = winrect(t, &["--ids".into()])?
        .split_whitespace()
        .map(String::from)
        .collect();
    match t {
        Terminal::Apple => open_apple(opts, dir, &script)?,
        Terminal::Ghostty => open_ghostty(opts, &script)?,
    }
    let result = film_window(opts, dir, &before);
    let closed = close_window(t, dir, &before);
    let ended = match t {
        Terminal::Apple => Ok(()),
        Terminal::Ghostty => end_ghostty(dir),
    };
    let film = result?;
    closed?;
    ended.map(|_| film)
}

/// Opens Apple's Terminal on a settings file that runs `script` in a window of the asked
/// font and grid.
fn open_apple(opts: &FilmOptions, dir: &Path, script: &Path) -> Result<(), String> {
    let profile = dir.join("witness.terminal");
    let made = Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg(rig_dir().join("mkprofile.js"))
        .arg(&profile)
        .arg("Phosphor Witness")
        .arg("Menlo-Regular")
        .arg(opts.font.to_string())
        .arg(opts.cols.to_string())
        .arg(opts.rows.to_string())
        // Terminal types this into a shell; the shell must leave so the window closes
        .arg(format!("{}; exit", quote(&script.to_string_lossy())))
        .output()
        .map_err(|e| format!("osascript: {e}"))?;
    if !made.status.success() {
        return Err(format!(
            "mkprofile: {}",
            String::from_utf8_lossy(&made.stderr)
        ));
    }
    Command::new("open")
        .arg("-a")
        .arg("Terminal")
        .arg(&profile)
        .status()
        .map_err(|e| format!("open: {e}"))?;
    Ok(())
}

/// Opens a new Ghostty, apart from any Ghostty already open, with one window of the asked
/// font and grid that runs `script` and closes when it ends. See `rigs/ghostty/open.sh`.
fn open_ghostty(opts: &FilmOptions, script: &Path) -> Result<(), String> {
    let opened = Command::new(ghostty_rig_dir().join("open.sh"))
        .arg(opts.font.to_string())
        .arg(opts.cols.to_string())
        .arg(opts.rows.to_string())
        .arg(script)
        .output()
        .map_err(|e| format!("open.sh: {e}"))?;
    if !opened.status.success() {
        return Err(format!(
            "open.sh: {}",
            String::from_utf8_lossy(&opened.stderr).trim()
        ));
    }
    Ok(())
}

/// Makes sure the Ghostty this film opened has gone. It quits when its window closes; one
/// that is still running five seconds later is ended. Only processes whose command line names
/// this film's own folder are touched, so a Ghostty opened by anyone else is left alone.
fn end_ghostty(dir: &Path) -> Result<(), String> {
    let mine = || -> Vec<String> {
        Command::new("pgrep")
            .arg("-f")
            .arg(dir.to_string_lossy().as_ref())
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .split_whitespace()
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    };
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if mine().is_empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    for signal in ["-TERM", "-KILL"] {
        let left = mine();
        if left.is_empty() {
            return Ok(());
        }
        let _ = Command::new("kill")
            .arg(signal)
            .args(&left)
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(Duration::from_secs(1));
    }
    if mine().is_empty() {
        Ok(())
    } else {
        Err("the Ghostty this film opened would not quit".into())
    }
}

/// Ends the command in the test window and waits for the window to go.
fn close_window(terminal: Terminal, dir: &Path, before: &[String]) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if let Ok(pid) = std::fs::read_to_string(dir.join("pid")) {
            let pid = pid.trim();
            // the script may still be waiting for the go file; let it run into the kill
            let _ = std::fs::write(dir.join("go"), "");
            let _ = Command::new("kill").arg(pid).stderr(Stdio::null()).status();
        }
        if winrect(terminal, before)
            .map(|r| r.is_empty())
            .unwrap_or(false)
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err("the test window did not close".into())
}

fn film_window(opts: &FilmOptions, dir: &Path, before: &[String]) -> Result<Film, String> {
    // wait for the window, then for it to stop moving. A Ghostty window takes its grid size a
    // moment after it appears, so it has to keep still for a second.
    let settle = match opts.terminal {
        Terminal::Apple => Duration::from_millis(250),
        Terminal::Ghostty => Duration::from_secs(1),
    };
    let start = Instant::now();
    let mut rect = String::new();
    while start.elapsed() < Duration::from_secs(20) {
        let now = winrect(opts.terminal, before)?;
        if !now.is_empty() && now == rect {
            break;
        }
        rect = now;
        std::thread::sleep(settle);
    }
    let v: Vec<f64> = rect
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    if v.len() != 5 {
        return Err("the test window did not appear on the main screen".into());
    }
    let (wx, wy, ww, wh, scale) = (
        v[0] as usize,
        v[1] as usize,
        v[2] as usize,
        v[3] as usize,
        v[4],
    );
    // Film only the window's top-left corner: the title bar, then two and a half rows, and
    // a quarter more than the strip's 18 cells. Both crops happen inside ffmpeg.
    let cell_w = ww as f64 / opts.cols as f64;
    let cell_h = (wh as f64 - 40.0 * scale) / opts.rows as f64;
    let cw = ((cell_w * 23.0) as usize + 8).min(ww) & !1;
    let ch = ((40.0 * scale + cell_h * 2.5) as usize).min(wh) & !1;
    let filter = format!("crop={ww}:{wh}:{wx}:{wy},crop={cw}:{ch}:0:0");
    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-loglevel", "error"])
        .args([
            "-f",
            "avfoundation",
            "-capture_cursor",
            "0",
            "-capture_mouse_clicks",
            "0",
        ])
        .args(["-framerate", &opts.fps.to_string(), "-pixel_format", "bgr0"])
        .args([
            "-i",
            "Capture screen 0:none",
            "-t",
            &opts.seconds.to_string(),
        ])
        .args([
            "-vf",
            &filter,
            "-fps_mode",
            "cfr",
            "-r",
            &opts.fps.to_string(),
        ])
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24"])
        .args(["-nostats", "-progress", "pipe:2", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    let mut err = child.stderr.take().unwrap();
    let log = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let mut out = child.stdout.take().unwrap();
    let size = cw * ch * 3;
    let mut frames: Vec<Vec<u8>> = Vec::new();
    loop {
        let mut f = vec![0u8; size];
        if out.read_exact(&mut f).is_err() {
            break;
        }
        if frames.is_empty() {
            std::fs::write(dir.join("go"), "").map_err(|e| e.to_string())?;
        }
        frames.push(f);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let log = log.join().unwrap_or_default();
    if frames.is_empty() {
        return Err(format!(
            "ffmpeg gave no frames ({status}); is screen recording allowed? {}",
            log.lines()
                .filter(|l| !l.contains('='))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    let last = |key: &str| -> u64 {
        log.lines()
            .rev()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('=')?.trim().parse().ok())
            .unwrap_or(0)
    };
    let (dup, dropped) = (last("dup_frames"), last("drop_frames"));
    let n = frames.len() as u64;
    let secs = n as f64 / opts.fps;
    let strip = locate_strip(&frames, cw, ch).ok_or("no frame counter strip in the film")?;
    let numbers: Vec<Option<u16>> = frames
        .iter()
        .map(|f| read_strip(f, cw, ch, strip))
        .collect();
    drop(frames);
    // the film runs a little before the program starts painting; count from its first frame
    let first = numbers.iter().position(|n| n.is_some()).unwrap_or(0);
    let end = numbers
        .iter()
        .rposition(|n| n.is_some())
        .map_or(0, |i| i + 1);
    Ok(Film {
        film_fps: opts.fps,
        captured_fps: (n + dropped).saturating_sub(dup) as f64 / secs,
        copied: dup,
        strip,
        cadence: cadence(&numbers[first..end.max(first)], opts.fps),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_shown_repeats_skips_and_holds() {
        let s = [1, 1, 2, 3, 3, 3, 5].map(Some);
        let c = cadence(&s, 60.0);
        assert_eq!(c.film_frames, 7);
        assert_eq!(c.shown, 4);
        assert_eq!(c.repeats, 3);
        assert_eq!(c.skips, 1);
        assert_eq!(c.torn, 0);
        assert!(
            (c.longest_hold_ms - 50.0).abs() < 1e-9,
            "{}",
            c.longest_hold_ms
        );
        assert_eq!(c.holds_ms.len(), 4);
    }

    #[test]
    fn wrap_around_is_not_a_skip() {
        let c = cadence(&[Some(65_534), Some(65_535), Some(0), Some(1)], 60.0);
        assert_eq!(c.shown, 4);
        assert_eq!(c.skips, 0);
        let c = cadence(&[Some(65_535), Some(1)], 60.0);
        assert_eq!(c.skips, 1);
    }

    #[test]
    fn torn_frames_are_counted_and_held() {
        let c = cadence(&[Some(1), None, Some(2), Some(2)], 100.0);
        assert_eq!(c.torn, 1);
        assert_eq!(c.shown, 2);
        assert_eq!(c.repeats, 1);
        assert_eq!(c.holds_ms, vec![20.0, 20.0]);
    }

    /// A synthetic film frame: the strip for `n` with cells `cw` pixels wide, starting at
    /// (`x0`, `y0`), `chh` pixels tall, on black. Pixels a cell edge passes through are mixed,
    /// as a screen recording mixes them. `c17` and `c18` are the two marks' colours.
    #[allow(clippy::too_many_arguments)]
    fn frame(
        w: usize,
        h: usize,
        x0: f64,
        y0: usize,
        cw: f64,
        chh: usize,
        n: u16,
        c17: bool,
        c18: bool,
    ) -> Vec<u8> {
        let mut rgb = vec![0u8; w * h * 3];
        let white = |i: usize| match i {
            0..=15 => n >> (15 - i) & 1 == 1,
            16 => c17,
            17 => c18,
            _ => false,
        };
        for x in 0..w {
            // how much of pixel x each cell covers
            let mut v = 0.0;
            for i in 0..18 {
                let (a, b) = (x0 + i as f64 * cw, x0 + (i + 1) as f64 * cw);
                let cover = (b.min(x as f64 + 1.0) - a.max(x as f64)).max(0.0);
                if white(i) {
                    v += cover;
                }
            }
            let g = (v * 255.0).round() as u8;
            for y in y0..y0 + chh {
                let p = (y * w + x) * 3;
                rgb[p..p + 3].copy_from_slice(&[g, g, g]);
            }
        }
        rgb
    }

    #[test]
    fn reads_a_number_from_cells_14_25_pixels_wide() {
        let (w, h) = (300, 40);
        let (x0, y0, cw, chh) = (7.6, 9, 14.25, 28);
        for n in [0xA5C3u16, 0, 0xFFFF, 1, 0x8000] {
            let rgb = frame(w, h, x0, y0, cw, chh, n, true, false);
            let strip = (8, y0, (18.0 * cw).round() as usize, chh);
            assert_eq!(read_strip(&rgb, w, h, strip), Some(n), "{n:#x}");
        }
    }

    #[test]
    fn a_black_cell_17_reads_as_none() {
        let (w, h) = (300, 40);
        let rgb = frame(w, h, 7.6, 9, 14.25, 28, 0xA5C3, false, false);
        assert_eq!(read_strip(&rgb, w, h, (8, 9, 257, 28)), None);
        let rgb = frame(w, h, 7.6, 9, 14.25, 28, 0xA5C3, true, true);
        assert_eq!(read_strip(&rgb, w, h, (8, 9, 257, 28)), None);
    }

    #[test]
    fn a_torn_strip_reads_as_none() {
        let (w, h) = (300, 40);
        let mut rgb = frame(w, h, 7.6, 9, 14.25, 28, 0x00FF, true, false);
        let lower = frame(w, h, 7.6, 9, 14.25, 28, 0x0100, true, false);
        // the bottom half of the row already shows the next frame
        let half = (9 + 14) * w * 3;
        rgb[half..].copy_from_slice(&lower[half..]);
        assert_eq!(read_strip(&rgb, w, h, (8, 9, 257, 28)), None);
    }

    #[test]
    fn finds_the_strip_in_a_film() {
        let (w, h) = (320, 60);
        let (x0, y0, cw, chh) = (11.3, 23, 14.45, 29);
        let film: Vec<Vec<u8>> = (0..300u16)
            .map(|n| frame(w, h, x0, y0, cw, chh, n / 2, true, false))
            .collect();
        let strip = locate_strip(&film, w, h).expect("strip");
        assert!((strip.0 as f64 - x0).abs() <= 1.0, "{strip:?}");
        assert_eq!((strip.1, strip.3), (y0, chh), "{strip:?}");
        assert!((strip.2 as f64 - 18.0 * cw).abs() <= 2.0, "{strip:?}");
        for (i, f) in film.iter().enumerate() {
            assert_eq!(read_strip(f, w, h, strip), Some(i as u16 / 2));
        }
    }

    /// Paints `rgb` white or black over a rectangle.
    fn paint(rgb: &mut [u8], w: usize, (x, y, rw, rh): (usize, usize, usize, usize), white: bool) {
        for yy in y..y + rh {
            for xx in x..x + rw {
                let p = (yy * w + xx) * 3;
                rgb[p..p + 3].fill(if white { 255 } else { 0 });
            }
        }
    }

    /// As Apple's Terminal shows it in dark mode: a title bar whose white title changes once
    /// during the film, a picture beside the strip with hard white and black edges in every
    /// frame, and something moving in the row below the strip.
    #[test]
    fn finds_the_strip_under_a_changing_title_and_beside_a_picture() {
        let (w, h) = (340, 110);
        let (x0, y0, cw, chh) = (10.0, 40, 14.0, 28);
        let film: Vec<Vec<u8>> = (0..400u16)
            .map(|i| {
                let mut f = frame(w, h, x0, y0, cw, chh, i / 3, true, false);
                // the title: letters that change when the program starts
                for k in 0..12 {
                    let on = (k % 3 == 0) != (i >= 40);
                    paint(&mut f, w, (20 + k * 18, 8, 9, 14), on);
                }
                // the picture beside the strip: still stripes, white and black
                for k in 0..8 {
                    paint(&mut f, w, (270 + k * 8, y0, 4, chh), true);
                }
                // something moving in the row below
                paint(&mut f, w, ((i as usize * 3) % 300, y0 + chh, 20, 20), true);
                f
            })
            .collect();
        let strip = locate_strip(&film, w, h).expect("strip");
        assert!((strip.0 as f64 - x0).abs() <= 1.0, "{strip:?}");
        assert_eq!((strip.1, strip.3), (y0, chh), "{strip:?}");
        assert!((strip.2 as f64 - 18.0 * cw).abs() <= 2.0, "{strip:?}");
        for (i, f) in film.iter().enumerate() {
            assert_eq!(read_strip(f, w, h, strip), Some(i as u16 / 3));
        }
    }

    /// As Ghostty shows it, from a real film: no frame around the window, a title bar of 32
    /// points whose white title changes when the program starts, two points of padding in
    /// Ghostty's own dark grey (not black) around a black grid, so the strip starts 4 pixels
    /// from the window's left edge and 68 down; cells 14 pixels wide and 32 tall; and the
    /// ticker's bar moving through the rows below.
    #[test]
    fn finds_the_strip_in_a_ghostty_window() {
        let (w, h) = (330, 150);
        let (x0, y0, cw, chh) = (4usize, 68usize, 14.0, 32);
        let grey = [40u8, 44, 52];
        let film: Vec<Vec<u8>> = (0..400u16)
            .map(|i| {
                let strip = frame(w, h, x0 as f64, y0, cw, chh, i / 2, true, false);
                let mut f = vec![0u8; w * h * 3];
                for y in 0..h {
                    for x in 0..w {
                        let p = (y * w + x) * 3;
                        if x < x0 || y < y0 {
                            f[p..p + 3].copy_from_slice(&grey);
                        } else if y < y0 + chh {
                            f[p..p + 3].copy_from_slice(&strip[p..p + 3]);
                        }
                    }
                }
                // the title, in the middle of the bar: it changes when the program starts
                for k in 0..10 {
                    let on = (k % 2 == 0) != (i >= 30);
                    if on {
                        paint(&mut f, w, (100 + k * 12, 22, 8, 18), true);
                    }
                }
                // the ticker's bar, a cell further right each frame, in the third row down
                let bar = x0 + (i as usize % 20) * 14;
                paint(&mut f, w, (bar, y0 + 2 * chh, 14, h - y0 - 2 * chh), true);
                f
            })
            .collect();
        let strip = locate_strip(&film, w, h).expect("strip");
        assert!((strip.0 as f64 - x0 as f64).abs() <= 1.0, "{strip:?}");
        assert_eq!((strip.1, strip.3), (y0, chh), "{strip:?}");
        assert!((strip.2 as f64 - 18.0 * cw).abs() <= 2.0, "{strip:?}");
        for (i, f) in film.iter().enumerate() {
            assert_eq!(read_strip(f, w, h, strip), Some(i as u16 / 2));
        }
    }

    #[test]
    fn names_the_terminals() {
        assert_eq!(Terminal::parse("apple"), Some(Terminal::Apple));
        assert_eq!(Terminal::parse("ghostty"), Some(Terminal::Ghostty));
        assert_eq!(Terminal::parse("iterm"), None);
        assert_eq!(Terminal::default(), Terminal::Apple);
        assert_eq!(Terminal::Ghostty.owner(), "Ghostty");
        assert_eq!(Terminal::Apple.owner(), "Terminal");
    }

    #[test]
    fn a_film_without_a_counting_strip_has_none() {
        let (w, h) = (300, 60);
        // the marks are there, but the number never changes
        let film: Vec<Vec<u8>> = (0..50)
            .map(|_| frame(w, h, 7.6, 9, 14.25, 28, 0x00F0, true, false))
            .collect();
        assert_eq!(locate_strip(&film, w, h), None);
    }

    /// Needs the Mac's screen. Run by hand through `rigs/apple-terminal/with-screen.sh`.
    #[test]
    #[ignore]
    fn films_the_ticker_in_apple_terminal() {
        films_the_ticker(Terminal::Apple);
    }

    /// Needs the Mac's screen and Ghostty. Run by hand through
    /// `rigs/apple-terminal/with-screen.sh`.
    #[test]
    #[ignore]
    fn films_the_ticker_in_ghostty() {
        films_the_ticker(Terminal::Ghostty);
    }

    fn films_the_ticker(terminal: Terminal) {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let built = Command::new(cargo)
            .args([
                "build",
                "-q",
                "-p",
                "phosphor-witness",
                "--bin",
                "witness-ticker",
            ])
            .status()
            .expect("cargo");
        assert!(built.success());
        let exe = std::env::current_exe().unwrap();
        let ticker = exe
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("witness-ticker");
        let film = film(&FilmOptions {
            terminal,
            font: 12.0,
            cols: 160,
            rows: 45,
            seconds: 6.0,
            fps: 120.0,
            command: vec![
                ticker.to_string_lossy().into(),
                "--seconds".into(),
                "30".into(),
                "--cols".into(),
                "160".into(),
            ],
        })
        .expect("film");
        println!("{}", film.to_json());
        let c = &film.cadence;
        assert!(c.shown > 60, "{c:?}");
        assert!(c.film_frames > 0);
    }
}
