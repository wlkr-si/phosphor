//! Stills: photograph the test window and read back the dots it shows.
//!
//! `shoot` opens Apple's Terminal at a chosen font size and grid, runs one program, and
//! photographs only that window. `read_dots` finds the four registration marks in the
//! photograph and reads the picture seen. `compare` measures its fidelity against the picture
//! meant. See "Registration marks" in docs/architecture.md.

use phosphor_kernel::{colour_distance, Canvas, Ladder, Rgb};
use std::io::Cursor;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The paper colour of a registration mark.
pub const MARK: Rgb = 0xFF00FF;

/// A test window: Apple's Terminal at one font size and grid, running one command.
#[derive(Clone, Debug)]
pub struct Window {
    pub font_size: f32,
    pub cols: usize,
    pub rows: usize,
    /// A shell command line. It runs from the home folder, so give programs by full path.
    pub command: String,
}

/// A photograph: `w` by `h` pixels, three bytes each, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Photo {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
}

impl Photo {
    fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.w + x) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

/// How closely the picture seen matches the picture meant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fidelity {
    /// Dots compared.
    pub dots: usize,
    /// Dots where some channel differs by more than the tolerance.
    pub wrong: usize,
    /// The largest difference in any one channel of any compared dot, 0 to 255.
    pub worst: u32,
    /// The first wrong dot, as (x, y), counting along rows from the top left.
    pub first_wrong: Option<(usize, usize)>,
}

/// The folder of Apple's Terminal rig scripts.
fn rigs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rigs/apple-terminal")
}

