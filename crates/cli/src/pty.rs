//! Tests that run the program inside a pseudo-terminal, as a viewer's terminal would.
//!
//! The crate takes no dependencies beyond the parts, so the few C calls needed to make a
//! pseudo-terminal are declared here.

use crate::play::Stop;
use crate::{parse, play_and_report};
use phosphor_surface::tty::Tty;
use std::os::raw::{c_char, c_int, c_ulong, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[repr(C)]
struct Winsize {
    rows: u16,
    cols: u16,
    xpixel: u16,
    ypixel: u16,
}

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

#[cfg(target_os = "linux")]
type Nfds = c_ulong;
#[cfg(not(target_os = "linux"))]
type Nfds = std::os::raw::c_uint;

#[cfg(target_os = "linux")]
const TIOCSWINSZ: c_ulong = 0x5414;
#[cfg(not(target_os = "linux"))]
const TIOCSWINSZ: c_ulong = 0x8008_7467;

const POLLIN: i16 = 1;
const SIGWINCH: c_int = 28;

#[cfg_attr(target_os = "linux", link(name = "util"))]
extern "C" {
    fn openpty(
        master: *mut c_int,
        slave: *mut c_int,
        name: *mut c_char,
        termp: *const c_void,
        winp: *const Winsize,
    ) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, n: usize) -> isize;
    fn write(fd: c_int, buf: *const c_void, n: usize) -> isize;
    fn poll(fds: *mut PollFd, n: Nfds, timeout_ms: c_int) -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    fn raise(sig: c_int) -> c_int;
}

/// Terminals, signal handlers and the window-change flag are process-wide, so these tests
/// take turns.
static TURN: Mutex<()> = Mutex::new(());

struct Pty {
    master: c_int,
    slave: c_int,
}

impl Pty {
    fn new(cols: u16, rows: u16) -> Pty {
        let (mut m, mut s) = (0, 0);
        let ws = Winsize {
            rows,
            cols,
            xpixel: 0,
            ypixel: 0,
        };
        let r = unsafe { openpty(&mut m, &mut s, std::ptr::null_mut(), std::ptr::null(), &ws) };
        assert_eq!(r, 0, "openpty failed");
        Pty {
            master: m,
            slave: s,
        }
    }
}

/// Change the window size, as dragging the window's corner would.
fn set_size(master: c_int, cols: u16, rows: u16) {
    let ws = Winsize {
        rows,
        cols,
        xpixel: 0,
        ypixel: 0,
    };
    assert_eq!(
        unsafe { ioctl(master, TIOCSWINSZ, &ws as *const Winsize) },
        0
    );
}

impl Drop for Pty {
    fn drop(&mut self) {
        unsafe {
            close(self.master);
            close(self.slave);
        }
    }
}

fn type_keys(fd: c_int, bytes: &[u8]) {
    let n = unsafe { write(fd, bytes.as_ptr().cast(), bytes.len()) };
    assert_eq!(n, bytes.len() as isize);
}

/// The viewer's side: reads everything the program writes and answers each `ESC [ 6 n` the
/// way a terminal does, until told to stop. Returns all it read.
fn viewer(master: c_int, answer: bool, stop: Arc<AtomicBool>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut seen = Vec::new();
        let mut buf = [0u8; 65536];
        loop {
            let mut p = PollFd {
                fd: master,
                events: POLLIN,
                revents: 0,
            };
            let ready = unsafe { poll(&mut p, 1, 50) };
            if ready <= 0 {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                continue;
            }
            let n = unsafe { read(master, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            // Look for questions only in the new bytes (and the three before them, for one
            // split across reads), so the viewer keeps up however much has been seen.
            let from = seen.len().saturating_sub(3);
            seen.extend_from_slice(&buf[..n as usize]);
            if answer {
                let asked = seen[from..].windows(4).filter(|w| *w == b"\x1b[6n").count();
                for _ in 0..asked {
                    type_keys(master, b"\x1b[12;34R");
                }
            }
        }
        seen
    })
}

fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

fn ends_restored(seen: &[u8]) -> bool {
    let leave = b"\x1b[0m\x1b[?25h\x1b[?1049l";
    seen.ends_with(leave)
}

fn no_env(_: &str) -> Option<String> {
    None
}

