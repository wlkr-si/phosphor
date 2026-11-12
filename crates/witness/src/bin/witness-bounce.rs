//! Photographs `phosphor play bounce`, held still, in Apple's Terminal and measures its
//! fidelity against the picture meant.
//!
//! usage: witness-bounce --rung half|quarter --font 12 --freeze 0.5 [--cols 160] [--rows 45]
//!        [--tolerance 24] [--phosphor path] --out photo.png
//!        witness-bounce --all <folder>
//!
//! The picture meant is worked out here, apart from the program: the benchmark composed at
//! the freeze time, fitted to the rung, then `unfit` back to dots. The program runs with the
//! Apple profile, the registration marks and `--freeze-at`, and the corner cells are left
//! out of the comparison. `--all` runs the twelve combinations of issue #19 (rungs half and
//! quarter, fonts 12 and 6, freeze times 0.5, 3.0 and 7.3 seconds), saves each photograph in
//! the folder and prints a table. Run it through `rigs/apple-terminal/with-screen.sh`.

use phosphor_bounce::{Bounce, ROOM};
use phosphor_compose::{Layer, Motion, Placement, Scene};
use phosphor_fit::exact::{fit, unfit};
use phosphor_kernel::{Canvas, Grid, Ladder};
use phosphor_witness::still::{compare, read_dots, save_photo, shoot, Fidelity, Window};
use std::path::{Path, PathBuf};

/// The program's own default speed, in dots a second.
const SPEED: f64 = 10.0;
/// The reference grid.
const COLS: usize = 160;
const ROWS: usize = 45;
/// How long after the freeze to wait before photographing.
const SETTLE_MS: u64 = 2000;

/// One photograph to take.
#[derive(Clone, Debug, PartialEq)]
struct Shot {
    ladder: Ladder,
    font: f32,
    freeze: f64,
    cols: usize,
    rows: usize,
    tolerance: u32,
}

/// The freeze time as the program reads it: whole nanoseconds.
fn freeze_ns(seconds: f64) -> u64 {
    (seconds * 1e9).round() as u64
}

/// The picture meant: the benchmark at `t_ns` on a `cols` × `rows` grid, composed as the
/// program composes it, fitted to `ladder` and turned back into dots.
fn picture_meant(cols: usize, rows: usize, ladder: Ladder, t_ns: u64) -> Canvas {
    let (w, h) = ladder.canvas_size(cols, rows);
    // As the program does: soft text edges on the half rung, hard ones on finer rungs.
    let bounce = Bounce::new(w, h, matches!(ladder, Ladder::Half));
    let (bw, bh) = bounce.size();
    let mut computer = Canvas::new(bw, bh, ROOM);
    bounce.draw(t_ns as f64 / 1e9, &mut computer);
    let scene = Scene {
        w,
        h,
        background: ROOM,
        layers: vec![Layer {
            canvas: computer,
            placement: Placement { x: 0, y: 0 },
            motion: Motion::Bounce {
                dots_per_second: SPEED,
            },
            clear: None,
            visible: true,
        }],
    };
    let mut canvas = Canvas::new(w, h, ROOM);
    scene.compose(t_ns, &mut canvas);
    let mut grid = Grid::new(cols, rows);
    fit(&canvas, ladder, None, &mut grid);
    unfit(&grid, ladder)
}

fn rung_name(l: Ladder) -> &'static str {
    match l {
        Ladder::Half => "half",
        Ladder::Quarter => "quarter",
        Ladder::Sixth => "sixth",
        Ladder::Eighth => "eighth",
    }
}

/// Quote a string for /bin/sh.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The command line that runs the program held still at the shot's freeze time. It runs
/// long enough to be photographed and is ended once it has been. The governor is off, so
/// the picture shown is compose → fit alone, the picture meant worked out here.
fn command(phosphor: &Path, shot: &Shot) -> String {
    format!(
        "{} play bounce --profile apple --no-govern --rung {} --marks --freeze-at {} \
         --seconds {}",
        sh_quote(&phosphor.to_string_lossy()),
        rung_name(shot.ladder),
        shot.freeze,
        shot.freeze + 60.0,
    )
}

/// What one photograph showed.
struct Taken {
    png: Vec<u8>,
    /// Against the picture meant at the freeze time.
    fidelity: Fidelity,
    /// Wrong dots against the picture meant one second later: a control, to show the
    /// comparison can tell one picture of the benchmark from another.
    control: usize,
}

