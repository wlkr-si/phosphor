//! The line to a real terminal: raw mode, sending, receiving, and always restoring it.
//!
//! `Tty` is the one place Phosphor touches a terminal. Opening it saves the terminal's
//! settings, switches to raw mode, enters the alternate screen and hides the cursor.
//! Everything is put back when it is dropped, when the program panics, and when the
//! program is interrupted, terminated or hung up.
use phosphor_kernel::Wire;
use std::cell::UnsafeCell;
use std::io;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Once;
use std::time::{Duration, Instant};

/// Enter the alternate screen, then hide the cursor.
const ENTER: &[u8] = b"\x1b[?1049h\x1b[?25l";
/// Show the cursor, then leave the alternate screen.
const LEAVE: &[u8] = b"\x1b[?25h\x1b[?1049l";

// ---- What a signal handler or panic hook may need to restore -------------------------
//
// Handlers may only use async-signal-safe calls, so live terminals are kept in a fixed
// table of atomics: no locks, no allocation.

const FREE: RawFd = -1;
const BUSY: RawFd = -2;
const SLOT_COUNT: usize = 16;

struct Slot {
    /// `FREE`, `BUSY` while being filled in or torn down, or the live descriptor.
    fd: AtomicI32,
    saved: UnsafeCell<libc::termios>,
}

// `saved` is written only by the thread that holds the slot as `BUSY`, and read only
// after `fd` has been published as a live descriptor.
unsafe impl Sync for Slot {}

#[allow(clippy::declare_interior_mutable_const)]
const EMPTY_SLOT: Slot = Slot {
    fd: AtomicI32::new(FREE),
    saved: UnsafeCell::new(unsafe { std::mem::zeroed() }),
};
static SLOTS: [Slot; SLOT_COUNT] = [EMPTY_SLOT; SLOT_COUNT];

static WINDOW_CHANGED: AtomicBool = AtomicBool::new(false);

fn claim_slot(fd: RawFd, saved: &libc::termios) -> Option<usize> {
    for (i, slot) in SLOTS.iter().enumerate() {
        if slot
            .fd
            .compare_exchange(FREE, BUSY, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            unsafe { *slot.saved.get() = *saved };
            slot.fd.store(fd, Ordering::Release);
            return Some(i);
        }
    }
    None
}

/// Leave the alternate screen, show the cursor, put the settings back. Safe to call from a
/// signal handler. Idempotent.
fn restore_fd(fd: RawFd, saved: &libc::termios) {
    unsafe {
        libc::write(fd, LEAVE.as_ptr().cast(), LEAVE.len());
        libc::tcsetattr(fd, libc::TCSANOW, saved);
    }
}

/// Restore every live terminal. Safe to call from a signal handler.
fn restore_all() {
    for slot in SLOTS.iter() {
        let fd = slot.fd.load(Ordering::Acquire);
        if fd >= 0 {
            restore_fd(fd, unsafe { &*slot.saved.get() });
        }
    }
}

extern "C" fn on_fatal_signal(sig: libc::c_int) {
    restore_all();
    // The handler was installed with SA_RESETHAND, so the default action is back. Raising
    // the signal again leaves it pending until this handler returns, then ends the program
    // exactly as the signal would have.
    unsafe { libc::raise(sig) };
}

extern "C" fn on_window_change(_: libc::c_int) {
    WINDOW_CHANGED.store(true, Ordering::Release);
}

fn install_handlers() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_all();
            previous(info);
        }));
        unsafe {
            for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
                let mut act: libc::sigaction = std::mem::zeroed();
                act.sa_sigaction = on_fatal_signal as extern "C" fn(libc::c_int) as usize;
                act.sa_flags = libc::SA_RESETHAND;
                libc::sigemptyset(&mut act.sa_mask);
                libc::sigaction(sig, &act, std::ptr::null_mut());
            }
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = on_window_change as extern "C" fn(libc::c_int) as usize;
            act.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut act.sa_mask);
            libc::sigaction(libc::SIGWINCH, &act, std::ptr::null_mut());
        }
    });
}

// ---- Waiting ------------------------------------------------------------------------

/// Wait until `fd` can be read (`write = false`) or written, for at most `timeout`.
/// Returns whether it is ready (or hung up). Never spins.
fn wait_ready(fd: RawFd, write: bool, timeout: Duration) -> io::Result<bool> {
    let ms = timeout
        .as_micros()
        .div_ceil(1000)
        .min(libc::c_int::MAX as u128) as libc::c_int;
    let want = if write { libc::POLLOUT } else { libc::POLLIN };
    let mut p = libc::pollfd {
        fd,
        events: want,
        revents: 0,
    };
    let r = unsafe { libc::poll(&mut p, 1, ms) };
    if r < 0 {
        let e = io::Error::last_os_error();
        return if e.kind() == io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(e)
        };
    }
    if r > 0 && p.revents & libc::POLLNVAL != 0 {
        // Some systems' poll refuses some terminal devices. select handles them.
        return select_ready(fd, write, timeout);
    }
    Ok(r > 0)
}

