//! Panics, kept where the next launch can read them.
//!
//! On Android there is no stderr and the user has no `adb`, so a panic
//! on the device used to vanish with the process — the script-editor
//! crash came and went with nothing left to show for it. This module
//! keeps the last one in the app's private storage (the
//! `XDG_DATA_HOME` the Android host points at its files dir, see
//! `rbx_android::boot`), and Home offers it for copying until it is
//! dismissed (see `launcher::home_window::view::crash_notice`).

use std::backtrace::Backtrace;
use std::path::PathBuf;

/// The file the hook writes and the next launch reads: one epoch-seconds
/// line, then the panic with its backtrace.
const CRASH_LOG: &str = "last-crash.log";

/// The folder it lives in, beside the editor's other private data.
fn crash_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(|root| PathBuf::from(root).join("rbx-native"))
}

/// Installs the hook. Logcat first, as the host's own hook already did
/// (this replaces it — the last `set_hook` wins), then the file, so a
/// crash is still visible to whoever does have `adb` and to whoever
/// doesn't. Called from [`crate::boot_mobile`], before anything of the
/// editor's starts.
pub(crate) fn install() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Box<dyn Any>".to_string()
        };
        let at = info
            .location()
            .map_or_else(|| "<unknown>".to_string(), |at| format!("{at}"));
        let thread = std::thread::current();
        let who = thread.name().unwrap_or("<unnamed>").to_string();
        let backtrace = Backtrace::force_capture();
        let when = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        // One line of metadata and the rest report: `take_last_crash`
        // splits on the first newline, so the report itself may contain
        // anything.
        let report =
            format!("{when}\nthread '{who}' panicked at {at}:\n{payload}\n\n{backtrace}\n");
        log::error!("PANIC: {report}");
        // One attempt, errors ignored: a panic hook must not be able to
        // panic itself, and a phone with no storage left has nowhere for
        // a crash log anyway.
        if let Some(dir) = crash_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join(CRASH_LOG), report);
        }
    }));
    install_native();
}

/// The crash file's path, formatted and leaked once so the signal
/// handler below can read it without allocating.
#[cfg(target_os = "android")]
static PATH: std::sync::atomic::AtomicPtr<u8> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
#[cfg(target_os = "android")]
static PATH_LEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Catches the signals that take the process without unwinding, so a
/// native crash leaves the same report a panic does — and then
/// re-raises, with `SA_RESETHAND` having restored the default action,
/// so Android's own tombstone and crash dialog still happen.
///
/// `SIGKILL`, the low-memory killer's signal, cannot be caught: an app
/// that vanishes with no report and no dialog is that, and the answer
/// becomes memory, not this.
#[cfg(target_os = "android")]
fn install_native() {
    if let Some(dir) = crash_dir() {
        let path = dir.join(CRASH_LOG).to_string_lossy().into_owned();
        let bytes: &'static [u8] = Box::leak(path.into_bytes().into_boxed_slice());
        PATH.store(
            bytes.as_ptr() as *mut u8,
            std::sync::atomic::Ordering::Release,
        );
        PATH_LEN.store(bytes.len(), std::sync::atomic::Ordering::Release);
        unsafe {
            for signal in [
                libc::SIGSEGV,
                libc::SIGBUS,
                libc::SIGILL,
                libc::SIGFPE,
                libc::SIGABRT,
            ] {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = caught as libc::sighandler_t;
                action.sa_flags = libc::SA_SIGINFO | libc::SA_RESETHAND;
                libc::sigaction(signal, &action, std::ptr::null_mut());
            }
        }
    }
}

#[cfg(not(target_os = "android"))]
fn install_native() {}

