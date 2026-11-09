//! Chunked file walk and per-file checks.
//!
//! The pipeline follows the shape of a scan task: enumerate, chunk, check, report.
//! Detection rules are injected. Nothing in this crate deletes files.

mod access;
mod apps;
mod care;
mod cleanup;
mod cloud;
mod clutter;
mod control;
mod detect;
mod duplicates;
mod enumerate;
mod error;
mod event;
mod hash;
mod performance;
mod remove;
mod scan;
mod snapshots;
mod space;
mod telemetry;
mod threats;

pub use access::{folder_access, AccessStatus};
pub use apps::{installed_apps, survey_leftovers, InstalledApp, LeftoverHit};
pub use care::{run_care, CareReport, CareState, ModuleResult};
pub use cleanup::{
    dev_junk_roots, directory_size, general_junk_candidates, junk_candidates, junk_roots,
    move_download_to_trash, move_root_to_trash, JunkRoot,
};
pub use duplicates::{find_duplicates, resolve_duplicate_scan_root, DuplicateGroup, DuplicateReport};
pub use cloud::{local_cloud_folders, CloudFolder};
pub use clutter::{large_downloads, LargeFile};
pub use control::ExecutionControl;
pub use detect::{Detector, ExtensionWatchDetector, MetadataDetector};
pub use enumerate::{walk, Chunk, WalkOptions};
pub use error::ScanError;
pub use event::{Detection, ScanEvent};
pub use performance::{
    flush_dns_cache, launch_agents, machine_sample, reindex_spotlight, spotlight_status, LaunchAgent,
    MachineSample, ProcessSnap, VolumeBytes,
};
pub use snapshots::{list_local_snapshots, LocalSnapshot, SnapshotReport};
pub use telemetry::{
    HostPoint, MonitorAnalytics, MonitorBucket, ProcessAggregate, ProcessPoint, TelemetryStore,
    TelemetrySummary,
};
pub use remove::{RemovalPlan, RemoveTask};
pub use scan::{run_scan, ScanConfig};
pub use space::{
    cloud_status, computer_root, deletion_units, home_folder_access, is_spared, normalize_space_path,
    effective_shallow_dir_names, parse_shallow_dir_names, scan_space, scan_space_volume,
    scan_space_volume_with_options, scan_space_volume_with_progress, shallow_dir_basename_match,
    shallow_dir_name_match, space_entry_size_only, DEFAULT_SPACE_SHALLOW_DIR_NAMES,
    space_folder, space_report_at, trash_selected, volume_contains, CloudScannerStrategy, CloudStatus,
    DeletionUnit, SpaceEntity, SpaceFolder,
    SpaceNode, SpaceProgress, SpaceReport, SpaceVolumeScan, SPACE_LENS_BUBBLE_THRESHOLD,
};
pub use threats::{prepare_and_scan, remember_ignore, trash_threat, ThreatHit, ThreatReport};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc;

    use super::*;

    #[tokio::test]
    async fn cancel_unblocks_checkpoint() {
        let control = ExecutionControl::new();
        control.pause();
        control.cancel();
        let err = control.checkpoint().await.unwrap_err();
        assert!(matches!(err, ScanError::Canceled));
    }

    #[tokio::test]
    async fn walk_chunks_files_and_marks_finale() {
        let dir = std::env::temp_dir().join(format!("rust_utility_walk_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for name in ["a.txt", "b.txt", "sub/c.txt", "sub/d.txt", "sub/e.dat"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let control = ExecutionControl::new();
        let (tx, mut rx) = mpsc::channel(8);
        walk(WalkOptions::new(&dir, 2), &control, tx).await.unwrap();
        let mut chunks = Vec::new();
        while let Some(chunk) = rx.recv().await {
            chunks.push(chunk);
        }
        let files: usize = chunks.iter().map(|c| c.paths.len()).sum();
        assert_eq!(files, 5);
        assert!(chunks.last().unwrap().finale);
        assert!(chunks.iter().rev().skip(1).all(|c| !c.finale));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scan_reports_extension_hits_and_removal_is_a_plan() {
        let dir = std::env::temp_dir().join(format!("rust_utility_scan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ok.txt"), b"ok").unwrap();
        std::fs::write(dir.join("note.dmg"), b"no").unwrap();

        let control = Arc::new(ExecutionControl::new());
        let detector = Arc::new(ExtensionWatchDetector::new(["dmg"]));
        let (tx, mut rx) = mpsc::channel(32);
        let config = ScanConfig {
            chunk_size: 1,
            check_concurrency: 2,
            ..ScanConfig::new(&dir)
        };
        run_scan(config, detector, control, tx).await;

        let mut hits = Vec::new();
        let mut finished = None;
        while let Some(event) = rx.recv().await {
            match event {
                ScanEvent::Detection(hit) => hits.push(hit),
                ScanEvent::Finished {
                    scanned,
                    detections,
                    errors,
                } => finished = Some((scanned, detections, errors)),
                _ => {}
            }
        }
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("note.dmg"));
        assert_eq!(finished, Some((2, 1, 0)));

        let plan = RemoveTask::plan(&hits);
        assert!(!plan.executed);
        assert!(dir.join("note.dmg").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scan_respects_exclude_dirs() {
        let dir = std::env::temp_dir().join(format!("rust_utility_exclude_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let nm = dir.join("node_modules");
        let gradle = dir.join(".gradle");
        let normal = dir.join("src");
        std::fs::create_dir_all(&nm).unwrap();
        std::fs::create_dir_all(&gradle).unwrap();
        std::fs::create_dir_all(&normal).unwrap();

        std::fs::write(nm.join("pkg.dmg"), b"no").unwrap();
        std::fs::write(gradle.join("cache.dmg"), b"no").unwrap();
        std::fs::write(normal.join("valid.dmg"), b"yes").unwrap();

        let control = Arc::new(ExecutionControl::new());
        let detector = Arc::new(ExtensionWatchDetector::new(["dmg"]));
        let (tx, mut rx) = mpsc::channel(32);
        let config = ScanConfig {
            chunk_size: 1,
            check_concurrency: 2,
            exclude_dirs: vec!["node_modules".into(), ".gradle".into()],
            ..ScanConfig::new(&dir)
        };
        run_scan(config, detector, control, tx).await;

        let mut hits = Vec::new();
        while let Some(event) = rx.recv().await {
            if let ScanEvent::Detection(hit) = event {
                hits.push(hit);
            }
        }
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("valid.dmg"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trash_refuses_paths_outside_cache_roots() {
        let err = move_root_to_trash(std::path::Path::new("/tmp"), std::path::Path::new("/etc")).unwrap_err();
        assert!(err.contains("surveyed"));
    }

    #[test]
    fn space_spares_directories_and_system_paths() {
        let home = std::path::Path::new("/Users/example");
        assert!(is_spared(std::path::Path::new("/System/Library"), home));
        assert!(is_spared(home.join("Library/Caches").as_path(), home));
    }

    #[test]
    fn download_trash_refuses_the_downloads_folder() {
        let err = move_download_to_trash(std::path::Path::new("/tmp"), std::path::Path::new("/tmp/Downloads")).unwrap_err();
        assert!(err.contains("Downloads"));
    }

    #[test]
    fn leftover_survey_matches_bundle_id() {
        let home = std::env::temp_dir().join(format!("rust_utility_left_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let prefs = home.join("Library/Preferences");
        std::fs::create_dir_all(&prefs).unwrap();
        std::fs::write(prefs.join("com.example.demo.plist"), b"x").unwrap();
        let hits = survey_leftovers(&home, "com.example.demo");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("com.example.demo.plist"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn directory_size_sums_files() {
        let dir = std::env::temp_dir().join(format!("rust_utility_size_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("a.txt"), b"abcd").unwrap();
        std::fs::write(dir.join("nested/b.txt"), b"xy").unwrap();
        assert_eq!(directory_size(&dir), 6);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
