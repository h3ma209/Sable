//! Exact-hash malware check against a public signature list.
//!
//! Signatures come from the abuse.ch MalwareBazaar recent SHA-256 export, an
//! open defender feed. This module does not embed or decrypt any other product's
//! detection database. It does not read browser cookies, history, passwords, or
//! chat logs, and it does not change app permissions.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use crate::cleanup::finder_trash;
use crate::hash::sha256_file;

pub const FEED_URL: &str = "https://bazaar.abuse.ch/export/txt/sha256/recent/";
const MAX_HASH_BYTES: u64 = 50 * 1024 * 1024;
const MAX_FILES: u64 = 20_000;
const MAX_DEPTH: u32 = 8;
const MAX_SIGNATURES: usize = 500_000;

#[derive(Clone, Debug)]
pub struct ThreatHit {
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct ThreatReport {
    pub root: PathBuf,
    pub hits: Vec<ThreatHit>,
    pub scanned: u64,
    pub skipped_large: u64,
    pub unreadable: u64,
    pub signature_count: usize,
    pub signatures_note: String,
}

pub fn support_dir(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Sable")
}

pub fn signature_cache_path(home: &Path) -> PathBuf {
    support_dir(home).join("malwarebazaar-sha256-recent.txt")
}

fn ignore_path_file(home: &Path) -> PathBuf {
    support_dir(home).join("threat-ignore.txt")
}

/// Lines that are 64 hex characters. Comments and prose are dropped.
pub fn parse_sha256_list(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for line in text.lines() {
        if out.len() >= MAX_SIGNATURES {
            break;
        }
        let line = line.trim().to_ascii_lowercase();
        if line.len() == 64 && line.bytes().all(|b| b.is_ascii_hexdigit()) {
            out.insert(line);
        }
    }
    out
}

pub fn load_ignore(home: &Path) -> HashSet<PathBuf> {
    let text = fs::read_to_string(ignore_path_file(home)).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

pub fn remember_ignore(home: &Path, path: &Path) -> Result<(), String> {
    let file = ignore_path_file(home);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut existing = fs::read_to_string(&file).unwrap_or_default();
    let line = path.display().to_string();
    if existing.lines().any(|item| item == line) {
        return Ok(());
    }
    if !existing.is_empty() && !existing.ends_with('\n') {
        existing.push('\n');
    }
    existing.push_str(&line);
    existing.push('\n');
    fs::write(file, existing).map_err(|err| err.to_string())
}

/// Use a cached list younger than one day. Otherwise download a fresh copy.
/// A failed download keeps the previous cache when one exists.
pub fn load_signatures(home: &Path) -> Result<(HashSet<String>, String), String> {
    let cache = signature_cache_path(home);
    if let Some(parent) = cache.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let fresh = cache_age(&cache)
        .map(|age| age < Duration::from_secs(24 * 60 * 60))
        .unwrap_or(false);
    if !fresh {
        let tmp = cache.with_extension("txt.partial");
        match download_feed(&tmp) {
            Ok(()) => {
                let _ = fs::rename(&tmp, &cache);
            }
            Err(err) => {
                let _ = fs::remove_file(&tmp);
                if !cache.exists() {
                    return Err(err);
                }
            }
        }
    }
    let text = fs::read_to_string(&cache).map_err(|err| err.to_string())?;
    let set = parse_sha256_list(&text);
    if set.is_empty() {
        return Err("the signature list did not contain any hashes".into());
    }
    let note = if fresh {
        format!("{} hashes, cached list", set.len())
    } else {
        format!("{} hashes from MalwareBazaar", set.len())
    };
    Ok((set, note))
}

fn cache_age(path: &Path) -> Option<Duration> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

fn download_feed(dest: &Path) -> Result<(), String> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "60",
            "-A",
            "Sable",
            "-o",
        ])
        .arg(dest)
        .arg(FEED_URL)
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("could not download signatures: {}", err.trim()))
    }
}

