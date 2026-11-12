// Phosphor rig: how far can a plain terminal be pushed as a true-colour pixel screen?
// Half blocks give two dots per cell, each with its own colour. Throwaway. `rustc -O fb.rs`.
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::process::Command;
use std::time::{Duration, Instant};

fn stty(tty: &File, args: &[&str]) -> String {
    let out = Command::new("stty").args(args).stdin(tty.try_clone().unwrap()).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
fn read_until(tty: &mut File, end: u8, max: Duration) -> Vec<u8> {
    let mut v = Vec::new();
    let t = Instant::now();
    let mut b = [0u8; 256];
    while t.elapsed() < max {
        let n = tty.read(&mut b).unwrap_or(0);
        v.extend_from_slice(&b[..n]);
        if v.contains(&end) {
            break;
        }
    }
    v
}
fn show(v: &[u8]) -> String {
    String::from_utf8_lossy(v).replace('\x1b', "ESC")
}
fn num(b: &mut Vec<u8>, mut n: usize) {
    let mut t = [0u8; 8];
    let mut i = 8;
    loop {
        i -= 1;
        t[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    b.extend_from_slice(&t[i..]);
}
fn cup(b: &mut Vec<u8>, r: usize, c: usize) {
    b.extend_from_slice(b"\x1b[");
    num(b, r + 1);
    b.push(b';');
    num(b, c + 1);
    b.push(b'H');
}

// The test picture, in dots. w x h is the whole canvas; (x, y) may run past it and wraps.
fn pix(x: usize, y: usize, w: usize, h: usize) -> u32 {
    let (x, y) = (x % w, y % h);
    if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
        return 0xffffff;
    }
    let third = w / 3;
    if x < third {
        // smooth colour: any banding means the terminal is not really using full colour
        let r = 255 * x / third;
        let g = 255 * y / h;
        return (r as u32) << 16 | (g as u32) << 8 | 160;
    }
    if x < 2 * third {
        if y < h / 2 {
            return if (x + y) & 1 == 0 { 0xffffff } else { 0x000000 }; // one-dot checks
        }
        return if (x + y) % 8 == 0 { 0x33ff66 } else { 0x06120b }; // one-dot diagonals
    }
    // rings: round only if dots are square
    let (dx, dy) = (x as i64 - (2 * third + third / 2) as i64, y as i64 - h as i64 / 2);
    let d = ((dx * dx + dy * dy) as f64).sqrt() as usize;
    if d % 16 < 8 { 0xffb000 } else { 0x202020 }
}

// ---- the price sweep ----------------------------------------------------------------------
// Paints the same kind of frame over and over while changing one thing at a time, and writes one
// CSV row per combination. Every frame is new: every cell of every changing row gets a colour
// that differs from the one before it and from the last frame. Nothing here looks like a
// picture; it only counts. A run ends wherever the ink or paper differs from the cell before,
// and the first cell of a row starts one.
// A colour code to send: 24-bit red, green, blue; or a 256-colour index from the 6x6x6 cube.
// Colours come from a palette of 7 that shifts every frame, as a picture of a few colours
// would. Ink and paper sit 3 apart in the palette, and neighbouring runs 2 apart, so a new run
// always changes both.
fn pick(c256: bool, run: u64, row: u64, frame: u64, paper: u64) -> u32 {
    let j = (run * 2 + row + frame + paper * 3) % 7;
    let (r, g, b) = ((j * 83 + 20) & 255, (j * 151 + 60) & 255, (j * 37 + 200) & 255);
    if c256 {
        let q = |v: u64| (v * 5 + 127) / 255;
        (16 + 36 * q(r) + 6 * q(g) + q(b)) as u32
    } else {
        (r << 16 | g << 8 | b) as u32
    }
}
fn code(b: &mut Vec<u8>, c256: bool, base: &[u8], c: u32) {
    b.extend_from_slice(base);
    if c256 {
        b.extend_from_slice(b";5;");
        num(b, c as usize);
    } else {
        b.extend_from_slice(b";2;");
        num(b, (c >> 16 & 255) as usize);
        b.push(b';');
        num(b, (c >> 8 & 255) as usize);
        b.push(b';');
        num(b, (c & 255) as usize);
    }
}
struct Tally {
    cells: usize,
    runs: usize,
    crowding: usize,
}
// One frame: `runs_per_row` runs across each changing row, every `step`th row changing.
fn sweep_frame(b: &mut Vec<u8>, c256: bool, cols: usize, rows: usize, runs_per_row: usize, step: usize, frame: u64) -> Tally {
    let mut t = Tally { cells: 0, runs: 0, crowding: 0 };
    let mut last = (u32::MAX, u32::MAX);
    for r in (0..rows).step_by(step) {
        cup(b, r, 0);
        for k in 0..runs_per_row {
            let len = (k + 1) * cols / runs_per_row - k * cols / runs_per_row;
            let (mut ink, paper) = (pick(c256, k as u64, r as u64, frame, 0), pick(c256, k as u64, r as u64, frame, 1));
            // a run ends where ink or paper differs from the cell before: never repeat the last pair
            while ink == last.0 && paper == last.1 {
                ink = if c256 { 16 + (ink - 16 + 1) % 216 } else { ink ^ 0x010101 };
            }
            b.extend_from_slice(b"\x1b[");
            code(b, c256, b"38", ink);
            b.push(b';');
            code(b, c256, b"48", paper);
            b.push(b'm');
            for _ in 0..len {
                b.extend_from_slice("\u{2580}".as_bytes());
            }
            last = (ink, paper);
        }
        t.cells += cols;
        t.runs += runs_per_row;
        t.crowding += runs_per_row * runs_per_row;
    }
    t
}
fn sweep(tty: &mut File, path: &str, font: &str, cols: usize, rows: usize) -> std::io::Result<String> {
    let mut csv = OpenOptions::new().create(true).append(true).open(path)?;
    if csv.metadata()?.len() == 0 {
        csv.write_all(b"font,cols,rows,depth,runs_per_row,rows_changed,bytes,cells,runs,crowding,frames,seconds,median_ms,p99_ms\n")?;
    }
    let mut per_row: Vec<usize> = vec![1, 2, 4, 8, 16, 32, 64];
    per_row.retain(|&n| n < cols);
    per_row.push(cols);
    let mut done = 0;
    let mut b: Vec<u8> = Vec::with_capacity(64 << 20);
    for (depth, c256) in [("full", false), ("256", true)] {
        for step in [1usize, 4] {
            for &n in &per_row {
                // start every combination from a clean, answered screen
                tty.write_all(b"\x1b[0m\x1b[2J\x1b[6n")?;
                read_until(tty, b'R', Duration::from_secs(20));
                let (mut bytes, mut frames, mut tally) = (0usize, 0u64, Tally { cells: 0, runs: 0, crowding: 0 });
                let mut lat: Vec<f64> = Vec::new();
                let start = Instant::now();
                while frames < 120 && start.elapsed() < Duration::from_secs(3) {
                    frames += 1;
                    b.clear();
                    tally = sweep_frame(&mut b, c256, cols, rows, n, step, frames);
                    b.extend_from_slice(b"\x1b[6n");
                    bytes += b.len();
                    let t = Instant::now();
                    tty.write_all(&b)?;
                    read_until(tty, b'R', Duration::from_secs(20));
                    lat.push(t.elapsed().as_secs_f64() * 1e3);
                }
                let seconds = start.elapsed().as_secs_f64();
                lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let (median, p99) = (lat[lat.len() / 2], lat[(lat.len() * 99 / 100).min(lat.len() - 1)]);
                let line = format!("{font},{cols},{rows},{depth},{n},{},{},{},{},{},{frames},{seconds:.3},{median:.3},{p99:.3}\n", rows.div_ceil(step), bytes / frames as usize, tally.cells, tally.runs, tally.crowding);
                csv.write_all(line.as_bytes())?;
                csv.flush()?;
                done += 1;
            }
        }
    }
    tty.write_all(b"\x1b[0m\x1b[2J")?;
    Ok(format!("sweep: {done} combinations written to {path}\n"))
}

struct Painter {
    b: Vec<u8>,
    fg: u32,
    bg: u32,
    c256: bool,
    c16: bool,    // eight plain colours, the shortest colour codes there are
    always: bool, // send both colours for every cell, even when unchanged
    pad: bool,    // send every colour instruction twice: more bytes, same picture
    mono: bool,   // fixed colours; only the shape in each cell changes
    runs: usize,  // colour changes only every `runs` cells
    phase: usize,
}
impl Painter {
    fn colour(&mut self, base: &[u8], c: u32) {
        if self.c16 {
            let bit = |sh: u32| ((c >> sh & 255) > 127) as u8;
            self.b.push(base[0]);
            self.b.push(b'0' + (bit(16) | bit(8) << 1 | bit(0) << 2));
        } else if self.c256 {
            let q = |v: u32| (v * 5 + 127) / 255;
            self.b.extend_from_slice(base);
            self.b.extend_from_slice(b";5;");
            num(&mut self.b, (16 + 36 * q(c >> 16 & 255) + 6 * q(c >> 8 & 255) + q(c & 255)) as usize);
        } else {
            self.b.extend_from_slice(base);
            self.b.extend_from_slice(b";2;");
            num(&mut self.b, (c >> 16 & 255) as usize);
            self.b.push(b';');
            num(&mut self.b, (c >> 8 & 255) as usize);
            self.b.push(b';');
            num(&mut self.b, (c & 255) as usize);
        }
    }
    // one cell: top dot in the ink colour, bottom dot in the paper colour
    fn cell(&mut self, top: u32, bot: u32) {
        let (nf, nb) = (self.always || self.fg != top, self.always || self.bg != bot);
        for _ in 0..if self.pad { 2 } else { 1 } {
            if !(nf || nb) {
                break;
            }
            self.b.extend_from_slice(b"\x1b[");
            if nf {
                self.colour(b"38", top);
                self.fg = top;
            }
            if nb {
                if nf {
                    self.b.push(b';');
                }
                self.colour(b"48", bot);
                self.bg = bot;
            }
            self.b.push(b'm');
        }
        self.b.extend_from_slice("▀".as_bytes());
    }
    // a whole screen row r showing canvas rows starting at dot (ox, oy)
    fn row(&mut self, r: usize, src_r: usize, cols: usize, rows: usize, ox: usize, oy: usize) {
        cup(&mut self.b, r, 0);
        if self.mono {
            if self.fg != 0xffffff || self.bg != 0 {
                self.b.extend_from_slice(b"\x1b[38;2;255;255;255;48;2;0;0;0m");
                self.fg = 0xffffff;
                self.bg = 0;
            }
            for c in 0..cols {
                self.b.extend_from_slice(if (c + r + self.phase) & 1 == 0 { "▀" } else { "▄" }.as_bytes());
            }
            return;
        }
        for c in 0..cols {
            let x = c / self.runs * self.runs;
            let (t, b) = (pix(x + ox, 2 * src_r + oy, cols, 2 * rows), pix(x + ox, 2 * src_r + 1 + oy, cols, 2 * rows));
            self.cell(t, b);
        }
    }
    fn frame(&mut self, cols: usize, rows: usize, ox: usize, oy: usize) {
        for r in 0..rows {
            self.row(r, r, cols, rows, ox, oy);
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opt = |k: &str, d: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned().unwrap_or(d.to_string());
    let dir = opt("--dir", ".");
    let hold = Duration::from_millis(opt("--hold", "2500").parse().unwrap());
    let c256 = a.iter().any(|x| x == "--c256");
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty").expect("needs a terminal");
    let saved = stty(&tty, &["-g"]);
    std::thread::sleep(Duration::from_millis(900));
    stty(&tty, &["raw", "-echo", "min", "0", "time", "1"]);
    let mut rep = String::new();
    let stage = |n: &str| std::fs::write(format!("{dir}/stage-{n}"), "").unwrap();
    let mut go = || -> std::io::Result<()> {
        tty.write_all(b"\x1b[?1049h\x1b[?25l\x1b[0m\x1b[2J")?;
        let size = stty(&tty, &["size"]);
        let mut it = size.split_whitespace().map(|x| x.parse::<usize>().unwrap());
        let (rows, cols) = (it.next().unwrap(), it.next().unwrap());
        tty.write_all(b"\x1b[?2026$p\x1b[14t\x1b[16t\x1b[>q\x1b[c")?;
        let caps = show(&read_until(&mut tty, b'c', Duration::from_secs(2)));
        let sync = caps.contains("?2026;1$y") || caps.contains("?2026;2$y");
        rep += &format!("grid {cols}x{rows} cells = {} cells; canvas {}x{} dots\nreplies {caps}\natomic frames {sync}\n", cols * rows, cols, rows * 2);
        for (name, s) in [("half", "▀▄"), ("quarter", "▚▞"), ("sixth", "\u{1FB00}\u{1FB01}"), ("eighth", "\u{1CD00}\u{1CD01}")] {
            tty.write_all(format!("\x1b[1;1H{s}\x1b[6n").as_bytes())?;
            let r = show(&read_until(&mut tty, b'R', Duration::from_millis(600)));
            rep += &format!("{name} blocks take one cell each: {}\n", r.contains(";3R"));
        }
        tty.write_all(b"\x1b[1;1Hx\x1b[3b\x1b[6n")?;
        rep += &format!("repeat command works: {}\n", show(&read_until(&mut tty, b'R', Duration::from_millis(600))).contains(";5R"));
        let has = |k: &str| std::env::args().any(|x| x == k);
        let sweep_to = opt("--sweep", "");
        if !sweep_to.is_empty() {
            rep += &sweep(&mut tty, &sweep_to, &opt("--font", "0"), cols, rows)?;
            return Ok(());
        }
        let mut p = Painter { b: Vec::with_capacity(64 << 20), fg: 1 << 30, bg: 1 << 30, c256, c16: has("--c16"), always: has("--always"), pad: has("--pad"), mono: has("--mono"), runs: opt("--runs", "1").parse().unwrap(), phase: 0 };
        let (on, off): (&[u8], &[u8]) = if sync { (b"\x1b[?2026h", b"\x1b[?2026l") } else { (b"", b"") };
        let mut send = |tty: &mut File, p: &mut Painter| -> std::io::Result<(usize, f64, bool)> {
            p.b.extend_from_slice(b"\x1b[6n");
            let t = Instant::now();
            tty.write_all(&p.b)?;
            let ok = read_until(tty, b'R', Duration::from_secs(20)).contains(&b'R');
            let n = p.b.len();
            p.b.clear();
            Ok((n, t.elapsed().as_secs_f64(), ok))
        };
        // 1. one whole picture
        p.b.extend_from_slice(on);
        p.frame(cols, rows, 0, 0);
        p.b.extend_from_slice(off);
        let (n, secs, ok) = send(&mut tty, &mut p)?;
        rep += &format!("whole picture: {:.2} MB in {:.3}s = {:.1} MB/s (answered {ok})\n", n as f64 / 1e6, secs, n as f64 / 1e6 / secs);
        stage("1");
        std::thread::sleep(hold);
        if std::env::args().any(|x| x == "--fill-only") {
            return Ok(());
        }
        if std::env::args().any(|x| x == "--sprite") {
            // a small white square sliding over the still picture: only its own rows and columns change
            for (sw, sh) in [(16usize, 8usize), (64, 32)] {
                let (mut bytes, start, mut steps) = (0usize, Instant::now(), 0usize);
                let mut lat = Vec::new();
                while steps < 600 && start.elapsed() < Duration::from_secs(5) {
                    let (x, y) = (2 + steps % (cols - sw - 4), 2 + (steps / 2) % (rows - sh - 4));
                    p.b.extend_from_slice(on);
                    for r in y.saturating_sub(1)..y + sh + 1 {
                        cup(&mut p.b, r, x - 1);
                        for c in x - 1..x + sw + 1 {
                            let inside = r >= y && r < y + sh && c >= x && c < x + sw;
                            if inside { p.cell(0xffffff, 0xffffff) } else { p.cell(pix(c, 2 * r, cols, 2 * rows), pix(c, 2 * r + 1, cols, 2 * rows)) }
                        }
                    }
                    p.b.extend_from_slice(off);
                    let (n, secs, _) = send(&mut tty, &mut p)?;
                    bytes += n;
                    lat.push(secs * 1e3);
                    steps += 1;
                }
                let secs = start.elapsed().as_secs_f64();
                lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
                rep += &format!("small thing moving ({sw}x{sh} cells) over the still picture: {:.1} steps/s, {:.1} kB a step, answer median {:.1} ms, 99th {:.1} ms\n", steps as f64 / secs, bytes as f64 / steps as f64 / 1e3, lat[lat.len() / 2], lat[lat.len() * 99 / 100]);
            }
            stage("2");
            std::thread::sleep(hold);
            return Ok(());
        }
        // 2. every cell changing, back to back: the picture sliding one dot diagonally a frame
        let (mut bytes, start, mut frames) = (0usize, Instant::now(), 0);
        while frames < 600 && start.elapsed() < Duration::from_secs(6) {
            frames += 1;
            p.phase = frames;
            p.b.extend_from_slice(on);
            p.frame(cols, rows, frames, frames);
            p.b.extend_from_slice(off);
            bytes += send(&mut tty, &mut p)?.0;
        }
        let secs = start.elapsed().as_secs_f64();
        rep += &format!("every cell changing: {frames} frames in {secs:.2}s = {:.1} frames/s, {:.1} MB/s, {:.0} kB a frame\n", frames as f64 / secs, bytes as f64 / 1e6 / secs, bytes as f64 / frames as f64 / 1e3);
        if std::env::args().any(|x| x == "--anim-only") {
            return Ok(());
        }
        // 3. let the terminal move the picture itself: scroll up a row, paint only the new bottom row
        p.b.extend_from_slice(on);
        p.frame(cols, rows, 0, 0);
        p.b.extend_from_slice(off);
        send(&mut tty, &mut p)?;
        let (mut bytes, start, mut steps) = (0usize, Instant::now(), 0);
        while steps < rows / 3 && start.elapsed() < Duration::from_secs(6) {
            steps += 1;
            p.b.extend_from_slice(on);
            cup(&mut p.b, rows - 1, 0);
            p.b.extend_from_slice(b"\x1b[0m\n");
            p.fg = 1 << 30;
            p.bg = 1 << 30;
            p.row(rows - 1, (steps - 1) % rows, cols, rows, 0, 0);
            p.b.extend_from_slice(off);
            bytes += send(&mut tty, &mut p)?.0;
        }
        let secs = start.elapsed().as_secs_f64();
        rep += &format!("terminal scrolls up, one row painted: {steps} steps in {secs:.2}s = {:.1} steps/s, {:.1} kB a step\n", steps as f64 / secs, bytes as f64 / steps as f64 / 1e3);
        stage("2");
        std::thread::sleep(hold);
        // 4. sideways: delete one character at the start of every row, paint the new right-hand column
        p.b.extend_from_slice(on);
        p.frame(cols, rows, 0, 0);
        p.b.extend_from_slice(off);
        send(&mut tty, &mut p)?;
        let (mut bytes, start, mut steps) = (0usize, Instant::now(), 0);
        while steps < cols / 3 && start.elapsed() < Duration::from_secs(6) {
            steps += 1;
            p.b.extend_from_slice(on);
            for r in 0..rows {
                cup(&mut p.b, r, 0);
                p.b.extend_from_slice(b"\x1b[P");
                cup(&mut p.b, r, cols - 1);
                p.cell(pix(steps - 1, 2 * r, cols, 2 * rows), pix(steps - 1, 2 * r + 1, cols, 2 * rows));
            }
            p.b.extend_from_slice(off);
            bytes += send(&mut tty, &mut p)?.0;
        }
        let secs = start.elapsed().as_secs_f64();
        rep += &format!("terminal shifts left, one column painted: {steps} steps in {secs:.2}s = {:.1} steps/s, {:.1} kB a step\n", steps as f64 / secs, bytes as f64 / steps as f64 / 1e3);
        stage("3");
        std::thread::sleep(hold);
        Ok(())
    };
    let res = go();
    let _ = tty.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
    stty(&tty, &[&saved]);
    if let Err(e) = res {
        rep += &format!("error {e}\n");
    }
    std::fs::write(format!("{dir}/report.txt"), &rep).unwrap();
}
