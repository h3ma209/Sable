use std::path::{Path, PathBuf};
use std::process::Command;

use crate::state::{Notice, Screen};

pub fn describe_junk(label: &str) -> &'static str {
    match label {
        "User caches" => "Temporary files apps keep so they open faster. Rebuilt automatically.",
        "User logs" => "Activity notes apps write for troubleshooting. Not needed day to day.",
        "Diagnostic reports" => "Crash reports macOS saves after an app quits unexpectedly.",
        "Spotify cache" => "Songs Spotify stored for faster playback. It downloads them again if needed.",
        "Adobe media cache" => "Preview files from Adobe video and audio apps. Rebuilt on demand.",
        "Xcode derived data" => "Build files from Xcode. The next build recreates them.",
        _ => "Temporary files that apps can recreate.",
    }
}

pub fn outcome(result: Result<String, String>, fallback: &str) -> Notice {
    match result {
        Ok(text) => Notice {
            ok: true,
            text: if text.trim().is_empty() {
                fallback.to_string()
            } else {
                text.trim().to_string()
            },
            refresh: None,
        },
        Err(text) => Notice {
            ok: false,
            text,
            refresh: None,
        },
    }
}

pub fn reveal(path: &Path) {
    let _ = Command::new("open").arg("-R").arg(path).spawn();
}

#[allow(dead_code)]
pub fn open_url(url: &str) {
    let _ = Command::new("open").arg(url).spawn();
}

pub fn normalize_space_path(path: &str) -> String {
    let trimmed = path.trim();
    let candidate = PathBuf::from(trimmed);
    std::fs::canonicalize(&candidate)
        .map(|resolved| resolved.display().to_string())
        .unwrap_or_else(|_| trimmed.to_string())
}


pub fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("item")
        .to_string()
}

pub fn pretty(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

pub fn format_count(n: u64) -> String {
    let raw = n.to_string();
    let mut out = String::new();
    for (i, ch) in raw.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn scan_age(at: Option<std::time::Instant>) -> String {
    let Some(at) = at else {
        return "never".into();
    };
    let secs = at.elapsed().as_secs();
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

pub fn gauge_class(fraction: f32) -> &'static str {
    if fraction > 0.85 {
        "danger"
    } else if fraction > 0.65 {
        "warn"
    } else {
        ""
    }
}

pub fn is_dev_artifact(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        "node_modules"
            | "target"
            | ".gradle"
            | ".venv"
            | "venv"
            | ".cargo"
            | ".next"
            | ".turbo"
            | "build"
            | "deriveddata"
            | ".docker"
            | ".rustup"
            | ".pnpm-store"
            | ".npm"
            | ".m2"
    )
}

pub fn space_category_style(is_dir: bool, name: &str) -> (&'static str, &'static str) {
    if is_dev_artifact(name) {
        return ("#f59e0b", "Dev Artifact"); // Amber
    }
    if is_dir {
        return ("#10b981", "Folder"); // Emerald
    }
    let lower = name.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "app" | "dmg" | "pkg" | "iso" => ("#8b5cf6", "App / Installer"), // Purple
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => ("#3b82f6", "Archive"), // Blue
        "mp4" | "mov" | "mkv" | "avi" | "mp3" | "wav" | "flac" | "m4a" => ("#ec4899", "Media"), // Pink
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "heic" | "psd" => ("#06b6d4", "Image"), // Cyan
        "rs" | "js" | "ts" | "py" | "go" | "c" | "cpp" | "h" | "json" | "toml" | "yaml" | "yml" => {
            ("#10b981", "Code") // Green
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" => {
            ("#64748b", "Document") // Slate
        }
        _ => ("#94a3b8", "File"),
    }
}

pub fn screen_kicker(screen: Screen) -> &'static str {
    match screen {
        Screen::Care => "Made for you",
        Screen::Cleanup | Screen::Clutter | Screen::Space | Screen::Cloud => "Library",
        Screen::Apps | Screen::Performance => "Your Mac",
        Screen::Protection | Screen::Malware => "Safety",
        Screen::Assistant => "Now playing",
    }
}
