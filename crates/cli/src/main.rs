//! The one program.
//!
//! `phosphor play bounce [--rung half|quarter|sixth|eighth] [--speed dots-per-second]
//! [--seconds n] [--fps n] [--profile probe|apple|cautious] [--strip] [--marks]
//! [--freeze-at seconds] [--report file.json] [--no-govern] [--moving-share 0..1]
//! [--full-colour-layer file.png]`
//!
//! `phosphor play develop picture.png [--thing] [--speed dots-per-second] [--seconds n]
//! [--fps n] [--profile probe|apple|cautious] [--strip] [--marks] [--freeze-at seconds]
//! [--report file.json]`
mod develop;
mod frame;
mod play;

use phosphor_kernel::Ladder;
use phosphor_surface::tty::Tty;
use play::{play, Choice, Options, Report, What};

const USAGE: &str = "usage: phosphor play bounce [--rung half|quarter|sixth|eighth] \
[--speed dots-per-second] [--seconds n] [--fps n] [--profile probe|apple|cautious] \
[--strip] [--marks] [--freeze-at seconds] [--report file.json] [--no-govern] \
[--moving-share 0..1] [--full-colour-layer file.png]
       phosphor play develop picture.png [--thing] [--speed dots-per-second] [--seconds n] \
[--fps n] [--profile probe|apple|cautious] [--strip] [--marks] [--freeze-at seconds] \
[--report file.json]

bounce shows the bouncing computer. The governor keeps every frame inside the budget;
--no-govern sends every frame whole.
develop shows a picture that arrives coarse and sharpens, on the half rung; --thing adds a
small white square gliding across it.
Frames go out at the surface's paint rate, how often it is seen to paint; --fps sets
another beat.
Either plays until the time is up, or q or Ctrl-C is pressed.";

/// Develop's square moves a dot a beat at 60 frames a second.
const DEVELOP_SPEED: f64 = 60.0;

/// Read the command line, after the program's own name.
fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let flags = match (
        args.first().map(String::as_str),
        args.get(1).map(String::as_str),
    ) {
        (Some("play"), Some("bounce")) => &args[2..],
        (Some("play"), Some("develop")) => {
            o.what = What::Develop;
            o.look.speed = DEVELOP_SPEED;
            match args.get(2) {
                Some(p) if !p.starts_with('-') => o.picture = Some(p.clone()),
                _ => return Err("develop needs a picture: a PNG file".into()),
            }
            &args[3..]
        }
        (Some("play"), Some(other)) => {
            return Err(format!("there is nothing called {other} to play"))
        }
        _ => return Err("expected `play bounce` or `play develop`".into()),
    };
    let develop = o.what == What::Develop;
    let mut it = flags.iter();
    while let Some(flag) = it.next() {
        let only_bounce = matches!(
            flag.as_str(),
            "--rung" | "--no-govern" | "--moving-share" | "--full-colour-layer"
        );
        if develop && only_bounce {
            return Err(format!("{flag} is for bounce, not develop"));
        }
        if !develop && flag == "--thing" {
            return Err("--thing is for develop, not bounce".into());
        }
        let mut value = || {
            it.next()
                .map(String::as_str)
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        let number = |v: &str, min: f64| -> Result<f64, String> {
            match v.parse::<f64>() {
                Ok(n) if n.is_finite() && n >= min => Ok(n),
                _ => Err(format!("{flag} needs a number of at least {min}, not {v}")),
            }
        };
        match flag.as_str() {
            "--rung" => {
                o.rung = Some(match value()? {
                    "half" => Ladder::Half,
                    "quarter" => Ladder::Quarter,
                    "sixth" => Ladder::Sixth,
                    "eighth" => Ladder::Eighth,
                    v => return Err(format!("no rung called {v}")),
                })
            }
            "--speed" => o.look.speed = number(value()?, 0.0)?,
            "--seconds" => o.seconds = Some(number(value()?, 0.0)?),
            "--fps" => {
                let fps = number(value()?, 1.0)?;
                if fps > 1000.0 {
                    return Err("--fps is at most 1000".into());
                }
                o.fps = Some(fps);
            }
            "--profile" => {
                o.profile = match value()? {
                    "probe" => Choice::Probe,
                    "apple" => Choice::Apple,
                    "cautious" => Choice::Cautious,
                    v => return Err(format!("no profile called {v}")),
                }
            }
            "--strip" => o.look.strip = true,
            "--marks" => o.look.marks = true,
            "--freeze-at" => o.look.freeze_ns = Some((number(value()?, 0.0)? * 1e9).round() as u64),
            "--report" => o.report = Some(value()?.to_string()),
            "--no-govern" => o.govern = false,
            "--moving-share" => {
                let share = number(value()?, 0.0)?;
                if share > 1.0 {
                    return Err("--moving-share is at most 1".into());
                }
                o.moving_share = share;
            }
            "--full-colour-layer" => o.picture = Some(value()?.to_string()),
            "--thing" => o.thing = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("{other} is not understood")),
        }
    }
    Ok(o)
}