/// Quote a string for /bin/sh.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The number of Apple's Terminal's largest on-screen window, from `winid.js`.
fn largest_window() -> Option<u64> {
    let out = Command::new("osascript")
        .args(["-l", "JavaScript"])
        .arg(rigs().join("winid.js"))
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Ask Terminal something in AppleScript and return its answer.
fn ask_terminal(script: &str) -> Option<String> {
    let out = Command::new("osascript")
        .args([
            "-e",
            &format!("tell application \"Terminal\"\n{script}\nend tell"),
        ])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The number of the Terminal window whose tab is on `tty`. Other windows may be open on the
/// shared screen, so the test window is found by its own terminal line. Terminal's window id
/// is the window number `screencapture -l` takes.
fn window_on(tty: &str) -> Option<u64> {
    ask_terminal(&format!(
        "repeat with w in windows\nrepeat with t in tabs of w\n\
         if tty of t is \"{tty}\" then return id of w\nend repeat\nend repeat\nreturn \"\""
    ))?
    .parse()
    .ok()
}

/// True while Terminal still has window `n`.
fn window_open(n: u64) -> bool {
    ask_terminal(&format!("exists window id {n}")).as_deref() == Some("true")
}

/// The line spacing that makes a Menlo cell in Apple's Terminal one em tall.
///
/// Terminal draws block characters from the font's em square, not from the cell. At the
/// usual spacing of 1.0 a cell is taller than an em, so `▀` covers only the middle of the
/// cell: at font 12, pixel rows 5.5 to 17.5 of a 30-pixel cell (Retina). The half rung then
/// cannot split a cell evenly. When the cell is exactly one em tall, `▀` covers the top
/// half. Terminal rounds cell heights, so the spacing that does this was measured per font
/// size, on macOS 27.2 with Terminal 2.15. Sizes not measured keep 1.0.
pub fn line_spacing(font_size: f32) -> f64 {
    if font_size == 6.0 {
        0.75
    } else if font_size == 12.0 {
        0.8
    } else {
        1.0
    }
}

/// Open the window, wait `after_ms`, photograph only that window, then close it.
pub fn shoot(window: &Window, after_ms: u64) -> Result<Photo, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("phosphor-witness-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("making {}: {e}", dir.display()))?;
    let result = shoot_in(window, after_ms, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn shoot_in(window: &Window, after_ms: u64, dir: &Path) -> Result<Photo, String> {
    let settings = dir.join("PhosphorWitness.terminal");
    let pid_file = dir.join("pid");
    let tty_file = dir.join("tty");
    let shot = dir.join("shot.png");
    // The program records its terminal line, to find its window, and its process number, to
    // close the window once photographed.
    let inner = format!(
        "tty > {}; echo $$ > {}; exec {}",
        sh_quote(&tty_file.to_string_lossy()),
        sh_quote(&pid_file.to_string_lossy()),
        window.command
    );
    let command = format!("/bin/sh -c {}; exit", sh_quote(&inner));
    let made = Command::new("osascript")
        .args(["-l", "JavaScript"])
        .arg(rigs().join("mkprofile.js"))
        .arg(&settings)
        .arg("Phosphor Witness")
        .arg("Menlo-Regular")
        .arg(window.font_size.to_string())
        .arg(window.cols.to_string())
        .arg(window.rows.to_string())
        .arg(&command)
        .output()
        .map_err(|e| format!("osascript: {e}"))?;
    if !made.status.success() {
        return Err(format!(
            "mkprofile.js failed: {}",
            String::from_utf8_lossy(&made.stderr)
        ));
    }
    let spacing = line_spacing(window.font_size);
    let spaced = Command::new("plutil")
        .args([
            "-replace",
            "FontHeightSpacing",
            "-float",
            &spacing.to_string(),
        ])
        .arg(&settings)
        .status()
        .map_err(|e| format!("plutil: {e}"))?;
    if !spaced.success() {
        return Err("plutil could not set the line spacing".into());
    }
    let opened = Command::new("open")
        .args(["-a", "Terminal"])
        .arg(&settings)
        .status()
        .map_err(|e| format!("open: {e}"))?;
    if !opened.success() {
        return Err("open -a Terminal failed".into());
    }

    // Wait for the program to start, then for the picture to settle.
    let started = Instant::now();
    let read_pid = || {
        std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    };
    let mut pid = read_pid();
    while pid.is_none() && started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(50));
        pid = read_pid();
    }
    let tty = std::fs::read_to_string(&tty_file).unwrap_or_default();
    std::thread::sleep(Duration::from_millis(after_ms));

    let number = window_on(tty.trim()).or_else(largest_window);
    let captured = match number {
        Some(n) => Command::new("screencapture")
            .args(["-x", "-o", "-l", &n.to_string()])
            .arg(&shot)
            .status()
            .map_err(|e| format!("screencapture: {e}"))
            .and_then(|s| {
                if s.success() {
                    std::fs::read(&shot).map_err(|e| format!("reading the photograph: {e}"))
                } else {
                    Err("screencapture failed".into())
                }
            }),
        None => Err("no Terminal window found".into()),
    };

    // Close: end the program; its window closes itself when the command exits.
    if let Some(pid) = pid {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    if let Some(n) = number {
        let closing = Instant::now();
        while window_open(n) && closing.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if pid.is_none() {
        return Err("the command never started".into());
    }
    load_photo(&captured?)
}

/// Decode a PNG into a photograph. Any alpha is dropped.
pub fn load_photo(png: &[u8]) -> Result<Photo, String> {
    let mut decoder = png::Decoder::new(Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("png: {e}"))?;
    let size = reader.output_buffer_size().ok_or("png: image too large")?;
    let mut buf = vec![0; size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("png: {e}"))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("png: palette not expanded".into()),
    };
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("png: unexpected bit depth {:?}", info.bit_depth));
    }
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let line = &buf[y * info.line_size..][..w * channels];
        for px in line.chunks_exact(channels) {
            if channels < 3 {
                rgb.extend_from_slice(&[px[0], px[0], px[0]]);
            } else {
                rgb.extend_from_slice(&px[..3]);
            }
        }
    }
    Ok(Photo { w, h, rgb })
}

