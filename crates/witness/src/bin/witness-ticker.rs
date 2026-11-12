//! A program to film: 60 times a second it paints the frame counter strip and a bar that
//! moves one cell a frame, with plain writes to the terminal.
//!
//! usage: witness-ticker [--seconds 10] [--cols 80]

use std::io::Write;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opt = |k: &str, d: f64| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(d)
    };
    let seconds = opt("--seconds", 10.0);
    let cols = (opt("--cols", 80.0) as usize).max(19);
    let frames = (seconds * 60.0) as u64;
    let out = std::io::stdout();
    let mut out = out.lock();
    let mut b = Vec::with_capacity(4096);
    // alternate screen, no cursor, black paper, clear
    b.extend_from_slice(b"\x1b[?1049h\x1b[?25l\x1b[48;2;0;0;0m\x1b[2J");
    let white: &[u8] = b"\x1b[48;2;255;255;255m";
    let black: &[u8] = b"\x1b[48;2;0;0;0m";
    let start = Instant::now();
    for n in 0..frames {
        let due = start + Duration::from_secs_f64(n as f64 / 60.0);
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        }
        // the strip: 16 bits of the frame number, then a white cell and a black one
        b.extend_from_slice(b"\x1b[1;1H");
        let number = n as u16;
        let mut paper: Option<bool> = None;
        for i in 0..18 {
            let on = match i {
                0..=15 => number >> (15 - i) & 1 == 1,
                16 => true,
                _ => false,
            };
            if paper != Some(on) {
                b.extend_from_slice(if on { white } else { black });
                paper = Some(on);
            }
            b.push(b' ');
        }
        // the bar: four cells tall on rows 3 to 6, one cell further right each frame
        let (old, new) = ((n as usize + cols - 1) % cols, n as usize % cols);
        for (col, colour) in [(old, black), (new, white)] {
            if n == 0 && col == old {
                continue;
            }
            b.extend_from_slice(colour);
            for row in 3..=6 {
                b.extend_from_slice(format!("\x1b[{row};{}H ", col + 1).as_bytes());
            }
        }
        b.extend_from_slice(black);
        if out.write_all(&b).and_then(|_| out.flush()).is_err() {
            return;
        }
        b.clear();
    }
    let _ = out.write_all(b"\x1b[0m\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
}
