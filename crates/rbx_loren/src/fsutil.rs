//! File helpers with upstream's semantics (`lib/fsutil.js`): sorted walks that
//! skip VCS and backup folders, recursive copies, posix-style relative paths.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub fn is_file(p: &Path) -> bool {
    fs::metadata(p).is_ok_and(|m| m.is_file())
}

pub fn is_dir(p: &Path) -> bool {
    fs::metadata(p).is_ok_and(|m| m.is_dir())
}

/// `fs.existsSync`: follows symlinks.
pub fn exists(p: &Path) -> bool {
    fs::metadata(p).is_ok()
}

/// UTF-8 text (lossy, as Node's `'utf8'` decoding is).
pub fn read_text(p: &Path) -> io::Result<String> {
    fs::read(p).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

pub fn read_text_if_exists(p: &Path) -> Option<String> {
    if is_file(p) {
        read_text(p).ok()
    } else {
        None
    }
}

/// Writes, creating the parent folders.
pub fn write_text(p: &Path, text: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(p, text)
}

/// `rm -rf`, silent when nothing is there.
pub fn remove_path(p: &Path) {
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => {
            let _ = fs::remove_dir_all(p);
        }
        Ok(_) => {
            let _ = fs::remove_file(p);
        }
        Err(_) => {}
    }
}

/// `path.resolve` without touching the disk: absolute, `.` and `..` folded.
pub fn resolve(base: &Path, p: impl AsRef<Path>) -> PathBuf {
    let joined = base.join(p);
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `path.relative(from, to)`, both resolved first.
pub fn relative(from: &Path, to: &Path) -> PathBuf {
    let (from, to) = (resolve(Path::new("/"), from), resolve(Path::new("/"), to));
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for c in &to[common..] {
        out.push(c.as_os_str());
    }
    out
}

pub fn to_posix(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Project-relative path with forward slashes, for messages (`.` for the root).
pub fn rel(root: &Path, p: &Path) -> String {
    let r = to_posix(&relative(root, p));
    if r.is_empty() {
        ".".to_string()
    } else {
        r
    }
}

/// True when `p` is `root` or inside it.
pub fn is_inside(root: &Path, p: &Path) -> bool {
    let r = relative(root, p);
    !matches!(r.components().next(), Some(Component::ParentDir)) && !r.is_absolute()
}

/// Decides whether `copy_dir` copies an entry: `(relative path, is a folder)`.
pub type Filter<'a> = &'a dyn Fn(&str, bool) -> bool;
/// Rewrites a copied file's bytes: `(relative path, contents)`.
pub type Transform<'a> = Option<&'a dyn Fn(&str, Vec<u8>) -> Vec<u8>>;

pub const DEFAULT_SKIP: [&str; 3] = [".git", "node_modules", ".loren-backup"];

/// Every file under `dir` (sorted by name at each level), skipping folders named in `skip`.
pub fn walk_files(dir: &Path, skip: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    visit(dir, skip, &mut out);
    out
}

fn visit(dir: &Path, skip: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let full = entry.path();
        let name = entry.file_name();
        if is_dir(&full) {
            if !skip.iter().any(|s| name == *s) {
                visit(&full, skip, out);
            }
        } else if is_file(&full) {
            out.push(full);
        }
    }
}

/// Relative (posix) paths of every file under `dir`, nothing skipped.
pub fn list_rel_files(dir: &Path) -> Vec<String> {
    if !is_dir(dir) {
        return Vec::new();
    }
    walk_files(dir, &[])
        .iter()
        .map(|f| to_posix(&relative(dir, f)))
        .collect()
}

/// Copies a folder. `filter(rel, is_dir)` drops entries; `transform(rel, bytes)`
/// may rewrite a file's contents. Returns the copied files' relative paths.
pub fn copy_dir(
    src: &Path,
    dest: &Path,
    filter: Filter,
    transform: Transform,
) -> io::Result<Vec<String>> {
    let mut copied = Vec::new();
    copy_into(src, dest, "", filter, transform, &mut copied)?;
    Ok(copied)
}

fn copy_into(
    from: &Path,
    to: &Path,
    rel_base: &str,
    filter: Filter,
    transform: Transform,
    copied: &mut Vec<String>,
) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel_path = if rel_base.is_empty() {
            name.clone()
        } else {
            format!("{rel_base}/{name}")
        };
        let full = entry.path();
        let directory = is_dir(&full);
        if !filter(&rel_path, directory) {
            continue;
        }
        if directory {
            copy_into(&full, &to.join(&name), &rel_path, filter, transform, copied)?;
        } else {
            let mut data = fs::read(&full)?;
            if let Some(transform) = transform {
                data = transform(&rel_path, data);
            }
            fs::write(to.join(&name), data)?;
            copied.push(rel_path);
        }
    }
    Ok(())
}

/// Copies a file or a folder.
pub fn copy_path(src: &Path, dest: &Path) -> io::Result<()> {
    if is_dir(src) {
        copy_dir(src, dest, &|_, _| true, None).map(|_| ())
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest).map(|_| ())
    }
}

/// `2026-09-24T03-06-00-123Z`: sortable and safe in file names on every OS.
pub fn timestamp(now: std::time::SystemTime) -> String {
    let since = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}-{:02}-{:02}-{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        since.subsec_millis()
    )
}

/// Howard Hinnant's days-to-civil conversion.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn normalize_eol(text: &str) -> String {
    text.replace("\r\n", "\n")
}

pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}
