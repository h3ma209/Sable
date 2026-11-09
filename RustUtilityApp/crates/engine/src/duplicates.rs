use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::hash::sha256_file;
use crate::space::shallow_dir_basename_match;

/// Only skip dependency/cache trees — unlike Space Lens we still walk build/, vendor/, Library/, etc.
const DUPLICATE_SKIP_DIR_NAMES: &[&str] = &["node_modules", ".git", "target", ".gradle", ".npm", ".pnpm-store"];

const MAX_FILES: usize = 120_000;
const MAX_DUPLICATE_GROUPS: usize = 150;
const MIN_BYTES: u64 = 64 * 1024;
const MAX_HASH_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub hash: String,
    pub bytes: u64,
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DuplicateReport {
    pub root: PathBuf,
    pub groups: Vec<DuplicateGroup>,
    pub scanned: u64,
    pub hashed: u64,
    pub skipped_dirs: u64,
    pub wasted_bytes: u64,
    /// Hit file scan cap before finishing the tree — narrow the folder or rescan after cleanup.
    pub cap_hit: bool,
}

/// Expand `~`, then canonicalize when possible so scans always walk the full directory tree.
pub fn resolve_duplicate_scan_root(input: &str) -> PathBuf {
    let trimmed = input.trim();
    let expanded = if trimmed == "~" {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(trimmed))
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        home.join(rest)
    } else {
        PathBuf::from(trimmed)
    };
    std::fs::canonicalize(&expanded).unwrap_or(expanded)
}

fn duplicate_skip_dir_names() -> Vec<String> {
    DUPLICATE_SKIP_DIR_NAMES
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

fn should_skip_dir(path: &Path) -> bool {
    shallow_dir_basename_match(path, &duplicate_skip_dir_names())
}

/// Recursively finds duplicate files under `root` (all descendants, not only the top level).
pub fn find_duplicates(root: &Path) -> DuplicateReport {
    let root = if root.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
    };
    let mut report = DuplicateReport {
        root: root.clone(),
        ..Default::default()
    };
    if !root.is_dir() {
        return report;
    }
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        if report.scanned as usize >= MAX_FILES {
            report.cap_hit = true;
            break;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if report.scanned as usize >= MAX_FILES {
                report.cap_hit = true;
                stack.clear();
                break;
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
                if should_skip_dir(&path) {
                    report.skipped_dirs += 1;
                    continue;
                }
                stack.push(path);
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let size = meta.len();
            report.scanned += 1;
            if size < MIN_BYTES {
                continue;
            }
            by_size.entry(size).or_default().push(path);
        }
    }

    let mut by_hash: HashMap<String, Vec<(PathBuf, u64)>> = HashMap::new();
    for (size, paths) in by_size {
        if paths.len() < 2 {
            continue;
        }
        for path in paths {
            if size > MAX_HASH_BYTES {
                continue;
            }
            let Ok(hash) = sha256_file(&path) else { continue };
            report.hashed += 1;
            by_hash
                .entry(hash)
                .or_default()
                .push((path, size));
        }
    }

    for (hash, items) in by_hash {
        if items.len() < 2 {
            continue;
        }
        let bytes = items.first().map(|(_, s)| *s).unwrap_or(0);
        let paths: Vec<PathBuf> = items.into_iter().map(|(p, _)| p).collect();
        let copies = paths.len().saturating_sub(1) as u64;
        report.wasted_bytes = report.wasted_bytes.saturating_add(bytes.saturating_mul(copies));
        report.groups.push(DuplicateGroup {
            hash,
            bytes,
            paths,
        });
    }
    report.groups.sort_by(|a, b| {
        let wa = a.bytes.saturating_mul(a.paths.len() as u64);
        let wb = b.bytes.saturating_mul(b.paths.len() as u64);
        wb.cmp(&wa)
    });
    report.groups.truncate(MAX_DUPLICATE_GROUPS);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_equal_files() {
        let dir = std::env::temp_dir().join(format!("sable_dup_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let payload = vec![b'x'; 70_000];
        fs::write(dir.join("a.bin"), &payload).unwrap();
        fs::write(dir.join("b.bin"), &payload).unwrap();
        let report = find_duplicates(&dir);
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].paths.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scans_inside_build_not_space_shallow_skip() {
        let dir = std::env::temp_dir().join(format!("sable_dup_build_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("build/release")).unwrap();
        let payload = vec![b'z'; 75_000];
        fs::write(dir.join("build/release/a.bin"), &payload).unwrap();
        fs::write(dir.join("build/release/b.bin"), &payload).unwrap();
        let report = find_duplicates(&dir);
        assert_eq!(report.groups.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_duplicates_in_nested_subfolders() {
        let dir = std::env::temp_dir().join(format!("sable_dup_nested_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Desktop")).unwrap();
        fs::create_dir_all(dir.join("Documents/deep")).unwrap();
        let payload = vec![b'y'; 80_000];
        fs::write(dir.join("Desktop/one.bin"), &payload).unwrap();
        fs::write(dir.join("Documents/deep/two.bin"), &payload).unwrap();
        let report = find_duplicates(&dir);
        assert_eq!(report.groups.len(), 1, "expected dup across nested paths");
        assert_eq!(report.groups[0].paths.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_tilde_home_root() {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        if home.as_os_str().is_empty() {
            return;
        }
        let resolved = resolve_duplicate_scan_root("~");
        assert_eq!(resolved, std::fs::canonicalize(&home).unwrap_or(home));
    }
}
