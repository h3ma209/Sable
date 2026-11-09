use std::path::Path;
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSnapshot {
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnapshotReport {
    pub volume: String,
    pub snapshots: Vec<LocalSnapshot>,
    pub thin_hint: String,
}

/// Lists APFS / Time Machine local snapshots for the boot volume. Read-only.
pub fn list_local_snapshots(volume: &Path) -> SnapshotReport {
    let volume = volume.display().to_string();
    let mut report = SnapshotReport {
        volume: volume.clone(),
        thin_hint: "Free space: run `tmutil thinlocalsnapshots / 999999999999 1` in Terminal (may need admin)."
            .into(),
        ..Default::default()
    };
    let output = Command::new("tmutil").args(["listlocalsnapshots", &volume]).output();
    let Ok(output) = output else {
        report.thin_hint = "Could not run tmutil. Local snapshots listing needs macOS.".into();
        return report;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.contains("Snapshots for") {
            continue;
        }
        if line.contains(".local") || line.starts_with("com.apple") {
            report.snapshots.push(LocalSnapshot {
                name: line.to_string(),
            });
        }
    }
    if report.snapshots.is_empty() && !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !err.is_empty() {
            report.thin_hint = err;
        }
    }
    report
}

#[cfg(test)]
pub fn parse_snapshot_lines(text: &str) -> Vec<LocalSnapshot> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.contains(".local") || (line.starts_with("com.apple") && line.contains("TimeMachine")) {
            out.push(LocalSnapshot {
                name: line.to_string(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tmutil_output() {
        let text = "Snapshots for disk3s1s1 (2 found)\ncom.apple.TimeMachine.2024-11-01-120000.local\ncom.apple.os.update-ABC.local\n";
        let snaps = parse_snapshot_lines(text);
        assert_eq!(snaps.len(), 2);
    }
}
