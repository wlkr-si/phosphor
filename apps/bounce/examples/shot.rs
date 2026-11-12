//! Write a binary PPM of the computer.
//!
//! `cargo run -p phosphor-bounce --example shot -- <canvas_w> <canvas_h> <seconds> <out.ppm> [soft]`
//!
//! The computer is 90% of the canvas size given, as it is when drawn for real. Say `soft` for
//! the soft edges the half rung uses; otherwise edges are hard.

use phosphor_bounce::Bounce;
use phosphor_kernel::Canvas;
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let soft = a.len() == 5 && a[4] == "soft";
    if a.len() != 4 && !soft {
        eprintln!("usage: shot <canvas_w> <canvas_h> <seconds> <out.ppm> [soft]");
        std::process::exit(2);
    }
    let w: usize = a[0].parse().expect("canvas_w is a whole number");
    let h: usize = a[1].parse().expect("canvas_h is a whole number");
    let seconds: f64 = a[2].parse().expect("seconds is a number");
    let bounce = Bounce::new(w, h, soft);
    let (cw, ch) = bounce.size();
    let mut canvas = Canvas::new(cw, ch, 0);
    bounce.draw(seconds, &mut canvas);
    let mut out = format!("P6\n{} {}\n255\n", cw, ch).into_bytes();
    for d in &canvas.dots {
        out.extend_from_slice(&[(d >> 16) as u8, (d >> 8) as u8, *d as u8]);
    }
    std::fs::File::create(&a[3])
        .and_then(|mut f| f.write_all(&out))
        .expect("write the picture");
}
