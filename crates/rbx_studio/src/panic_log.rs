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
}
