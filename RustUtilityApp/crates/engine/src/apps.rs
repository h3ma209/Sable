use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub name: String,
    pub bundle_id: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftoverHit {
    pub root: String,
    pub path: PathBuf,
}

const LEFTOVER_ROOTS: &[(&str, &str)] = &[
    ("Application Support", "Library/Application Support"),
    ("Preferences", "Library/Preferences"),
    ("Saved Application State", "Library/Saved Application State"),
    ("Caches", "Library/Caches"),
    ("Logs", "Library/Logs"),
    ("Containers", "Library/Containers"),
    ("Group Containers", "Library/Group Containers"),
    ("HTTPStorages", "Library/HTTPStorages"),
    ("WebKit", "Library/WebKit"),
];

/// Counts files whose names contain `bundle_id` under the library roots named in the applications spec.
/// Read-only. Does not open cookie or password stores.
pub fn survey_leftovers(home: &Path, bundle_id: &str) -> Vec<LeftoverHit> {
    let bundle_id = bundle_id.trim();
    if bundle_id.len() < 3 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (label, relative) in LEFTOVER_ROOTS {
        walk_leftovers(&home.join(relative), label, bundle_id, 0, &mut hits);
        if hits.len() >= 40 {
            break;
        }
    }
    hits
}

fn walk_leftovers(dir: &Path, label: &str, bundle_id: &str, depth: usize, hits: &mut Vec<LeftoverHit>) {
    if depth > 2 || hits.len() >= 40 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for (seen, entry) in entries.flatten().enumerate() {
        if seen > 400 || hits.len() >= 40 {
            return;
        }
        let path = entry.path();
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
        if name.contains(bundle_id) {
            hits.push(LeftoverHit {
                root: label.to_string(),
                path: path.clone(),
            });
        }
        if path.is_dir() && !path.symlink_metadata().is_ok_and(|meta| meta.file_type().is_symlink()) {
            walk_leftovers(&path, label, bundle_id, depth + 1, hits);
        }
    }
}

/// Reads `CFBundleName` and `CFBundleIdentifier` from application bundles.
pub fn installed_apps(home: &Path) -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    for dir in [PathBuf::from("/Applications"), home.join("Applications")] {
        collect_apps(&dir, &mut apps);
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

fn collect_apps(dir: &Path, apps: &mut Vec<InstalledApp>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if apps.len() >= 120 {
            return;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("app") {
            continue;
        }
        let plist = path.join("Contents/Info.plist");
        if !plist.is_file() {
            continue;
        }
        let name = plist_raw(&plist, "CFBundleName")
            .or_else(|| plist_raw(&plist, "CFBundleDisplayName"))
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("App")
                    .to_string()
            });
        let bundle_id = plist_raw(&plist, "CFBundleIdentifier").unwrap_or_default();
        apps.push(InstalledApp {
            name,
            bundle_id,
            path,
        });
    }
}

fn plist_raw(path: &Path, key: &str) -> Option<String> {
    let output = Command::new("plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}
