use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::hash::sha256_file;

const MAX_FILES: usize = 25_000;
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
}

/// Size-bucket then SHA-256. Skips symlinks and tiny files.
pub fn find_duplicates(root: &Path) -> DuplicateReport {
    let mut report = DuplicateReport {
        root: root.to_path_buf(),
        ..Default::default()
    };
    if !root.is_dir() {
        return report;
    }
    let default_skip = [
        "node_modules",
        ".git",
        "target",
        ".gradle",
        ".npm",
        "Library/Caches",
    ];
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if report.scanned as usize >= MAX_FILES {
                break;
            }
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if default_skip.iter().any(|s| name == *s || path.to_string_lossy().contains(s)) {
                    report.skipped_dirs += 1;
                    continue;
                }
                stack.push(path);
                continue;
            }
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if meta.file_type().is_symlink() || !meta.is_file() {
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
    report.groups.truncate(80);
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
}