/// Photograph the program for one shot and measure it.
fn take(phosphor: &Path, shot: &Shot) -> Result<Taken, String> {
    let window = Window {
        font_size: shot.font,
        cols: shot.cols,
        rows: shot.rows,
        command: command(phosphor, shot),
    };
    let after = (shot.freeze * 1000.0).ceil() as u64 + SETTLE_MS;
    let photo = shoot(&window, after)?;
    let png = save_photo(&photo)?;
    let seen = read_dots(&photo, shot.cols, shot.rows, shot.ladder)?;
    let meant = picture_meant(shot.cols, shot.rows, shot.ladder, freeze_ns(shot.freeze));
    let fidelity = compare(&meant, &seen, shot.tolerance, Some(shot.ladder));
    let later = picture_meant(
        shot.cols,
        shot.rows,
        shot.ladder,
        freeze_ns(shot.freeze + 1.0),
    );
    let control = compare(&later, &seen, shot.tolerance, Some(shot.ladder)).wrong;
    Ok(Taken {
        png,
        fidelity,
        control,
    })
}

/// The twelve combinations of issue #19.
fn all_shots() -> Vec<Shot> {
    let mut v = Vec::new();
    for ladder in [Ladder::Half, Ladder::Quarter] {
        for font in [12.0, 6.0] {
            for freeze in [0.5, 3.0, 7.3] {
                v.push(Shot {
                    ladder,
                    font,
                    freeze,
                    cols: COLS,
                    rows: ROWS,
                    tolerance: 24,
                });
            }
        }
    }
    v
}

/// A photograph's file name.
fn photo_name(shot: &Shot) -> String {
    format!(
        "{}-font{}-t{:.1}.png",
        rung_name(shot.ladder),
        shot.font,
        shot.freeze
    )
}

/// One line of the results table.
fn table_row(shot: &Shot, name: &str, f: &Fidelity, control: usize) -> String {
    let first = match f.first_wrong {
        Some((x, y)) => format!("({x}, {y})"),
        None => "–".into(),
    };
    format!(
        "| {} | {} | {:.1} | {} | {} | {} | {} | {control} | [{name}](../evidence/bounce/{name}) |",
        rung_name(shot.ladder),
        shot.font,
        shot.freeze,
        f.dots,
        f.wrong,
        f.worst,
        first
    )
}

fn json(shot: &Shot, out: &str, f: &Fidelity, control: usize) -> String {
    let first = match f.first_wrong {
        Some((x, y)) => format!("[{x}, {y}]"),
        None => "null".into(),
    };
    format!(
        "{{\"rung\": \"{}\", \"font\": {}, \"freeze\": {}, \"cols\": {}, \"rows\": {}, \
         \"tolerance\": {}, \"photo\": \"{}\", \"dots\": {}, \"wrong\": {}, \"worst\": {}, \
         \"first_wrong\": {first}, \"control_wrong\": {control}}}",
        rung_name(shot.ladder),
        shot.font,
        shot.freeze,
        shot.cols,
        shot.rows,
        shot.tolerance,
        out.replace('\\', "\\\\").replace('"', "\\\""),
        f.dots,
        f.wrong,
        f.worst,
    )
}

/// The program, beside this one unless named.
fn default_phosphor() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("phosphor")))
        .unwrap_or_else(|| PathBuf::from("phosphor"))
}