fn select_ready(fd: RawFd, write: bool, timeout: Duration) -> io::Result<bool> {
    if fd as usize >= libc::FD_SETSIZE {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let mut set: libc::fd_set = unsafe { std::mem::zeroed() };
    unsafe {
        libc::FD_ZERO(&mut set);
        libc::FD_SET(fd, &mut set);
    }
    let mut tv = libc::timeval {
        tv_sec: timeout.as_secs().min(i32::MAX as u64) as _,
        tv_usec: timeout.subsec_micros() as _,
    };
    let (r, w) = if write {
        (std::ptr::null_mut(), &mut set as *mut _)
    } else {
        (&mut set as *mut _, std::ptr::null_mut())
    };
    let n = unsafe { libc::select(fd + 1, r, w, std::ptr::null_mut(), &mut tv) };
    if n < 0 {
        let e = io::Error::last_os_error();
        return if e.kind() == io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(e)
        };
    }
    Ok(n > 0)
}

// ---- Tty ----------------------------------------------------------------------------

pub struct Tty {
    fd: RawFd,
    saved: libc::termios,
    slot: usize,
    /// Bytes read past the end byte of the last `recv_until`, kept for the next one.
    pending: Vec<u8>,
}

impl Tty {
    /// Open the process's controlling terminal, `/dev/tty`.
    pub fn open() -> io::Result<Tty> {
        let fd = unsafe {
            libc::open(
                c"/dev/tty".as_ptr(),
                libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Tty::take(fd)
    }

    /// Use an already open terminal descriptor. The `Tty` keeps its own duplicate of it,
    /// so the caller's descriptor stays theirs to close.
    pub fn from_fd(fd: RawFd) -> io::Result<Tty> {
        let own = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
        if own < 0 {
            return Err(io::Error::last_os_error());
        }
        Tty::take(own)
    }

    /// Take ownership of `fd`, put it in raw mode, enter the alternate screen.
    fn take(fd: RawFd) -> io::Result<Tty> {
        install_handlers();
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            let e = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(e);
        }
        let Some(slot) = claim_slot(fd, &saved) else {
            unsafe { libc::close(fd) };
            return Err(io::Error::other("too many terminals open"));
        };
        // From here, dropping `tty` undoes whatever has been done.
        let mut tty = Tty {
            fd,
            saved,
            slot,
            pending: Vec::new(),
        };
        let mut raw = saved;
        unsafe { libc::cfmakeraw(&mut raw) };
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        tty.send(ENTER)?;
        Ok(tty)
    }

    /// The window size as (columns, rows).
    pub fn size(&self) -> io::Result<(usize, usize)> {
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(self.fd, libc::TIOCGWINSZ, &mut ws) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((ws.ws_col as usize, ws.ws_row as usize))
    }

    /// True once after each window-size signal.
    pub fn resized(&self) -> bool {
        WINDOW_CHANGED.swap(false, Ordering::AcqRel)
    }
}

impl Wire for Tty {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut rest = bytes;
        while !rest.is_empty() {
            let n = unsafe { libc::write(self.fd, rest.as_ptr().cast(), rest.len()) };
            if n < 0 {
                let e = io::Error::last_os_error();
                match e.kind() {
                    io::ErrorKind::Interrupted => continue,
                    io::ErrorKind::WouldBlock => {
                        wait_ready(self.fd, true, Duration::from_secs(1))?;
                        continue;
                    }
                    _ => return Err(e),
                }
            }
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::WriteZero));
            }
            rest = &rest[n as usize..];
        }
        Ok(())
    }

    fn recv_until(&mut self, end: u8, timeout: Duration) -> io::Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(i) = self.pending.iter().position(|&b| b == end) {
                return Ok(self.pending.drain(..=i).collect());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if !wait_ready(self.fd, false, left)? {
                if Instant::now() >= deadline {
                    break;
                }
                continue;
            }
            let n = unsafe { libc::read(self.fd, chunk.as_mut_ptr().cast(), chunk.len()) };
            if n < 0 {
                let e = io::Error::last_os_error();
                match e.kind() {
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => continue,
                    _ => return Err(e),
                }
            }
            if n == 0 {
                break;
            }
            self.pending.extend_from_slice(&chunk[..n as usize]);
        }
        Ok(std::mem::take(&mut self.pending))
    }
}

