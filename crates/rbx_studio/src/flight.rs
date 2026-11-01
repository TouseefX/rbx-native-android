//! The flight recorder: what the app's memory was doing, kept where the
//! next launch can read it.
//!
//! The script editor's crash on the device is silent — no panic, no
//! native crash, no dialog — and with no `adb` there is nothing to ask
//! a dead process. This keeps a heartbeat instead: resident memory
//! every two seconds, in the same private storage the crash reports
//! live in, bounded to a few screens of lines. If the app dies again,
//! Home shows the tail: memory that was climbing says leak, memory
//! that was flat and modest says the death was not memory at all. A
//! clean close (`MainActivity`'s `onDestroy`, through the bridge)
//! marks the log so an ordinary exit shows nothing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// How often a reading is taken.
const TICK: std::time::Duration = std::time::Duration::from_secs(2);

/// The log's bound: at it, the oldest half is dropped. One line is
/// twenty bytes, so the file stays trivial however long the session.
const LINES_MAX: usize = 240;

const LOG: &str = "flight.log";

/// The last run's tail, taken once at boot before the recorder starts,
/// for Home's window to show when it opens.
static LAST_RUN: OnceLock<Option<String>> = OnceLock::new();

/// Set by [`mark_shutdown`]: the recorder's loop ends with it.
static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn log_path(dir: &Path) -> PathBuf {
    dir.join(LOG)
}

/// Reads the last run's log — taking it, so one death nags once — and
/// starts the recorder for this one. Order matters: the recorder would
/// happily append to the dead run's log and bury its tail.
pub(crate) fn boot_record() {
    let tail = crate::panic_log::store_dir().and_then(|dir| take_from(&dir));
    let _ = LAST_RUN.set(tail);
    if let Some(dir) = crate::panic_log::store_dir() {
        let path = log_path(&dir);
        let _ = std::thread::Builder::new()
            .name("flight-recorder".to_string())
            .spawn(move || {
                while !SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| since.as_secs());
                    if let Some(kb) = resident_kb() {
                        append(&path, &format!("{now} {kb}"));
                    }
                    std::thread::sleep(TICK);
                }
            });
    }
}

/// The tail [`boot_record`] took, for Home's notice.
pub(crate) fn last_run() -> Option<String> {
    LAST_RUN.get().cloned().flatten()
}

/// The clean-shutdown mark, from `MainActivity.onDestroy` through the
/// bridge: the recorder stops, and the log's last line says the run
/// ended on purpose.
pub fn mark_shutdown() {
    SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some(dir) = crate::panic_log::store_dir() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        append(&log_path(&dir), &format!("{now} shutdown"));
    }
}

/// The last run's readings, if that run ended without the shutdown
/// mark — a death, or a system kill in the background, which the
/// notice says too.
fn take_from(dir: &Path) -> Option<String> {
    let path = log_path(dir);
    let log = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let lines: Vec<&str> = log.lines().filter(|line| !line.is_empty()).collect();
    if lines.last()?.ends_with("shutdown") {
        return None;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let mut tail: Vec<String> = lines
        .iter()
        .rev()
        .take(5)
        .filter_map(|line| reading_label(line, now))
        .collect();
    if tail.is_empty() {
        return None;
    }
    tail.reverse();
    Some(tail.join("; "))
}

/// One reading as the notice words it: the memory, and when.
fn reading_label(line: &str, now: u64) -> Option<String> {
    let mut parts = line.split_whitespace();
    let at: u64 = parts.next()?.parse().ok()?;
    let kb: u64 = parts.next()?.parse().ok()?;
    let minutes = now.saturating_sub(at) / 60;
    Some(format!("{} MB {minutes} min ago", kb / 1024))
}

/// Appends one line, bounding the file by rewriting it half-size once
/// it crosses the cap.
fn append(path: &Path, line: &str) {
    if let Ok(log) = std::fs::read_to_string(path) {
        if log.lines().count() >= LINES_MAX {
            let kept: Vec<&str> = log.lines().skip(LINES_MAX / 2).collect();
            let _ = std::fs::write(path, format!("{}\n", kept.join("\n")));
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Resident memory in KB, from the kernel's own accounting of this
/// process. arm64 Android's base page is 4 KB; a 16 KB-page device
/// over-reports fourfold, which a leak hunt survives.
fn resident_kb() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    Some(statm_pages(&statm)?.saturating_mul(4))
}

/// `/proc/self/statm`'s second field: resident pages.
fn statm_pages(statm: &str) -> Option<u64> {
    statm.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statm_reads_the_second_field_as_pages() {
        assert_eq!(statm_pages("1234 567 89 10 11 12 13"), Some(567));
        assert_eq!(statm_pages("1234"), None);
        assert_eq!(statm_pages(""), None);
    }

    #[test]
    fn a_reading_names_its_memory_and_its_age() {
        assert_eq!(
            reading_label("1761230000 2097152", 1_761_233_600),
            Some("2048 MB 60 min ago".to_string())
        );
        assert_eq!(reading_label("0 0", 60), Some("0 MB 1 min ago".to_string()));
        assert_eq!(reading_label("not a reading", 0), None);
    }

    #[test]
    fn an_unmarked_log_reads_back_and_a_marked_one_does_not() {
        let dir = std::env::temp_dir().join(format!("rbx-flight-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        std::fs::write(log_path(&dir), format!("{now} 524288\n{}\n", now - 120)).unwrap();
        // The second line has no reading in it: the take skips what it
        // cannot read rather than stopping at the first odd line.
        let taken = take_from(&dir).expect("the unmarked log reads back");
        assert!(taken.contains("512 MB"), "{taken}");

        std::fs::write(log_path(&dir), format!("{now} 524288\n{now} shutdown\n")).unwrap();
        assert_eq!(take_from(&dir), None, "a marked log shows nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
