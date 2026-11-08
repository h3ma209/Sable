use std::path::Path;

use async_trait::async_trait;

use crate::error::ScanError;
use crate::event::Detection;

/// Per-file check. Returning `Ok(None)` means the path was clean.
/// Returning `Ok(Some(_))` is a hit. This crate does not ship signature rules.
#[async_trait]
pub trait Detector: Send + Sync {
    async fn check(&self, path: &Path) -> Result<Option<Detection>, ScanError>;
}

/// Reads size and extension and never raises a hit.
pub struct MetadataDetector;

#[async_trait]
impl Detector for MetadataDetector {
    async fn check(&self, path: &Path) -> Result<Option<Detection>, ScanError> {
        let _ = file_meta(path).await?;
        Ok(None)
    }
}

/// Raises a hit when the file extension is in `watch` and/or filename matches `names` (fuzzy / substring).
pub struct ExtensionWatchDetector {
    pub watch: Vec<String>,
    pub names: Vec<String>,
}

impl ExtensionWatchDetector {
    pub fn new(watch: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self::with_names(watch, std::iter::empty::<&str>())
    }

    pub fn with_names(
        watch: impl IntoIterator<Item = impl AsRef<str>>,
        names: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Self {
        Self {
            watch: watch
                .into_iter()
                .map(|s| s.as_ref().trim().trim_start_matches('.').to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            names: names
                .into_iter()
                .map(|s| s.as_ref().trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }
}

pub fn matches_pattern(filename: &str, pattern: &str) -> bool {
    let f = filename.to_ascii_lowercase();
    let p = pattern.to_ascii_lowercase();
    if f.contains(&p) {
        return true;
    }
    // Subsequence fuzzy match (e.g. "sbl" matches "sable-0.3.2.dmg")
    let mut p_chars = p.chars().peekable();
    for f_ch in f.chars() {
        if let Some(&target) = p_chars.peek() {
            if f_ch == target {
                p_chars.next();
            }
        } else {
            break;
        }
    }
    p_chars.peek().is_none()
}

#[async_trait]
impl Detector for ExtensionWatchDetector {
    async fn check(&self, path: &Path) -> Result<Option<Detection>, ScanError> {
        if self.watch.is_empty() && self.names.is_empty() {
            return Ok(None);
        }

        let (size, extension) = file_meta(path).await?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let ext_matches = if self.watch.is_empty() {
            true
        } else {
            self.watch.iter().any(|w| w == &extension)
        };

        let name_match = if self.names.is_empty() {
            None
        } else {
            self.names.iter().find(|&p| matches_pattern(name, p)).cloned()
        };

        let matched = match (!self.watch.is_empty(), !self.names.is_empty()) {
            (true, true) => ext_matches && name_match.is_some(),
            (true, false) => ext_matches,
            (false, true) => name_match.is_some(),
            (false, false) => false,
        };

        if matched {
            let note = match (ext_matches && !self.watch.is_empty(), name_match) {
                (true, Some(nm)) => format!("ext .{extension} · name match “{nm}”"),
                (true, None) => format!("ext .{extension}"),
                (false, Some(nm)) => format!("name match “{nm}”"),
                (false, None) => "match".to_string(),
            };
            Ok(Some(Detection {
                path: path.to_path_buf(),
                size,
                extension,
                note,
            }))
        } else {
            Ok(None)
        }
    }
}

pub async fn file_meta(path: &Path) -> Result<(u64, String), ScanError> {
    let meta = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|source| ScanError::io(path, source))?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok((meta.len(), extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("Sable-0.3.2.dmg", "sable"));
        assert!(matches_pattern("Sable-0.3.2.dmg", "SBL"));
        assert!(matches_pattern("photo_vacation.jpg", "vacation"));
        assert!(matches_pattern("photo_vacation.jpg", "pht"));
        assert!(!matches_pattern("photo_vacation.jpg", "xyz"));
    }

    #[tokio::test]
    async fn test_extension_watch_with_names() {
        let dir = std::env::temp_dir().join(format!("detect_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let file1 = dir.join("app_setup.dmg");
        let file2 = dir.join("notes.txt");
        let file3 = dir.join("installer.pkg");
        std::fs::write(&file1, b"dmg").unwrap();
        std::fs::write(&file2, b"txt").unwrap();
        std::fs::write(&file3, b"pkg").unwrap();

        // 1. Only ext
        let d_ext = ExtensionWatchDetector::new(["dmg"]);
        assert!(d_ext.check(&file1).await.unwrap().is_some());
        assert!(d_ext.check(&file2).await.unwrap().is_none());
        assert!(d_ext.check(&file3).await.unwrap().is_none());

        // 2. Only name (fuzzy)
        let d_name = ExtensionWatchDetector::with_names(Vec::<String>::new(), ["setup"]);
        assert!(d_name.check(&file1).await.unwrap().is_some());
        assert!(d_name.check(&file2).await.unwrap().is_none());

        // 3. Both ext and name
        let d_both = ExtensionWatchDetector::with_names(["dmg"], ["setup"]);
        assert!(d_both.check(&file1).await.unwrap().is_some());
        let file4 = dir.join("other.dmg");
        std::fs::write(&file4, b"other").unwrap();
        assert!(d_both.check(&file4).await.unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
