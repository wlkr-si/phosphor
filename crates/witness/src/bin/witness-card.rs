//! Paints the test card on the half rung, then the four registration marks, then waits.
//! It uses only cursor-to-position, full colour and `▀` (and hides the cursor, so it does
//! not sit on a mark).
//!
//! usage: witness-card [--shift N] <seconds>

use phosphor_kernel::{test_card, Ladder, Rgb};
use std::fs::OpenOptions;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

const MARK: Rgb = 0xFF00FF;

/// The grid size, as (cols, rows), from `stty size` on the controlling terminal.
fn grid_size() -> Result<(usize, usize), String> {
    let tty = OpenOptions::new()
        .read(true)
        .open("/dev/tty")
        .map_err(|e| format!("needs a terminal: {e}"))?;
    let out = Command::new("stty")
        .arg("size")
        .stdin(Stdio::from(tty))
        .output()
        .map_err(|e| format!("stty: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut it = text.split_whitespace().map(|n| n.parse::<usize>());
    match (it.next(), it.next()) {
        (Some(Ok(rows)), Some(Ok(cols))) if rows > 0 && cols > 0 => Ok((cols, rows)),
        _ => Err(format!("stty size said {text:?}")),
    }
}

fn cup(b: &mut Vec<u8>, col: usize, row: usize) {
    b.extend_from_slice(format!("\x1b[{};{}H", row + 1, col + 1).as_bytes());
}

fn colour(b: &mut Vec<u8>, base: u8, c: Rgb) {
    b.extend_from_slice(
        format!(
            "\x1b[{base}8;2;{};{};{}m",
            c >> 16 & 255,
            c >> 8 & 255,
            c & 255
        )
        .as_bytes(),
    );
}

/// The bytes that paint the card and the marks on a `cols` by `rows` grid.
fn paint(cols: usize, rows: usize, shift: usize) -> Vec<u8> {
    let (w, h) = Ladder::Half.canvas_size(cols, rows);
    let card = test_card(w, h, shift);
    let mut b = b"\x1b[?25l".to_vec();
    let (mut ink, mut paper) = (None, None);
    for row in 0..rows {
        cup(&mut b, 0, row);
        for col in 0..cols {
            let (top, bottom) = (
                card.dots[2 * row * w + col],
                card.dots[(2 * row + 1) * w + col],
            );
            if ink != Some(top) {
                colour(&mut b, 3, top);
                ink = Some(top);
            }
            if paper != Some(bottom) {
                colour(&mut b, 4, bottom);
                paper = Some(bottom);
            }
            b.extend_from_slice("▀".as_bytes());
        }
    }
    colour(&mut b, 4, MARK);
    for (col, row) in [(0, 0), (cols - 1, 0), (0, rows - 1), (cols - 1, rows - 1)] {
        cup(&mut b, col, row);
        b.push(b' ');
    }
    b
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut shift = 0;
    let mut seconds = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--shift" {
            i += 1;
            shift = args.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(|| {
                eprintln!("--shift needs a number");
                std::process::exit(2)
            });
        } else {
            seconds = args[i].parse::<f64>().ok();
        }
        i += 1;
    }
    let Some(seconds) = seconds else {
        eprintln!("usage: witness-card [--shift N] <seconds>");
        std::process::exit(2);
    };
    let (cols, rows) = grid_size().unwrap_or_else(|e| {
        eprintln!("witness-card: {e}");
        std::process::exit(1)
    });
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(&paint(cols, rows, shift));
    let _ = out.flush();
    std::thread::sleep(Duration::from_secs_f64(seconds.max(0.0)));
    let _ = out.write_all(b"\x1b[0m\x1b[?25h");
    let _ = out.flush();
}