/// Play, then write the report where asked. Returns the report and its JSON.
fn play_and_report(
    tty: Tty,
    options: &Options,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<(Report, String), String> {
    let report = play(tty, options, env).map_err(|e| e.to_string())?;
    let json = report.to_json();
    if let Some(path) = &options.report {
        std::fs::write(path, format!("{json}\n")).map_err(|e| format!("writing {path}: {e}"))?;
    }
    Ok((report, json))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(o) => o,
        Err(why) => {
            if !why.is_empty() {
                eprintln!("phosphor: {why}");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let tty = match Tty::open() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("phosphor: needs a terminal: {e}");
            std::process::exit(1);
        }
    };
    let env = |k: &str| std::env::var(k).ok();
    match play_and_report(tty, &options, &env) {
        Ok((_, json)) => println!("{json}"),
        Err(e) => {
            eprintln!("phosphor: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod pty;

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn every_option_is_read() {
        let o = parse(&args(
            "play bounce --rung half --speed 30 --seconds 5 --fps 30 --profile apple \
             --strip --marks --freeze-at 2.5 --report r.json --no-govern --moving-share 0.25 \
             --full-colour-layer p.png",
        ))
        .unwrap();
        assert_eq!(o.rung, Some(Ladder::Half));
        assert_eq!(o.look.speed, 30.0);
        assert_eq!(o.seconds, Some(5.0));
        assert_eq!(o.fps, Some(30.0));
        assert_eq!(o.profile, Choice::Apple);
        assert!(o.look.strip && o.look.marks);
        assert_eq!(o.look.freeze_ns, Some(2_500_000_000));
        assert_eq!(o.report.as_deref(), Some("r.json"));
        assert!(!o.govern);
        assert_eq!(o.moving_share, 0.25);
        assert_eq!(o.picture.as_deref(), Some("p.png"));
    }

    #[test]
    fn defaults_probe_at_the_paint_rate_until_quit() {
        let o = parse(&args("play bounce")).unwrap();
        assert_eq!(o, Options::default());
        assert_eq!(o.profile, Choice::Probe);
        assert_eq!(o.fps, None);
        assert_eq!(o.seconds, None);
        assert!(o.govern);
        assert_eq!(o.moving_share, 1.0);
        assert_eq!(o.picture, None);
    }

    #[test]
    fn develop_takes_a_picture_and_a_thing() {
        let o = parse(&args(
            "play develop p.png --thing --speed 30 --seconds 5 --fps 30 --profile apple \
             --strip --marks --freeze-at 2.5 --report r.json",
        ))
        .unwrap();
        assert_eq!(o.what, What::Develop);
        assert!(o.thing);
        assert_eq!(o.picture.as_deref(), Some("p.png"));
        assert_eq!(o.look.speed, 30.0);
        assert_eq!(o.seconds, Some(5.0));
        assert_eq!(o.fps, Some(30.0));
        assert_eq!(o.profile, Choice::Apple);
        assert!(o.look.strip && o.look.marks);
        assert_eq!(o.look.freeze_ns, Some(2_500_000_000));
        assert_eq!(o.report.as_deref(), Some("r.json"));
        let plain = parse(&args("play develop p.png")).unwrap();
        assert!(!plain.thing);
        assert_eq!(plain.look.speed, DEVELOP_SPEED);
        assert_eq!(parse(&args("play bounce")).unwrap().what, What::Bounce);
    }

    #[test]
    fn mistakes_are_refused() {
        for bad in [
            "",
            "play",
            "play pong",
            "bounce",
            "play bounce --rung ninth",
            "play bounce --fps 0",
            "play bounce --fps nan",
            "play bounce --seconds -1",
            "play bounce --profile ghostty",
            "play bounce --report",
            "play bounce --moving-share 1.5",
            "play bounce --moving-share -0.1",
            "play bounce --full-colour-layer",
            "play bounce --loud",
            "play bounce --thing",
            "play develop",
            "play develop --thing",
            "play develop p.png --rung half",
            "play develop p.png --no-govern",
            "play develop p.png --moving-share 0.5",
            "play develop p.png --full-colour-layer q.png",
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad}");
        }
    }
}
