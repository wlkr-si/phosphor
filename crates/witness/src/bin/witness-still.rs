//! Photographs a program in Apple's Terminal, reads back the dots and measures fidelity
//! against the test card.
//!
//! usage: witness-still --font 12 --cols 160 --rows 45 --after 1500 --ladder half
//!        [--tolerance 24] --out photo.png -- <command>
//!
//! The picture meant is the test card, unshifted, with the four corner cells left out for
//! the registration marks. Run it through `rigs/apple-terminal/with-screen.sh`.

use phosphor_kernel::{test_card, Ladder};
use phosphor_witness::still::{compare, read_dots, save_photo, shoot, Window};

fn usage() -> ! {
    eprintln!(
        "usage: witness-still --font 12 --cols 160 --rows 45 --after 1500 --ladder half \
         [--tolerance 24] --out photo.png -- <command>"
    );
    std::process::exit(2)
}

fn ladder_name(l: Ladder) -> &'static str {
    match l {
        Ladder::Half => "half",
        Ladder::Quarter => "quarter",
        Ladder::Sixth => "sixth",
        Ladder::Eighth => "eighth",
    }
}

fn json_string(s: &str) -> String {
    let mut o = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let split = args
        .iter()
        .position(|a| a == "--")
        .unwrap_or_else(|| usage());
    let (opts, command) = (&args[..split], &args[split + 1..]);
    if command.is_empty() {
        usage();
    }
    let opt = |k: &str| {
        opts.iter()
            .position(|a| a == k)
            .and_then(|i| opts.get(i + 1))
            .map(String::as_str)
    };
    let num = |k: &str, d: &str| -> f64 {
        opt(k).unwrap_or(d).parse().unwrap_or_else(|_| {
            eprintln!("{k} needs a number");
            usage()
        })
    };
    let font = num("--font", "12") as f32;
    let cols = num("--cols", "160") as usize;
    let rows = num("--rows", "45") as usize;
    let after = num("--after", "1500") as u64;
    let tolerance = num("--tolerance", "24") as u32;
    let ladder = match opt("--ladder").unwrap_or("half") {
        "half" => Ladder::Half,
        "quarter" => Ladder::Quarter,
        "sixth" => Ladder::Sixth,
        "eighth" => Ladder::Eighth,
        _ => usage(),
    };
    let out = opt("--out").unwrap_or("photo.png");
    let quoted: Vec<String> = command
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', r"'\''")))
        .collect();
    let window = Window {
        font_size: font,
        cols,
        rows,
        command: quoted.join(" "),
    };

    let fail = |e: String| -> ! {
        eprintln!("witness-still: {e}");
        std::process::exit(1)
    };
    let photo = shoot(&window, after).unwrap_or_else(|e| fail(e));
    let png = save_photo(&photo).unwrap_or_else(|e| fail(e));
    std::fs::write(out, png).unwrap_or_else(|e| fail(format!("writing {out}: {e}")));
    let seen = read_dots(&photo, cols, rows, ladder).unwrap_or_else(|e| fail(e));
    let (w, h) = ladder.canvas_size(cols, rows);
    let f = compare(&test_card(w, h, 0), &seen, tolerance, Some(ladder));
    let first = match f.first_wrong {
        Some((x, y)) => format!("[{x}, {y}]"),
        None => "null".into(),
    };
    println!(
        "{{\"font\": {font}, \"cols\": {cols}, \"rows\": {rows}, \"ladder\": \"{}\", \
         \"after_ms\": {after}, \"tolerance\": {tolerance}, \"photo\": {}, \
         \"photo_w\": {}, \"photo_h\": {}, \"dots\": {}, \"wrong\": {}, \"worst\": {}, \
         \"first_wrong\": {first}}}",
        ladder_name(ladder),
        json_string(out),
        photo.w,
        photo.h,
        f.dots,
        f.wrong,
        f.worst,
    );
}