#[test]
fn a_three_second_run_writes_its_report() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let pty = Pty::new(100, 30);
    let stop = Arc::new(AtomicBool::new(false));
    let seen = viewer(pty.master, true, stop.clone());
    let path = std::env::temp_dir().join(format!("phosphor-r-{}.json", std::process::id()));
    let o = parse(&args(&format!(
        "play bounce --seconds 3 --report {}",
        path.display()
    )))
    .unwrap();
    let tty = Tty::from_fd(pty.slave).unwrap();
    let (report, json) = play_and_report(tty, &o, &no_env).unwrap();
    stop.store(true, Ordering::Release);
    let seen = seen.join().unwrap();
    let written = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(written.trim_end(), json);
    assert!(report.frames > 0, "{json}");
    assert!(json.contains(&format!("\"frames\":{},", report.frames)));
    // Nobody answered the probe, so it assumed the least.
    assert_eq!(report.profile, "cautious");
    assert_eq!((report.cols, report.rows), (100, 30));
    // No --fps: the beat is the profile's paint rate, and the report says so.
    let paint_hz = phosphor_kernel::Profile::cautious().paint_hz;
    assert_eq!(report.fps, paint_hz);
    assert!(
        json.contains(&format!(
            "\"fps\":{paint_hz},\"fps_from\":\"paint rate\",\"budget_ms\":{:.3},",
            1e3 / paint_hz
        )),
        "{json}"
    );
    assert!((2.9..4.0).contains(&report.seconds), "{json}");
    assert!(report.acks.count > 0, "{json}");
    assert!(report.asking);
    assert_eq!(report.stop, Stop::Time);
    assert_eq!(report.tally.bytes, report.bytes);
    assert!(ends_restored(&seen), "the terminal was not restored");
}

#[test]
fn q_and_ctrl_c_quit_and_restore_the_terminal() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    for key in [&b"q"[..], &[0x03][..]] {
        let pty = Pty::new(60, 20);
        let stop = Arc::new(AtomicBool::new(false));
        let seen = viewer(pty.master, false, stop.clone());
        let master = pty.master;
        let k = key.to_vec();
        let typist = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(2000));
            type_keys(master, &k);
        });
        let o = parse(&args("play bounce --profile apple --seconds 30")).unwrap();
        let tty = Tty::from_fd(pty.slave).unwrap();
        let (report, _) = play_and_report(tty, &o, &no_env).unwrap();
        typist.join().unwrap();
        stop.store(true, Ordering::Release);
        let seen = seen.join().unwrap();
        assert_eq!(report.stop, Stop::Quit, "{key:?}");
        assert!(report.seconds < 5.0, "{key:?} took {}s", report.seconds);
        assert!(report.frames > 0);
        // Nobody answered, so it stopped asking and kept going.
        assert!(!report.asking);
        assert!(
            ends_restored(&seen),
            "{key:?}: the terminal was not restored"
        );
    }
}

#[test]
fn a_new_window_size_starts_again() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let pty = Pty::new(60, 20);
    let stop = Arc::new(AtomicBool::new(false));
    let seen = viewer(pty.master, true, stop.clone());
    let master = pty.master;
    let resizer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        set_size(master, 90, 25);
        unsafe { raise(SIGWINCH) };
    });
    let o = parse(&args("play bounce --profile apple --seconds 1.5")).unwrap();
    let tty = Tty::from_fd(pty.slave).unwrap();
    let (report, _) = play_and_report(tty, &o, &no_env).unwrap();
    resizer.join().unwrap();
    stop.store(true, Ordering::Release);
    let seen = seen.join().unwrap();
    assert_eq!(report.restarts, 1);
    assert_eq!((report.cols, report.rows), (90, 25));
    assert!(ends_restored(&seen));
}

#[test]
fn a_rung_the_profile_has_not_proved_is_refused_and_restores() {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let pty = Pty::new(40, 10);
    let stop = Arc::new(AtomicBool::new(false));
    let seen = viewer(pty.master, false, stop.clone());
    let o = parse(&args("play bounce --profile apple --rung eighth")).unwrap();
    let tty = Tty::from_fd(pty.slave).unwrap();
    assert!(play_and_report(tty, &o, &no_env).is_err());
    std::thread::sleep(Duration::from_millis(100));
    stop.store(true, Ordering::Release);
    let seen = seen.join().unwrap();
    assert!(seen.ends_with(b"\x1b[?25h\x1b[?1049l"));
}
