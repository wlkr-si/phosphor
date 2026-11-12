//! Films a program running in Apple's Terminal or Ghostty and prints its cadence as JSON.
//!
//! usage: witness-film [--terminal apple|ghostty] --font 12 --cols 160 --rows 45 --seconds 10
//! [--fps 120] -- <command>
//!
//! Run it through `rigs/apple-terminal/with-screen.sh`. A command without a `/` is looked
//! for beside this program first, so `-- witness-ticker --seconds 12` works.

use phosphor_witness::cadence::{film, FilmOptions, Terminal};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let split = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let (flags, command) = (&args[..split], args.get(split + 1..).unwrap_or(&[]));
    let opt = |k: &str, d: f64| -> f64 {
        match flags.iter().position(|a| a == k) {
            None => d,
            Some(i) => flags
                .get(i + 1)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| fail(&format!("{k} needs a number"))),
        }
    };
    if command.is_empty() {
        fail("usage: witness-film [--terminal apple|ghostty] --font 12 --cols 160 --rows 45 --seconds 10 [--fps 120] -- <command>");
    }
    let terminal = match flags.iter().position(|a| a == "--terminal") {
        None => Terminal::Apple,
        Some(i) => flags
            .get(i + 1)
            .and_then(|v| Terminal::parse(v))
            .unwrap_or_else(|| fail("--terminal is apple or ghostty")),
    };
    let mut command = command.to_vec();
    if !command[0].contains('/') {
        if let Some(beside) = std::env::current_exe()
            .ok()
            .and_then(|e| Some(e.parent()?.join(&command[0])))
            .filter(|p| p.exists())
        {
            command[0] = beside.to_string_lossy().into();
        }
    }
    let opts = FilmOptions {
        terminal,
        font: opt("--font", 12.0),
        cols: opt("--cols", 160.0) as usize,
        rows: opt("--rows", 45.0) as usize,
        seconds: opt("--seconds", 10.0),
        fps: opt("--fps", 120.0),
        command,
    };
    match film(&opts) {
        Ok(f) => println!("{}", f.to_json()),
        Err(e) => fail(&e),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("witness-film: {msg}");
    std::process::exit(1)
}
