use std::path::{Path, PathBuf};
use std::process::Command;

fn homebrew_cache(home: &Path) -> PathBuf {
    let opt_arm = PathBuf::from("/opt/homebrew/var/homebrew/cache");
    if opt_arm.is_dir() {
        return opt_arm;
    }
    let opt_intel = PathBuf::from("/usr/local/Homebrew/Homebrew/cache");
    if opt_intel.is_dir() {
        return opt_intel;
    }
    home.join("Library/Caches/Homebrew")
}

pub fn general_junk_candidates(home: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        ("User caches", home.join("Library/Caches")),
        ("User logs", home.join("Library/Logs")),
        ("Diagnostic reports", home.join("Library/Logs/DiagnosticReports")),
        (
            "Safari cache",
            home.join("Library/Caches/com.apple.Safari"),
        ),
        (
            "Mail downloads",
            home.join("Library/Containers/com.apple.mail/Data/Library/Mail Downloads"),
        ),
        (
            "Spotify cache",
            home.join("Library/Application Support/Spotify/PersistentCache/Storage"),
        ),
        (
            "Adobe media cache",
            home.join("Library/Application Support/Adobe/Common/Media Cache Files"),
        ),
        (
            "Xcode derived data",
            home.join("Library/Developer/Xcode/DerivedData"),
        ),
        ("pip cache", home.join("Library/Caches/pip")),
        ("npm cache", home.join(".npm/_cacache")),
        ("Homebrew cache", homebrew_cache(home)),
    ]
}

pub fn dev_junk_candidates(home: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        (
            "Xcode derived data",
            home.join("Library/Developer/Xcode/DerivedData"),
        ),
        (
            "Xcode Archives",
            home.join("Library/Developer/Xcode/Archives"),
        ),
        (
            "Xcode iOS DeviceSupport",
            home.join("Library/Developer/Xcode/iOS DeviceSupport"),
        ),
        (
            "CoreSimulator caches",
            home.join("Library/Developer/CoreSimulator/Caches"),
        ),
        ("Cargo registry", home.join(".cargo/registry")),
        ("Cargo git checkouts", home.join(".cargo/git")),
        ("Gradle caches", home.join(".gradle/caches")),
        ("Yarn cache", home.join("Library/Caches/Yarn")),
        ("pnpm store", home.join("Library/pnpm/store")),
        ("Go build cache", home.join("Library/Caches/go-build")),
        ("Docker local data", home.join(".docker")),
        ("Homebrew cache", homebrew_cache(home)),
        ("Node modules (Projects)", home.join("Projects")),
    ]
}

fn all_junk_candidates(home: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (label, path) in general_junk_candidates(home)
        .into_iter()
        .chain(dev_junk_candidates(home))
    {
        if seen.insert(path.clone()) {
            out.push((label, path));
        }
    }
    out
}

pub fn junk_candidates(home: &Path) -> Vec<(&'static str, PathBuf)> {
    general_junk_candidates(home)
}

fn survey_candidates(candidates: Vec<(&'static str, PathBuf)>) -> Vec<JunkRoot> {
    candidates
        .into_iter()
        .map(|(label, path)| {
            let exists = path.is_dir();
            let bytes = if exists { directory_size(&path) } else { 0 };
            JunkRoot {
                label: label.to_string(),
                path,
                bytes,
                exists,
            }
        })
        .collect()
}

/// User-level cache and log roots named in the cleanup notes.
/// Survey is read-only. Trash is a separate call and only accepts these roots.
pub fn junk_roots(home: &Path) -> Vec<JunkRoot> {
    survey_candidates(general_junk_candidates(home))
}

pub fn dev_junk_roots(home: &Path) -> Vec<JunkRoot> {
    survey_candidates(dev_junk_candidates(home))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JunkRoot {
    pub label: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub exists: bool,
}

pub fn directory_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    let mut seen = 0usize;
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > 80_000 {
                return total;
            }
            let path = entry.path();
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

/// Finder trash, matching `trashItemAtURL`. Refuses anything outside the surveyed roots.
pub fn move_root_to_trash(home: &Path, path: &Path) -> Result<(), String> {
    let allowed = all_junk_candidates(home)
        .into_iter()
        .any(|(_, root)| root == path && path.is_dir());
    if !allowed {
        return Err("only a surveyed cache or log root can be moved to the Trash".into());
    }
    finder_trash(path)
}

/// Moves one file that lives under `~/Downloads`. Refuses the Downloads folder itself.
pub fn move_download_to_trash(home: &Path, path: &Path) -> Result<(), String> {
    let downloads = home.join("Downloads");
    let under = path.starts_with(&downloads) && path != downloads;
    let file = path.is_file();
    if !under || !file {
        return Err("only a file inside Downloads can be moved to the Trash".into());
    }
    finder_trash(path)
}

pub(crate) fn finder_trash(path: &Path) -> Result<(), String> {
    let text = path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(r#"tell application "Finder" to delete POSIX file "{text}""#);
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