/// Encode a photograph as a PNG.
pub fn save_photo(photo: &Photo) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, photo.w as u32, photo.h as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| format!("png: {e}"))?;
    writer
        .write_image_data(&photo.rgb)
        .map_err(|e| format!("png: {e}"))?;
    writer.finish().map_err(|e| format!("png: {e}"))?;
    Ok(out)
}

/// True for a pixel near the registration mark's magenta, allowing for the display's colour
/// handling. Nothing in the test card comes close: its blues stay below 0xC0 wherever red is
/// high and green low.
fn is_mark(p: [u8; 3]) -> bool {
    p[0] >= 0xC0 && p[2] >= 0xC0 && p[1] <= 0x60
}

/// The grid's rectangle in the photograph, as (left, top, right, bottom) pixel edges: the
/// outer edges of the four registration marks.
fn find_grid(photo: &Photo) -> Result<(usize, usize, usize, usize), String> {
    // Count only pixels inside a solid patch of mark colour, so a stray pixel (the edge of a
    // window button, say) cannot widen the grid; then step back out to the patch's edge.
    let solid = |x: usize, y: usize| {
        (y - 1..=y + 1).all(|j| (x - 1..=x + 1).all(|i| is_mark(photo.pixel(i, j))))
    };
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
    for y in 1..photo.h.saturating_sub(1) {
        for x in 1..photo.w.saturating_sub(1) {
            if solid(x, y) {
                left = left.min(x - 1);
                top = top.min(y - 1);
                right = right.max(x + 2);
                bottom = bottom.max(y + 2);
            }
        }
    }
    if left == usize::MAX {
        return Err("no registration marks in the photograph".into());
    }
    // All four marks must be there: each corner of the rectangle must be a mark.
    for (x, y) in [
        (left, top),
        (right - 1, top),
        (left, bottom - 1),
        (right - 1, bottom - 1),
    ] {
        if !is_mark(photo.pixel(x, y)) {
            return Err(format!(
                "registration marks incomplete: no mark at pixel ({x}, {y})"
            ));
        }
    }
    Ok((left, top, right, bottom))
}

/// The pixels whose centres lie in the middle half of [start, start + size).
fn middle_half(start: f64, size: f64, limit: usize) -> Range<usize> {
    let (a, b) = (start + size / 4.0, start + size * 3.0 / 4.0);
    // pixel i has its centre at i + 0.5
    let first = (a - 0.5).ceil().max(0.0) as usize;
    let end = ((b - 0.5).ceil().max(0.0) as usize).min(limit);
    if first < end {
        first..end
    } else {
        // A dot under two pixels across: take the pixel under its centre.
        let mid = ((start + size / 2.0).floor().max(0.0) as usize).min(limit - 1);
        mid..mid + 1
    }
}

/// Read the picture seen out of a photograph of a `cols` by `rows` grid on one rung.
pub fn read_dots(
    photo: &Photo,
    cols: usize,
    rows: usize,
    ladder: Ladder,
) -> Result<Canvas, String> {
    if cols == 0 || rows == 0 {
        return Err("the grid has no cells".into());
    }
    if photo.rgb.len() != photo.w * photo.h * 3 {
        return Err("photograph is the wrong length for its size".into());
    }
    let (left, top, right, bottom) = find_grid(photo)?;
    let (w, h) = ladder.canvas_size(cols, rows);
    let dot_w = (right - left) as f64 / w as f64;
    let dot_h = (bottom - top) as f64 / h as f64;
    if dot_w < 1.0 || dot_h < 1.0 {
        return Err(format!(
            "dots are smaller than a pixel ({dot_w:.2} x {dot_h:.2})"
        ));
    }
    // The display may have colour-matched the picture into Display P3. The marks are a known
    // colour, so they tell which.
    let cell_h = (bottom - top) as f64 / rows as f64;
    let cell_w = (right - left) as f64 / cols as f64;
    let mark = average(
        photo,
        middle_half(left as f64, cell_w, photo.w),
        middle_half(top as f64, cell_h, photo.h),
    );
    let p3 = colour_distance(mark, MARK_IN_P3) < colour_distance(mark, MARK);
    let mut seen = Canvas::new(w, h, 0);
    for y in 0..h {
        let ys = middle_half(top as f64 + y as f64 * dot_h, dot_h, photo.h);
        for x in 0..w {
            let xs = middle_half(left as f64 + x as f64 * dot_w, dot_w, photo.w);
            let c = average(photo, xs, ys.clone());
            seen.dots[y * w + x] = if p3 { p3_to_srgb(c) } else { c };
        }
    }
    Ok(seen)
}