impl Drop for Tty {
    fn drop(&mut self) {
        let slot = &SLOTS[self.slot];
        // Hide the table entry from handlers while we tear down, then free it.
        slot.fd.store(BUSY, Ordering::Release);
        restore_fd(self.fd, &self.saved);
        unsafe { libc::close(self.fd) };
        slot.fd.store(FREE, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::Mutex;

    /// The terminals and the panic hook are process-wide, so tests that use them take turns.
    static TURN: Mutex<()> = Mutex::new(());

    fn take_turn() -> std::sync::MutexGuard<'static, ()> {
        TURN.lock().unwrap_or_else(|e| e.into_inner())
    }

    struct Pty {
        master: RawFd,
        slave: RawFd,
    }

    impl Pty {
        fn new() -> Pty {
            let (mut m, mut s) = (0, 0);
            let r = unsafe {
                libc::openpty(
                    &mut m,
                    &mut s,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            assert_eq!(r, 0, "openpty failed");
            Pty {
                master: m,
                slave: s,
            }
        }

        fn settings(&self) -> libc::termios {
            let mut t: libc::termios = unsafe { std::mem::zeroed() };
            assert_eq!(unsafe { libc::tcgetattr(self.slave, &mut t) }, 0);
            t
        }

        /// Read from the master until nothing more arrives for `quiet`.
        fn drain(&self, quiet: Duration) -> Vec<u8> {
            let mut out = Vec::new();
            let mut buf = [0u8; 4096];
            while wait_ready(self.master, false, quiet).unwrap() {
                let n = unsafe { libc::read(self.master, buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 {
                    break;
                }
                out.extend_from_slice(&buf[..n as usize]);
            }
            out
        }

        fn write_master(&self, bytes: &[u8]) {
            let n = unsafe { libc::write(self.master, bytes.as_ptr().cast(), bytes.len()) };
            assert_eq!(n, bytes.len() as isize);
        }
    }

    impl Drop for Pty {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.master);
                libc::close(self.slave);
            }
        }
    }

    /// The kernel itself sets PENDIN (retype pending input) when a terminal leaves line mode.
    /// It is not a setting anyone chose, so it is left out of the comparison.
    const PENDIN: libc::tcflag_t = 0x2000_0000;

    fn same(a: &libc::termios, b: &libc::termios) -> bool {
        a.c_iflag == b.c_iflag
            && a.c_oflag == b.c_oflag
            && a.c_cflag == b.c_cflag
            && a.c_lflag & !PENDIN == b.c_lflag & !PENDIN
            && a.c_cc == b.c_cc
            && a.c_ispeed == b.c_ispeed
            && a.c_ospeed == b.c_ospeed
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn drop_restores_settings_and_screen() {
        let _turn = take_turn();
        let pty = Pty::new();
        let before = pty.settings();
        let tty = Tty::from_fd(pty.slave).unwrap();
        assert!(!same(&before, &pty.settings()), "raw mode was not entered");
        let entered = pty.drain(Duration::from_millis(50));
        assert!(contains(&entered, ENTER));
        drop(tty);
        assert!(same(&before, &pty.settings()));
        let left = pty.drain(Duration::from_millis(50));
        assert!(contains(&left, b"\x1b[?25h"), "show-cursor not written");
        assert!(
            contains(&left, b"\x1b[?1049l"),
            "leave-alternate-screen not written"
        );
    }

    #[test]
    fn send_delivers_a_megabyte() {
        let _turn = take_turn();
        let pty = Pty::new();
        let mut tty = Tty::from_fd(pty.slave).unwrap();
        let data: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
        let master = pty.master;
        let want = data.len() + ENTER.len();
        let reader = std::thread::spawn(move || {
            let mut got = Vec::new();
            let mut buf = [0u8; 65536];
            while got.len() < want {
                if !wait_ready(master, false, Duration::from_secs(10)).unwrap() {
                    break;
                }
                let n = unsafe { libc::read(master, buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n as usize]);
            }
            got
        });
        tty.send(&data).unwrap();
        let got = reader.join().unwrap();
        assert_eq!(&got[..ENTER.len()], ENTER);
        assert_eq!(got.len(), want);
        assert!(got[ENTER.len()..] == data[..], "bytes were altered or lost");
    }

    #[test]
    fn recv_until_returns_when_the_end_byte_arrives() {
        let _turn = take_turn();
        let pty = Pty::new();
        let mut tty = Tty::from_fd(pty.slave).unwrap();
        pty.write_master(b"abc!def");
        let started = Instant::now();
        assert_eq!(
            tty.recv_until(b'!', Duration::from_secs(5)).unwrap(),
            b"abc!"
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        // What came after the end byte is kept for the next call.
        assert_eq!(
            tty.recv_until(b'f', Duration::from_secs(5)).unwrap(),
            b"def"
        );
    }

    #[test]
    fn recv_until_returns_promptly_when_the_end_byte_comes_later() {
        let _turn = take_turn();
        let pty = Pty::new();
        let mut tty = Tty::from_fd(pty.slave).unwrap();
        let master = pty.master;
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            unsafe { libc::write(master, b"ok\n".as_ptr().cast(), 3) };
        });
        let started = Instant::now();
        let got = tty.recv_until(b'\n', Duration::from_secs(10)).unwrap();
        assert_eq!(got, b"ok\n");
        assert!(started.elapsed() < Duration::from_secs(5));
        writer.join().unwrap();
    }

    #[test]
    fn recv_until_returns_what_it_has_at_the_time_limit() {
        let _turn = take_turn();
        let pty = Pty::new();
        let mut tty = Tty::from_fd(pty.slave).unwrap();
        pty.write_master(b"abc");
        let started = Instant::now();
        let got = tty.recv_until(b'!', Duration::from_millis(200)).unwrap();
        let took = started.elapsed();
        assert_eq!(got, b"abc");
        assert!(
            took >= Duration::from_millis(190),
            "returned early: {took:?}"
        );
        assert!(took < Duration::from_secs(2), "returned late: {took:?}");
        assert_eq!(
            tty.recv_until(b'!', Duration::from_millis(20)).unwrap(),
            b""
        );
    }

    #[test]
    fn size_reports_the_window_size() {
        let _turn = take_turn();
        let pty = Pty::new();
        let ws = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(unsafe { libc::ioctl(pty.master, libc::TIOCSWINSZ, &ws) }, 0);
        let tty = Tty::from_fd(pty.slave).unwrap();
        assert_eq!(tty.size().unwrap(), (80, 24));
        let ws = libc::winsize {
            ws_row: 50,
            ws_col: 132,
            ..ws
        };
        assert_eq!(unsafe { libc::ioctl(pty.master, libc::TIOCSWINSZ, &ws) }, 0);
        assert_eq!(tty.size().unwrap(), (132, 50));
    }

    #[test]
    fn resized_is_true_once_after_each_window_signal() {
        let _turn = take_turn();
        let pty = Pty::new();
        let tty = Tty::from_fd(pty.slave).unwrap();
        tty.resized();
        assert!(!tty.resized());
        unsafe { libc::raise(libc::SIGWINCH) };
        assert!(tty.resized());
        assert!(!tty.resized());
        unsafe { libc::raise(libc::SIGWINCH) };
        assert!(tty.resized());
    }

    #[test]
    fn a_panic_restores_the_settings() {
        let _turn = take_turn();
        let pty = Pty::new();
        let before = pty.settings();
        let slave = pty.slave;
        let result = std::panic::catch_unwind(move || {
            let _tty = Tty::from_fd(slave).unwrap();
            panic!("on purpose");
        });
        assert!(result.is_err());
        assert!(same(&before, &pty.settings()));
        let out = pty.drain(Duration::from_millis(50));
        assert!(contains(&out, b"\x1b[?1049l"));
    }

    #[test]
    fn the_panic_hook_restores_even_when_nothing_is_dropped() {
        let _turn = take_turn();
        let pty = Pty::new();
        let before = pty.settings();
        let tty = Tty::from_fd(pty.slave).unwrap();
        // A panic in a program built to abort never runs destructors; the hook alone acts.
        let _ = std::panic::catch_unwind(|| panic!("on purpose, hook only"));
        assert!(same(&before, &pty.settings()));
        drop(tty);
    }

    /// Runs inside the child process started by `signals_restore_then_end_the_program`.
    #[test]
    fn signal_child() {
        let (Ok(fd), Ok(sig)) = (
            std::env::var("PHOSPHOR_TTY_CHILD_FD"),
            std::env::var("PHOSPHOR_TTY_CHILD_SIGNAL"),
        ) else {
            return;
        };
        let _tty = Tty::from_fd(fd.parse().unwrap()).unwrap();
        unsafe { libc::raise(sig.parse().unwrap()) };
        std::thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn signals_restore_then_end_the_program() {
        let _turn = take_turn();
        for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            let pty = Pty::new();
            let before = pty.settings();
            unsafe {
                let flags = libc::fcntl(pty.slave, libc::F_GETFD);
                libc::fcntl(pty.slave, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
            }
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "tty::tests::signal_child", "--test-threads=1"])
                .env("PHOSPHOR_TTY_CHILD_FD", pty.slave.to_string())
                .env("PHOSPHOR_TTY_CHILD_SIGNAL", sig.to_string())
                .status()
                .unwrap();
            assert_eq!(
                status.signal(),
                Some(sig),
                "child should end by signal {sig}"
            );
            assert!(
                same(&before, &pty.settings()),
                "settings not restored after {sig}"
            );
            let out = pty.drain(Duration::from_millis(50));
            assert!(contains(&out, b"\x1b[?25h"));
            assert!(contains(&out, b"\x1b[?1049l"));
        }
    }
}
