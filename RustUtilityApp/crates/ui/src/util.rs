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
        "Safari cache" => "Web pages Safari kept for faster loading.",
        "Mail downloads" => "Attachments Mail saved locally.",
        "pip cache" => "Downloaded Python packages. Pip fetches again when needed.",
        "npm cache" => "Cached npm packages for Node projects.",
        "Homebrew cache" => "Downloaded formulae bottles. Homebrew re-downloads on install.",
        "Xcode Archives" => "Old .ipa / archive exports from Xcode.",
        "Xcode iOS DeviceSupport" => "Symbols copied from devices you plugged in.",
        "CoreSimulator caches" => "iOS Simulator temp data.",
        "Cargo registry" => "Downloaded Rust crates. Cargo restores on build.",
        "Cargo git checkouts" => "Git dependencies Cargo cloned.",
        "Gradle caches" => "Android/Java build cache.",
        "Yarn cache" => "Yarn package tarballs.",
        "pnpm store" => "pnpm content-addressable package store.",
        "Go build cache" => "Compiled Go objects for faster rebuilds.",
        "Docker local data" => "Local Docker images and layer cache.",
        "Node modules (Projects)" => "Whole node_modules tree under ~/Projects if present.",
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

pub fn push_monitor_spark(buf: &mut Vec<f32>, value: f32) {
    buf.push(value.clamp(0.0, 1.0));
    const MAX: usize = 96;
    if buf.len() > MAX {
        buf.drain(0..buf.len() - MAX);
    }
}

pub fn monitor_chart_max(values: &[f32], floor: f32) -> f32 {
    values
        .iter()
        .cloned()
        .fold(floor, f32::max)
        .max(floor)
}

pub fn monitor_area_path(values: &[f32], width: f32, height: f32, max: f32) -> String {
    if values.is_empty() {
        return String::new();
    }
    let max = max.max(0.08);
    let step = width / (values.len().saturating_sub(1).max(1) as f32);
    let mut path = String::from("M 0 ");
    path.push_str(&format!("{:.1}", height));
    for (i, v) in values.iter().enumerate() {
        let x = i as f32 * step;
        let y = height - (v / max * height * 0.9 + height * 0.05);
        path.push_str(&format!(" L {x:.1} {y:.1}"));
    }
    let last_x = (values.len() - 1) as f32 * step;
    path.push_str(&format!(" L {last_x:.1} {height} L 0 {height} Z"));
    path
}

pub fn monitor_sparkline_points(values: &[f32], width: f32, height: f32) -> String {
    if values.is_empty() {
        return String::new();
    }
    let max = values
        .iter()
        .cloned()
        .fold(0.08f32, |a, b| a.max(b))
        .max(1.0);
    let step = width / (values.len().saturating_sub(1).max(1) as f32);
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = i as f32 * step;
            let y = height - (v / max * height * 0.92 + height * 0.04);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn monitor_ring_color(class: &str) -> &'static str {
    match class {
        "danger" => "#ff6b6b",
        "warn" => "#f5b942",
        _ => "#1ed760",
    }
}

pub fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86_400, (secs % 86_400) / 3600)
    }
}

/// 0–100 health score from CPU, memory, swap fractions (0–1 each).
pub fn monitor_health_score(cpu: f32, mem: f32, swap: f32) -> u8 {
    let stress = cpu * 0.45 + mem * 0.4 + swap * 0.15;
    ((1.0 - stress.clamp(0.0, 1.0)) * 100.0).round() as u8
}

pub fn monitor_health_label(score: u8) -> &'static str {
    match score {
        85..=100 => "Excellent",
        70..=84 => "Good",
        50..=69 => "Fair",
        30..=49 => "Strained",
        _ => "Critical",
    }
}

pub fn format_unix_time(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%b %d · %H:%M:%S").to_string())
        .unwrap_or_else(|| "—".to_string())
}

pub fn screen_kicker(screen: Screen) -> &'static str {
    match screen {
        Screen::Care => "Made for you",
        Screen::Cleanup
        | Screen::Clutter
        | Screen::Space
        | Screen::Cloud
        | Screen::Duplicates
        | Screen::Snapshots => "Library",
        Screen::Apps | Screen::Performance | Screen::Assistant => "Your Mac",
        Screen::Protection | Screen::Malware => "Safety",
    }
}