/// Async-signal-safe or it is not here: no allocation, no `format!`,
/// only `clock_gettime`, `open`, `write`, `close` and `raise` — the
/// same first line of epoch seconds `take_last_crash` splits on, then
/// what the signal was and where it landed.
#[cfg(target_os = "android")]
unsafe extern "C" fn caught(signal: i32, info: *mut libc::siginfo_t, _: *mut libc::c_void) {
    let fault = unsafe { (*info).si_addr() } as usize;
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) };
    let mut line = [0u8; 96];
    let mut len = 0;
    push_decimal(&mut line, &mut len, now.tv_sec as u64);
    push_bytes(&mut line, &mut len, b"\nnative signal ");
    push_decimal(&mut line, &mut len, signal as u64);
    push_bytes(&mut line, &mut len, b" at 0x");
    push_hex(&mut line, &mut len, fault);
    push_bytes(&mut line, &mut len, b"\n");
    let path = PATH.load(std::sync::atomic::Ordering::Relaxed);
    let path_len = PATH_LEN.load(std::sync::atomic::Ordering::Relaxed);
    if !path.is_null() && path_len > 0 {
        let fd = unsafe { libc::open(path, libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC, 0o600) };
        if fd >= 0 {
            unsafe {
                libc::write(fd, line.as_ptr().cast(), len);
                libc::close(fd);
            }
        }
    }
    unsafe { libc::raise(signal) };
}

#[cfg(target_os = "android")]
fn push_bytes(line: &mut [u8], len: &mut usize, text: &[u8]) {
    for &byte in text {
        if *len < line.len() {
            line[*len] = byte;
            *len += 1;
        }
    }
}

#[cfg(target_os = "android")]
fn push_decimal(line: &mut [u8], len: &mut usize, value: u64) {
    let mut digits = [0u8; 20];
    let mut count = 0;
    let mut left = value;
    loop {
        digits[count] = b'0' + (left % 10) as u8;
        count += 1;
        left /= 10;
        if left == 0 {
            break;
        }
    }
    while count > 0 {
        count -= 1;
        push_bytes(line, len, &digits[count..count + 1]);
    }
}

#[cfg(target_os = "android")]
fn push_hex(line: &mut [u8], len: &mut usize, value: usize) {
    let mut digits = [0u8; 16];
    let mut count = 0;
    let mut left = value;
    loop {
        let digit = (left & 0xf) as u8;
        digits[count] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        count += 1;
        left >>= 4;
        if left == 0 {
            break;
        }
    }
    while count > 0 {
        count -= 1;
        push_bytes(line, len, &digits[count..count + 1]);
    }
}

/// One kept crash, as Home shows it.
pub(crate) struct CrashReport {
    /// When it happened, in seconds since the Unix epoch.
    pub(crate) when: u64,
    /// The panic message, location and backtrace, ready to paste.
    pub(crate) text: String,
}

/// The crash the last run left behind, taking it with us: reading is
/// dismissing-on-next-launch, so it nags once per crash and returns for
/// the next one.
pub(crate) fn take_last_crash() -> Option<CrashReport> {
    take_from(&crash_dir()?)
}

fn take_from(dir: &std::path::Path) -> Option<CrashReport> {
    let path = dir.join(CRASH_LOG);
    let kept = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let (when, text) = kept.split_once('\n')?;
    let when = when.trim().parse().ok()?;
    if text.trim().is_empty() {
        return None;
    }
    Some(CrashReport {
        when,
        text: text.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hook's own format, round-tripped: one line of when, then the
    /// report `take_last_crash` hands Home. Reading takes the file with
    /// it, so one crash nags once.
    #[test]
    fn a_kept_crash_reads_back_as_when_and_text() {
        let dir = std::env::temp_dir().join(format!("rbx-crash-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let report = "1761234567\nthread 'main' panicked at src/x.rs:9:5:\noops\n\n<backtrace>\n";
        std::fs::write(dir.join(CRASH_LOG), report).unwrap();

        let kept = take_from(&dir).expect("the kept crash reads back");
        assert_eq!(kept.when, 1_761_234_567);
        assert!(kept
            .text
            .starts_with("thread 'main' panicked at src/x.rs:9:5:"));
        assert!(kept.text.contains("<backtrace>"));
        assert!(take_from(&dir).is_none(), "reading took the file away");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The signal handler writes its own shape by hand — same first
    /// line, same split — so a native crash shows up on Home too.
    #[test]
    fn a_native_signal_report_reads_back_the_same_way() {
        let dir = std::env::temp_dir().join(format!("rbx-crash-native-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(CRASH_LOG),
            "1761234567\nnative signal 11 at 0x7fc0000000\n",
        )
        .unwrap();

        let kept = take_from(&dir).expect("the native report reads back");
        assert_eq!(kept.when, 1_761_234_567);
        assert_eq!(kept.text, "native signal 11 at 0x7fc0000000");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