fn usage() -> ! {
    eprintln!(
        "usage: witness-bounce --rung half|quarter --font 12 --freeze 0.5 [--cols 160] \
         [--rows 45] [--tolerance 24] [--phosphor path] --out photo.png\n       \
         witness-bounce --all <folder> [--phosphor path]"
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    let num = |k: &str, d: &str| -> f64 {
        opt(k).unwrap_or(d).parse().unwrap_or_else(|_| {
            eprintln!("{k} needs a number");
            usage()
        })
    };
    let fail = |e: String| -> ! {
        eprintln!("witness-bounce: {e}");
        std::process::exit(1)
    };
    let phosphor = opt("--phosphor").map_or_else(default_phosphor, PathBuf::from);
    if !phosphor.exists() {
        fail(format!(
            "no program at {}; build it with `cargo build -p phosphor`",
            phosphor.display()
        ));
    }

    if let Some(folder) = opt("--all") {
        let folder = Path::new(folder);
        std::fs::create_dir_all(folder)
            .unwrap_or_else(|e| fail(format!("making {}: {e}", folder.display())));
        println!(
            "| Rung | Font | Freeze (s) | Dots | Wrong | Worst | First wrong | Control | \
             Photograph |"
        );
        println!("|---|---|---|---|---|---|---|---|---|");
        let mut wrong = 0;
        for shot in all_shots() {
            let name = photo_name(&shot);
            let t = take(&phosphor, &shot).unwrap_or_else(|e| fail(e));
            let path = folder.join(&name);
            std::fs::write(&path, &t.png)
                .unwrap_or_else(|e| fail(format!("writing {}: {e}", path.display())));
            println!("{}", table_row(&shot, &name, &t.fidelity, t.control));
            wrong += t.fidelity.wrong;
        }
        std::process::exit(if wrong == 0 { 0 } else { 1 });
    }

    let ladder = match opt("--rung").unwrap_or("half") {
        "half" => Ladder::Half,
        "quarter" => Ladder::Quarter,
        _ => usage(),
    };
    let shot = Shot {
        ladder,
        font: num("--font", "12") as f32,
        freeze: num("--freeze", "0.5"),
        cols: num("--cols", "160") as usize,
        rows: num("--rows", "45") as usize,
        tolerance: num("--tolerance", "24") as u32,
    };
    let out = opt("--out").unwrap_or("photo.png");
    let t = take(&phosphor, &shot).unwrap_or_else(|e| fail(e));
    std::fs::write(out, &t.png).unwrap_or_else(|e| fail(format!("writing {out}: {e}")));
    println!("{}", json(&shot, out, &t.fidelity, t.control));
}

#[cfg(test)]
mod tests {
    use super::*;
    use phosphor_witness::still::MARK;

    #[test]
    fn twelve_combinations_with_distinct_photographs() {
        let shots = all_shots();
        assert_eq!(shots.len(), 12);
        let mut names: Vec<String> = shots.iter().map(photo_name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 12);
        assert_eq!(photo_name(&shots[0]), "half-font12-t0.5.png");
        assert_eq!(photo_name(&shots[11]), "quarter-font6-t7.3.png");
    }

    #[test]
    fn the_freeze_time_is_read_as_the_program_reads_it() {
        assert_eq!(freeze_ns(0.5), 500_000_000);
        assert_eq!(freeze_ns(3.0), 3_000_000_000);
        // 7.3 × 1e9 is 7299999999.999999 in floating point.
        assert_eq!(freeze_ns(7.3), 7_300_000_000);
    }

    #[test]
    fn the_command_holds_the_picture_with_marks_on_the_apple_profile() {
        let shot = all_shots()[4].clone();
        let c = command(Path::new("/x/it's/phosphor"), &shot);
        assert_eq!(
            c,
            "'/x/it'\\''s/phosphor' play bounce --profile apple --no-govern --rung half \
             --marks --freeze-at 3 --seconds 63"
        );
    }

    #[test]
    fn the_picture_meant_moves_with_time_and_fits_the_grid() {
        for ladder in [Ladder::Half, Ladder::Quarter] {
            let a = picture_meant(COLS, ROWS, ladder, freeze_ns(0.5));
            let b = picture_meant(COLS, ROWS, ladder, freeze_ns(7.3));
            assert_eq!((a.w, a.h), ladder.canvas_size(COLS, ROWS));
            assert_ne!(a, b, "{ladder:?}: the computer should have moved");
            // The room shows around the computer; the marks are never meant.
            assert_eq!(a.dots[0], ROOM);
            assert!(!a.dots.contains(&MARK));
        }
    }

    /// Needs the Mac's screen. Run through the screen lock:
    /// `rigs/apple-terminal/with-screen.sh cargo test -p phosphor-witness --bin witness-bounce
    /// -- --ignored`
    #[test]
    #[ignore]
    fn the_benchmark_is_seen_exactly_in_apple_terminal() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let built = std::process::Command::new(cargo)
            .args(["build", "-p", "phosphor"])
            .status()
            .unwrap();
        assert!(built.success());
        // this test runs from target/debug/deps
        let exe = std::env::current_exe().unwrap();
        let phosphor = exe.parent().unwrap().parent().unwrap().join("phosphor");
        let mut failed = Vec::new();
        for shot in all_shots() {
            let t = take(&phosphor, &shot).unwrap();
            println!(
                "{}",
                table_row(&shot, &photo_name(&shot), &t.fidelity, t.control)
            );
            if t.fidelity.wrong != 0 || t.control < 100 {
                failed.push(photo_name(&shot));
            }
        }
        assert!(
            failed.is_empty(),
            "wrong dots, or a weak control, in {failed:?}"
        );
    }
}