/// The average colour of a rectangle of pixels.
fn average(photo: &Photo, xs: Range<usize>, ys: Range<usize>) -> Rgb {
    let mut sum = [0u64; 3];
    for py in ys.clone() {
        for px in xs.clone() {
            let p = photo.pixel(px, py);
            for c in 0..3 {
                sum[c] += p[c] as u64;
            }
        }
    }
    let n = (ys.len() * xs.len()).max(1) as u64;
    let avg = |c: usize| ((sum[c] + n / 2) / n) as u32;
    avg(0) << 16 | avg(1) << 8 | avg(2)
}

/// The registration mark as a Display P3 screen shows it.
const MARK_IN_P3: Rgb = 0xEA33F7;

/// Convert a Display P3 colour to sRGB. Both share sRGB's transfer curve and white point.
fn p3_to_srgb(c: Rgb) -> Rgb {
    let lin = |v: u32| {
        let v = v as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let enc = |v: f64| {
        let v = v.clamp(0.0, 1.0);
        let e = if v <= 0.003_130_8 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (e * 255.0).round() as u32
    };
    let (r, g, b) = (lin(c >> 16 & 255), lin(c >> 8 & 255), lin(c & 255));
    let sr = 1.224_940_2 * r - 0.224_940_4 * g;
    let sg = -0.042_056_9 * r + 1.042_057_1 * g;
    let sb = -0.019_637_6 * r - 0.078_636_1 * g + 1.098_273_5 * b;
    enc(sr) << 16 | enc(sg) << 8 | enc(sb)
}

/// The largest difference in any one channel between two colours.
fn channel_gap(a: Rgb, b: Rgb) -> u32 {
    [16, 8, 0]
        .iter()
        .map(|s| ((a >> s & 255) as i32 - (b >> s & 255) as i32).unsigned_abs())
        .max()
        .unwrap_or(0)
}

/// Compare the picture seen with the picture meant. A dot is wrong when any channel differs
/// by more than `tolerance`. `corners`, the rung the canvases were drawn on, leaves out the
/// dots under the four corner cells, where the registration marks sit; `None` compares every
/// dot. Canvases of different sizes are wholly wrong.
pub fn compare(meant: &Canvas, seen: &Canvas, tolerance: u32, corners: Option<Ladder>) -> Fidelity {
    if meant.w != seen.w || meant.h != seen.h {
        let dots = meant.dots.len().max(seen.dots.len());
        return Fidelity {
            dots,
            wrong: dots,
            worst: 255,
            first_wrong: Some((0, 0)),
        };
    }
    let (w, h) = (meant.w, meant.h);
    let corner = |x: usize, y: usize| {
        corners.is_some_and(|ladder| {
            let (dw, dh) = ladder.dots();
            (x < dw || x + dw >= w) && (y < dh || y + dh >= h)
        })
    };
    let mut f = Fidelity {
        dots: 0,
        wrong: 0,
        worst: 0,
        first_wrong: None,
    };
    for y in 0..h {
        for x in 0..w {
            if corner(x, y) {
                continue;
            }
            let gap = channel_gap(meant.dots[y * w + x], seen.dots[y * w + x]);
            f.dots += 1;
            f.worst = f.worst.max(gap);
            if gap > tolerance {
                f.wrong += 1;
                f.first_wrong.get_or_insert((x, y));
            }
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use phosphor_kernel::test_card;

    /// Paint the four corner cells of a half-rung canvas with the registration mark.
    fn with_marks(mut c: Canvas) -> Canvas {
        let (w, h) = (c.w as i64, c.h as i64);
        for (x, y) in [(0, 0), (w - 1, 0), (0, h - 2), (w - 1, h - 2)] {
            c.fill_rect(x, y, 1, 2, MARK);
        }
        c
    }

    /// A photograph of a canvas: cells `cell_w` by `cell_h` pixels, the grid starting at
    /// (`ox`, `oy`) inside a dark grey window. Each pixel takes the dot under its centre.
    fn photograph(
        c: &Canvas,
        ladder: Ladder,
        cell_w: f64,
        cell_h: f64,
        ox: usize,
        oy: usize,
    ) -> Photo {
        let (dw, dh) = ladder.dots();
        let (dot_w, dot_h) = (cell_w / dw as f64, cell_h / dh as f64);
        let gw = (c.w as f64 * dot_w).ceil() as usize;
        let gh = (c.h as f64 * dot_h).ceil() as usize;
        let (w, h) = (gw + 2 * ox, gh + 2 * oy);
        let mut rgb = vec![0x18; w * h * 3];
        for py in 0..h {
            for px in 0..w {
                let fx = (px as f64 + 0.5 - ox as f64) / dot_w;
                let fy = (py as f64 + 0.5 - oy as f64) / dot_h;
                if let Some(d) = c.get(fx.floor() as i64, fy.floor() as i64) {
                    if fx >= 0.0 && fy >= 0.0 {
                        let i = (py * w + px) * 3;
                        rgb[i] = (d >> 16) as u8;
                        rgb[i + 1] = (d >> 8) as u8;
                        rgb[i + 2] = d as u8;
                    }
                }
            }
        }
        Photo { w, h, rgb }
    }

    #[test]
    fn reads_back_a_canvas_from_fractional_cells() {
        let (cols, rows) = (40, 12);
        let (w, h) = Ladder::Half.canvas_size(cols, rows);
        let meant = with_marks(test_card(w, h, 0));
        let photo = photograph(&meant, Ladder::Half, 14.25, 30.8, 9, 52);
        let seen = read_dots(&photo, cols, rows, Ladder::Half).unwrap();
        assert_eq!(seen, meant);
    }

    #[test]
    fn reads_back_the_full_reference_grid() {
        let (cols, rows) = (160, 45);
        let (w, h) = Ladder::Half.canvas_size(cols, rows);
        let meant = with_marks(test_card(w, h, 3));
        let photo = photograph(&meant, Ladder::Half, 14.25, 30.8, 4, 4);
        assert_eq!(read_dots(&photo, cols, rows, Ladder::Half).unwrap(), meant);
    }

    #[test]
    fn reads_back_the_quarter_rung() {
        let (cols, rows) = (30, 10);
        let (w, h) = Ladder::Quarter.canvas_size(cols, rows);
        let mut meant = test_card(w, h, 0);
        for (x, y) in [(0, 0), (w - 2, 0), (0, h - 2), (w - 2, h - 2)] {
            meant.fill_rect(x as i64, y as i64, 2, 2, MARK);
        }
        let photo = photograph(&meant, Ladder::Quarter, 14.25, 30.8, 7, 7);
        assert_eq!(
            read_dots(&photo, cols, rows, Ladder::Quarter).unwrap(),
            meant
        );
    }

    #[test]
    fn a_missing_mark_is_an_error() {
        let (cols, rows) = (20, 8);
        let (w, h) = Ladder::Half.canvas_size(cols, rows);
        let mut meant = with_marks(test_card(w, h, 0));
        meant.fill_rect(w as i64 - 1, h as i64 - 2, 1, 2, 0);
        let photo = photograph(&meant, Ladder::Half, 14.25, 30.8, 5, 5);
        assert!(read_dots(&photo, cols, rows, Ladder::Half).is_err());
        let blank = Photo {
            w: 10,
            h: 10,
            rgb: vec![0; 300],
        };
        assert!(read_dots(&blank, cols, rows, Ladder::Half).is_err());
    }

    fn srgb_to_p3(c: Rgb) -> Rgb {
        let lin = |v: u32| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let enc = |v: f64| {
            let e = if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.clamp(0.0, 1.0).powf(1.0 / 2.4) - 0.055
            };
            (e * 255.0).round() as u32
        };
        let (r, g, b) = (lin(c >> 16 & 255), lin(c >> 8 & 255), lin(c & 255));
        let pr = 0.822_462_1 * r + 0.177_538 * g;
        let pg = 0.033_194_1 * r + 0.966_805_8 * g;
        let pb = 0.017_082_7 * r + 0.072_397_4 * g + 0.910_519_9 * b;
        enc(pr) << 16 | enc(pg) << 8 | enc(pb)
    }

    #[test]
    fn the_mark_tells_a_display_p3_photograph() {
        // Apple's Terminal on a Display P3 screen shows the mark as 0xEA33F7.
        assert_eq!(srgb_to_p3(MARK), MARK_IN_P3);
        let (cols, rows) = (40, 12);
        let (w, h) = Ladder::Half.canvas_size(cols, rows);
        let meant = with_marks(test_card(w, h, 0));
        let mut on_screen = meant.clone();
        for d in on_screen.dots.iter_mut() {
            *d = srgb_to_p3(*d);
        }
        let photo = photograph(&on_screen, Ladder::Half, 14.0, 24.0, 20, 78);
        let seen = read_dots(&photo, cols, rows, Ladder::Half).unwrap();
        let f = compare(&meant, &seen, 4, None);
        assert_eq!(f.wrong, 0, "{f:?}");
    }

    #[test]
    fn a_stray_magenta_pixel_does_not_move_the_grid() {
        let (cols, rows) = (20, 8);
        let (w, h) = Ladder::Half.canvas_size(cols, rows);
        let meant = with_marks(test_card(w, h, 0));
        let mut photo = photograph(&meant, Ladder::Half, 14.25, 30.8, 6, 6);
        photo.rgb[..3].copy_from_slice(&[0xFF, 0x00, 0xFF]);
        assert_eq!(read_dots(&photo, cols, rows, Ladder::Half).unwrap(), meant);
    }

    #[test]
    fn photographs_survive_png() {
        let meant = with_marks(test_card(20, 16, 0));
        let photo = photograph(&meant, Ladder::Half, 7.5, 15.0, 3, 3);
        let back = load_photo(&save_photo(&photo).unwrap()).unwrap();
        assert_eq!(back, photo);
    }

    #[test]
    fn compare_counts_wrong_dots() {
        let meant = test_card(40, 20, 0);
        assert_eq!(
            compare(&meant, &meant, 0, None),
            Fidelity {
                dots: 800,
                wrong: 0,
                worst: 0,
                first_wrong: None
            }
        );
        let mut seen = meant.clone();
        seen.set(5, 3, meant.get(5, 3).unwrap() ^ 0x80_0000);
        seen.set(9, 7, meant.get(9, 7).unwrap() ^ 0x00_0080);
        let f = compare(&meant, &seen, 24, None);
        assert_eq!((f.wrong, f.worst, f.first_wrong), (2, 128, Some((5, 3))));
        // Shifted one dot, most of the card is wrong.
        let shifted = compare(&meant, &test_card(40, 20, 1), 24, None);
        assert!(shifted.wrong > 100, "{shifted:?}");
    }

    #[test]
    fn compare_honours_the_tolerance() {
        let meant = Canvas::new(4, 2, 0x808080);
        let mut seen = meant.clone();
        seen.set(0, 0, 0x808080 + 24); // blue off by 24: allowed
        seen.set(1, 0, 0x808080 - (25 << 8)); // green off by 25: wrong
        let f = compare(&meant, &seen, 24, None);
        assert_eq!(
            (f.dots, f.wrong, f.worst, f.first_wrong),
            (8, 1, 25, Some((1, 0)))
        );
        assert_eq!(compare(&meant, &seen, 25, None).wrong, 0);
        assert_eq!(compare(&meant, &seen, 0, None).wrong, 2);
    }

    #[test]
    fn compare_skips_corners_when_asked() {
        let meant = test_card(40, 20, 0);
        let seen = with_marks(meant.clone());
        let all = compare(&meant, &seen, 24, None);
        assert_eq!(all.wrong, 8);
        assert_eq!(all.first_wrong, Some((0, 0)));
        let skipped = compare(&meant, &seen, 24, Some(Ladder::Half));
        assert_eq!((skipped.dots, skipped.wrong), (800 - 8, 0));
        // A wrong dot next to a corner is still counted.
        let mut seen = seen;
        seen.set(1, 0, 0x123456);
        assert_eq!(
            compare(&meant, &seen, 24, Some(Ladder::Half)).first_wrong,
            Some((1, 0))
        );
    }

    #[test]
    fn compare_skips_quarter_rung_corners() {
        // On the quarter rung a corner cell is 2 x 2 dots: all four belong to the mark.
        let (w, h) = (40, 20);
        let meant = test_card(w, h, 0);
        let mut seen = meant.clone();
        for (x, y) in [(0, 0), (w - 2, 0), (0, h - 2), (w - 2, h - 2)] {
            seen.fill_rect(x as i64, y as i64, 2, 2, MARK);
        }
        assert_eq!(compare(&meant, &seen, 24, None).wrong, 16);
        let skipped = compare(&meant, &seen, 24, Some(Ladder::Quarter));
        assert_eq!((skipped.dots, skipped.wrong), (800 - 16, 0));
        // The half rung's corners are only 1 x 2 dots, so half the mark is still wrong.
        assert_eq!(compare(&meant, &seen, 24, Some(Ladder::Half)).wrong, 8);
        // A wrong dot just inside a corner cell's neighbour is still counted.
        seen.set(2, 0, 0x123456);
        let f = compare(&meant, &seen, 24, Some(Ladder::Quarter));
        assert_eq!(f.first_wrong, Some((2, 0)));
    }

    #[test]
    fn canvases_of_different_sizes_are_wholly_wrong() {
        let f = compare(&Canvas::new(4, 4, 0), &Canvas::new(4, 2, 0), 24, None);
        assert_eq!((f.dots, f.wrong), (16, 16));
    }

    /// Needs the Mac's screen. Run through the screen lock:
    /// `rigs/apple-terminal/with-screen.sh cargo test -p phosphor-witness -- --ignored`
    #[test]
    #[ignore]
    fn the_card_is_seen_in_apple_terminal() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let built = Command::new(cargo)
            .args(["build", "-p", "phosphor-witness", "--bin", "witness-card"])
            .status()
            .unwrap();
        assert!(built.success());
        // this test runs from target/debug/deps
        let exe = std::env::current_exe().unwrap();
        let card = exe.parent().unwrap().parent().unwrap().join("witness-card");
        let (cols, rows) = (160, 45);
        let (w, h) = Ladder::Half.canvas_size(cols, rows);
        let meant = test_card(w, h, 0);
        for (font, shift) in [(12.0, 0), (6.0, 0), (12.0, 1)] {
            let window = Window {
                font_size: font,
                cols,
                rows,
                command: format!("{} --shift {shift} 30", sh_quote(&card.to_string_lossy())),
            };
            let photo = shoot(&window, 2500).unwrap();
            let seen = read_dots(&photo, cols, rows, Ladder::Half).unwrap();
            let f = compare(&meant, &seen, 24, Some(Ladder::Half));
            println!(
                "font {font} shift {shift}: {photo_w} x {photo_h} pixels, {f:?}",
                photo_w = photo.w,
                photo_h = photo.h
            );
            if shift == 0 {
                assert_eq!(f.wrong, 0, "font {font}: {f:?}");
            } else {
                assert!(f.wrong > 1000, "shifted: {f:?}");
            }
        }
    }
}