pub fn scan_threats(root: &Path, signatures: &HashSet<String>, ignore: &HashSet<PathBuf>) -> ThreatReport {
    let mut report = ThreatReport {
        root: root.to_path_buf(),
        hits: Vec::new(),
        scanned: 0,
        skipped_large: 0,
        unreadable: 0,
        signature_count: signatures.len(),
        signatures_note: String::new(),
    };
    let mut stack = vec![(root.to_path_buf(), 0u32)];
    while let Some((dir, depth)) = stack.pop() {
        if report.scanned >= MAX_FILES {
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                report.unreadable += 1;
                continue;
            }
        };
        for entry in entries.flatten() {
            if report.scanned >= MAX_FILES {
                break;
            }
            let path = entry.path();
            if skip_private(&path) {
                continue;
            }
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(_) => {
                    report.unreadable += 1;
                    continue;
                }
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                if depth < MAX_DEPTH {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            if meta.len() > MAX_HASH_BYTES {
                report.skipped_large += 1;
                continue;
            }
            report.scanned += 1;
            if ignore.contains(&path) {
                continue;
            }
            match sha256_file(&path) {
                Ok(hash) if signatures.contains(&hash) => report.hits.push(ThreatHit {
                    path,
                    size: meta.len(),
                    sha256: hash,
                }),
                Ok(_) => {}
                Err(_) => report.unreadable += 1,
            }
        }
    }
    report.hits.sort_by(|a, b| a.path.cmp(&b.path));
    report
}

pub fn prepare_and_scan(home: &Path, root: &Path) -> Result<ThreatReport, String> {
    let (signatures, note) = load_signatures(home)?;
    let ignore = load_ignore(home);
    let mut report = scan_threats(root, &signatures, &ignore);
    report.signatures_note = note;
    Ok(report)
}

fn skip_private(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    const FILES: &[&str] = &[
        "Cookies",
        "Cookies.binarycookies",
        "History",
        "Login Data",
        "Web Data",
        "cookies.sqlite",
        "places.sqlite",
        "chat.db",
    ];
    if FILES.iter().any(|item| name.eq_ignore_ascii_case(item)) {
        return true;
    }
    path.components().any(|component| {
        let text = component.as_os_str().to_string_lossy();
        matches!(
            text.as_ref(),
            "Cookies" | "Messages" | "Mail" | "node_modules" | ".git"
        )
    })
}

/// Move one confirmed file to the Trash. Directories and system locations are refused.
pub fn trash_threat(home: &Path, path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|err| err.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("only a single file can be moved to the Trash".into());
    }
    if !threat_trash_allowed(home, path) {
        return Err("that file is outside the folders this scan is allowed to clean".into());
    }
    finder_trash(path)
}

fn threat_trash_allowed(home: &Path, path: &Path) -> bool {
    if skip_private(path) {
        return false;
    }
    let blocked = ["/System", "/usr", "/bin", "/sbin", "/Library", "/private"];
    if blocked.iter().any(|prefix| path.starts_with(prefix)) {
        return false;
    }
    path.starts_with(home) || path.starts_with("/Applications")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keeps_only_sha256_lines() {
        let text = "# comment\nnot a hash\nabc\n0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
        let set = parse_sha256_list(text);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn scan_matches_a_known_hash_and_skips_private_names() {
        let dir = std::env::temp_dir().join(format!("clean_your_mac_threat_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Cookies")).unwrap();
        fs::write(dir.join("sample.bin"), b"sable-threat-sample").unwrap();
        fs::write(dir.join("Cookies").join("Cookies.binarycookies"), b"nope").unwrap();
        let hash = sha256_file(&dir.join("sample.bin")).unwrap();
        let mut signatures = HashSet::new();
        signatures.insert(hash);
        let report = scan_threats(&dir, &signatures, &HashSet::new());
        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.hits[0].path, dir.join("sample.bin"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn trash_refuses_system_and_directories() {
        let home = Path::new("/Users/example");
        assert!(!threat_trash_allowed(home, Path::new("/System/Library/test")));
        assert!(!threat_trash_allowed(home, Path::new("/usr/bin/true")));
        assert!(threat_trash_allowed(home, Path::new("/Users/example/Downloads/a.dmg")));
    }
}
