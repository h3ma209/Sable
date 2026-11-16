use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub name: String,
    pub bundle_id: String,
    pub path: PathBuf,
    pub icon_data_url: Option<String>,
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

/// Fast count of installed `.app` bundles without parsing plists or decoding icons.
pub fn installed_app_count(home: &Path) -> usize {
    let mut count = 0usize;
    for dir in [PathBuf::from("/Applications"), home.join("Applications")] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if count >= 120 {
                return count;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("app")
                && path.join("Contents/Info.plist").is_file()
            {
                count += 1;
            }
        }
    }
    count
}

/// Reads `CFBundleName` and `CFBundleIdentifier` from application bundles.
pub fn installed_apps(home: &Path) -> Vec<InstalledApp> {
    let mut bundle_paths = Vec::new();
    for dir in [PathBuf::from("/Applications"), home.join("Applications")] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if bundle_paths.len() >= 120 {
                break;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("app")
                && path.join("Contents/Info.plist").is_file()
            {
                bundle_paths.push(path);
            }
        }
    }

    let workers = std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 8))
        .unwrap_or(4);
    let next_idx = std::sync::atomic::AtomicUsize::new(0);
    let results: std::sync::Mutex<Vec<InstalledApp>> = std::sync::Mutex::new(Vec::with_capacity(bundle_paths.len()));

    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                let mut local = Vec::new();
                loop {
                    let i = next_idx.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= bundle_paths.len() {
                        break;
                    }
                    let path = &bundle_paths[i];
                    let plist = path.join("Contents/Info.plist");
                    let plist_text = std::fs::read(&plist)
                        .ok()
                        .and_then(|b| if !b.starts_with(b"bplist") { String::from_utf8(b).ok() } else { None });
                    let read_key = |key: &str| -> Option<String> {
                        if let Some(ref xml) = plist_text {
                            xml_plist_string(xml, key)
                        } else {
                            plist_raw(&plist, key)
                        }
                    };
                    let name = read_key("CFBundleName")
                        .or_else(|| read_key("CFBundleDisplayName"))
                        .unwrap_or_else(|| {
                            path.file_stem()
                                .and_then(|name| name.to_str())
                                .unwrap_or("App")
                                .to_string()
                        });
                    let bundle_id = read_key("CFBundleIdentifier").unwrap_or_default();
                    let icon_data_url = resolve_app_icon_with(path, &plist, &name, &read_key);
                    local.push(InstalledApp {
                        name,
                        bundle_id,
                        path: path.clone(),
                        icon_data_url,
                    });
                }
                if let Ok(mut guard) = results.lock() {
                    guard.extend(local);
                }
            });
        }
    });

    let mut apps = results.into_inner().unwrap_or_default();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

fn xml_plist_string(xml: &str, key: &str) -> Option<String> {
    let needle = format!("<key>{key}</key>");
    let idx = xml.find(&needle)?;
    let rest = &xml[idx + needle.len()..];
    let s_start = rest.find("<string>")?;
    // Ensure we didn't cross into the next <key>
    if let Some(next_key) = rest.find("<key>") {
        if next_key < s_start {
            return None;
        }
    }
    let after_open = &rest[s_start + "<string>".len()..];
    let s_end = after_open.find("</string>")?;
    let val = after_open[..s_end].trim();
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

fn resolve_app_icon_with(
    app_path: &Path,
    _plist: &Path,
    app_name: &str,
    read_key: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let resources = app_path.join("Contents/Resources");
    let mut candidates = Vec::new();

    if let Some(icon_name) = read_key("CFBundleIconFile") {
        candidates.push(resources.join(&icon_name));
        if !icon_name.ends_with(".icns") {
            candidates.push(resources.join(format!("{icon_name}.icns")));
        }
    }
    if let Some(icon_name) = read_key("CFBundleIconName") {
        candidates.push(resources.join(&icon_name));
        if !icon_name.ends_with(".icns") {
            candidates.push(resources.join(format!("{icon_name}.icns")));
        }
    }

    candidates.push(resources.join("AppIcon.icns"));
    candidates.push(resources.join(format!("{app_name}.icns")));
    candidates.push(resources.join("app.icns"));
    candidates.push(resources.join("icon.icns"));

    for candidate in &candidates {
        if candidate.is_file() {
            if let Some(data_url) = extract_icns_png(candidate) {
                return Some(data_url);
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(&resources) {
        let mut fallback_icns: Option<PathBuf> = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("icns") {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if stem.contains("icon") || stem.contains(&app_name.to_lowercase()) {
                    if let Some(data_url) = extract_icns_png(&path) {
                        return Some(data_url);
                    }
                }
                if fallback_icns.is_none() {
                    fallback_icns = Some(path);
                }
            }
        }
        if let Some(fallback) = fallback_icns {
            if let Some(data_url) = extract_icns_png(&fallback) {
                return Some(data_url);
            }
        }
    }

    None
}

fn extract_icns_png(icns_path: &Path) -> Option<String> {
    let bytes = std::fs::read(icns_path).ok()?;
    if bytes.len() < 8 || &bytes[0..4] != b"icns" {
        return None;
    }

    let mut offset = 8;
    let mut best_score: i32 = -1;
    let mut best_slice: Option<&[u8]> = None;

    while offset + 8 <= bytes.len() {
        let tag = &bytes[offset..offset + 4];
        let clen = match bytes[offset + 4..offset + 8].try_into() {
            Ok(arr) => u32::from_be_bytes(arr) as usize,
            Err(_) => break,
        };
        if clen < 8 || offset + clen > bytes.len() {
            break;
        }
        let chunk_data = &bytes[offset + 8..offset + clen];
        if chunk_data.starts_with(b"\x89PNG\r\n\x1a\n") {
            let score = match tag {
                b"ic12" => 100, // 32x32@2x (64x64)
                b"ic07" => 95,  // 128x128
                b"icp6" => 90,  // 64x64
                b"ic13" => 85,  // 128x128@2x (256x256)
                b"ic08" => 80,  // 256x256
                b"ic11" => 70,  // 16x16@2x (32x32)
                b"icp5" => 60,  // 32x32
                b"ic14" => 50,  // 256x256@2x (512x512)
                b"ic09" => 40,  // 512x512
                b"ic10" => 30,  // 1024x1024
                _ => 20,
            };
            if score > best_score {
                best_score = score;
                best_slice = Some(chunk_data);
            }
        }
        offset += clen;
    }

    if let Some(png) = best_slice {
        return Some(format!("data:image/png;base64,{}", base64_encode(png)));
    }

    convert_icns_with_sips(icns_path)
}

fn convert_icns_with_sips(icns_path: &Path) -> Option<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("sable_icon_{}_{}.png", std::process::id(), now));
    let status = Command::new("sips")
        .args(["-s", "format", "png", "-Z", "128"])
        .arg(icns_path)
        .args(["--out"])
        .arg(&tmp)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    let png_bytes = std::fs::read(&tmp).ok();
    let _ = std::fs::remove_file(&tmp);
    let bytes = png_bytes?;
    Some(format!("data:image/png;base64,{}", base64_encode(&bytes)))
}

pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 3) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_installed_apps_loads_icons() {
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()));
        let apps = installed_apps(&home);
        if !apps.is_empty() {
            let with_icons = apps.iter().filter(|a| a.icon_data_url.is_some()).count();
            assert!(with_icons > 0, "Expected at least one installed app to have an icon");
            for app in apps.iter().filter(|a| a.icon_data_url.is_some()) {
                let url = app.icon_data_url.as_ref().unwrap();
                assert!(url.starts_with("data:image/png;base64,"));
            }
        }
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
