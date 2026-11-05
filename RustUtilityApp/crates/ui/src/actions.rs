use std::path::{Path, PathBuf};
use std::sync::Arc;

use dioxus::prelude::*;
use engine::{
    flush_dns_cache, folder_access, junk_roots, large_downloads, launch_agents, local_cloud_folders,
    machine_sample, move_download_to_trash, move_root_to_trash, prepare_and_scan, reindex_spotlight,
    remember_ignore, run_care, run_scan, scan_space_volume, space_report_at, spotlight_status,
    survey_leftovers, trash_selected, trash_threat as engine_trash_threat, volume_contains,
    computer_root, AccessStatus,
    ExecutionControl, ExtensionWatchDetector, MachineSample, ScanConfig, ScanEvent, SpaceReport,
};
use tokio::sync::mpsc;

use crate::state::{AppState, ConfirmKind, Notice, Screen, Tool};
use crate::util::{file_name, normalize_space_path, outcome};

pub fn arm(state: &mut AppState) {
    state.busy = true;
}

pub fn load(mut state: Signal<AppState>, screen: Screen) {
    state.write().loaded.insert(screen);
    match screen {
        Screen::Care => start_care(state),
        Screen::Cleanup => start_junk(state),
        Screen::Apps => load_apps(state),
        Screen::Performance => load_agents(state),
        Screen::Clutter => start_clutter(state),
        Screen::Cloud => start_cloud(state),
        Screen::Assistant => sample_machine(state),
        Screen::Protection | Screen::Malware | Screen::Space => {}
    }
}

pub fn ensure_auto_load(state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let screen = state.read().screen;
    for s in [screen, Screen::Care, Screen::Cleanup] {
        if s.auto_loads() && !state.read().loaded.contains(&s) {
            load(state, s);
            break;
        }
    }
}

pub fn finish_and_pump(mut state: Signal<AppState>) {
    state.write().finish();
    pump_care_queue(state);
}

#[allow(dead_code)]
pub fn dock_run(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let screen = state.read().screen;
    match screen {
        Screen::Care => run_checked_care(state),
        Screen::Clutter => {
            state.write().care_followups.clear();
            state.write().loaded.insert(Screen::Clutter);
            state.write().clutter.clear();
            start_clutter(state);
        }
        Screen::Apps => {
            state.write().care_followups.clear();
            load(state, Screen::Apps);
        }
        Screen::Performance => {
            state.write().care_followups.clear();
            load(state, Screen::Performance);
        }
        Screen::Space => {
            state.write().care_followups.clear();
            start_space(state, false);
        }
        _ => {}
    }
}

pub fn run_checked_care(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let picks = state.read().care_picks.clone();
    let threats_clear = state
        .read()
        .threat
        .as_ref()
        .is_some_and(|report| report.hits.is_empty());
    let mut jobs = Vec::new();
    if picks.contains(&Screen::Cleanup) {
        jobs.push(Screen::Cleanup);
    }
    if picks.contains(&Screen::Performance) {
        jobs.push(Screen::Performance);
    }
    if picks.contains(&Screen::Clutter) {
        jobs.push(Screen::Clutter);
    }
    if picks.contains(&Screen::Malware) && !threats_clear {
        jobs.push(Screen::Malware);
    }
    if jobs.is_empty() {
        state.write().notice = Some(Notice {
            ok: false,
            text: "Select at least one task.".into(),
            refresh: None,
        });
        return;
    }
    state.write().care_followups = jobs;
    pump_care_queue(state);
}

fn pump_care_queue(mut state: Signal<AppState>) {
    while !state.read().care_followups.is_empty() {
        let next = state.write().care_followups.remove(0);
        start_care_job(state, next);
        if state.read().busy {
            return;
        }
    }
}

fn start_care_job(mut state: Signal<AppState>, screen: Screen) {
    match screen {
        Screen::Cleanup => start_junk(state),
        Screen::Performance => load_agents(state),
        Screen::Apps => load_apps(state),
        Screen::Clutter => {
            state.write().loaded.insert(Screen::Clutter);
            state.write().clutter.clear();
            start_clutter(state);
        }
        Screen::Malware => start_threat(state),
        _ => {}
    }
}

pub fn start_care(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "scanning";
    spawn(async move {
        let report = run_care(home).await;
        {
            let mut s = state.write();
            s.access = report.access;
            s.care = Some(report);
        }
        finish_and_pump(state);
    });
}

pub fn start_junk(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "scanning";
    spawn(async move {
        let roots = tokio::task::spawn_blocking(move || junk_roots(&home))
            .await
            .unwrap_or_default();
        state.write().junk = roots;
        finish_and_pump(state);
    });
}

pub fn trash(mut state: Signal<AppState>, path: PathBuf) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "cleaning";
    spawn(async move {
        let notice = match move_root_to_trash(&home, &path) {
            Ok(()) => Notice {
                ok: true,
                text: format!(
                    "Moved \"{}\" to the Trash. Open the Trash to put it back.",
                    file_name(&path)
                ),
                refresh: Some(Screen::Cleanup),
            },
            Err(err) => Notice {
                ok: false,
                text: err,
                refresh: None,
            },
        };
        apply_notice(state, notice);
    });
}

pub fn trash_roots(mut state: Signal<AppState>, paths: Vec<PathBuf>) {
    if state.read().busy || paths.is_empty() {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "cleaning";
    spawn(async move {
        let mut count = 0;
        let mut last_err = None;
        for path in paths {
            match move_root_to_trash(&home, &path) {
                Ok(()) => count += 1,
                Err(e) => last_err = Some(e),
            }
        }
        let notice = if count > 0 {
            Notice {
                ok: true,
                text: format!("Moved {count} folders to the Trash. Open Trash to put them back."),
                refresh: Some(Screen::Cleanup),
            }
        } else {
            Notice {
                ok: false,
                text: last_err.unwrap_or_else(|| "Failed to move folders to Trash.".into()),
                refresh: None,
            }
        };
        apply_notice(state, notice);
    });
}

pub fn load_agents(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "scanning";
    spawn(async move {
        let agents = tokio::task::spawn_blocking(move || launch_agents(&home))
            .await
            .unwrap_or_default();
        state.write().agents = agents;
        finish_and_pump(state);
    });
}

pub fn run_tool(mut state: Signal<AppState>, kind: Tool) {
    if state.read().busy {
        return;
    }
    arm(&mut state.write());
    state.write().status = "working";
    spawn(async move {
        let notice = match kind {
            Tool::Dns => outcome(flush_dns_cache(), "DNS cache cleared."),
            Tool::SpotlightStatus => outcome(spotlight_status(), "Spotlight status unavailable."),
            Tool::Reindex => outcome(reindex_spotlight(), "Spotlight is rebuilding its index."),
        };
        apply_notice(state, notice);
    });
}

pub fn start_clutter(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "scanning";
    spawn(async move {
        let files = tokio::task::spawn_blocking(move || large_downloads(&home, 50 * 1024 * 1024))
            .await
            .unwrap_or_default();
        state.write().clutter = files;
        finish_and_pump(state);
    });
}

pub fn trash_download(mut state: Signal<AppState>, path: PathBuf) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "cleaning";
    spawn(async move {
        let notice = match move_download_to_trash(&home, &path) {
            Ok(()) => Notice {
                ok: true,
                text: format!(
                    "Moved \"{}\" to the Trash. Open the Trash to put it back.",
                    file_name(&path)
                ),
                refresh: Some(Screen::Clutter),
            },
            Err(err) => Notice {
                ok: false,
                text: err,
                refresh: None,
            },
        };
        apply_notice(state, notice);
    });
}

pub fn start_cloud(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "scanning";
    spawn(async move {
        let folders = tokio::task::spawn_blocking(move || local_cloud_folders(&home))
            .await
            .unwrap_or_default();
        state.write().cloud = folders;
        finish_and_pump(state);
    });
}

pub fn sample_machine(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    arm(&mut state.write());
    state.write().status = "scanning";
    spawn(async move {
        let sample = tokio::task::spawn_blocking(machine_sample)
            .await
            .unwrap_or(MachineSample {
                used_memory: 0,
                total_memory: 0,
                cpu_percent: 0.0,
            });
        state.write().sample = Some(sample);
        finish_and_pump(state);
    });
}

pub fn survey_selected(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let (home, bundle_id) = {
        let s = state.read();
        let Some(index) = s.selected_app else {
            return;
        };
        let Some(app) = s.apps.get(index) else {
            return;
        };
        (s.home.clone(), app.bundle_id.clone())
    };
    arm(&mut state.write());
    state.write().status = "scanning";
    spawn(async move {
        let hits = tokio::task::spawn_blocking(move || survey_leftovers(&home, &bundle_id))
            .await
            .unwrap_or_default();
        let mut s = state.write();
        s.leftovers = hits;
        s.leftovers_done = true;
        drop(s);
        finish_and_pump(state);
    });
}

pub fn load_apps(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "scanning";
    spawn(async move {
        let apps = tokio::task::spawn_blocking(move || engine::installed_apps(&home))
            .await
            .unwrap_or_default();
        {
            let mut s = state.write();
            s.apps = apps;
            s.selected_app = None;
            s.leftovers.clear();
            s.leftovers_done = false;
        }
        finish_and_pump(state);
    });
}

pub fn trash_space(mut state: Signal<AppState>, paths: Vec<PathBuf>) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    let root = PathBuf::from(state.read().space_root.trim());
    arm(&mut state.write());
    state.write().status = "cleaning";
    spawn(async move {
        let notice = match trash_selected(&root, &home, &paths) {
            Ok(count) => Notice {
                ok: true,
                text: format!(
                    "Moved {count} files to the Trash. Open the Trash to put them back."
                ),
                refresh: Some(Screen::Space),
            },
            Err(err) => Notice {
                ok: false,
                text: err,
                refresh: None,
            },
        };
        apply_notice(state, notice);
    });
}

pub fn start_threat(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    let root = PathBuf::from(state.read().threat_root.trim());
    if !root.is_dir() {
        state.write().notice = Some(Notice {
            ok: false,
            text: "Choose a folder to scan.".into(),
            refresh: None,
        });
        return;
    }
    state.write().threat = None;
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "scanning";
    spawn(async move {
        let result = tokio::task::spawn_blocking(move || prepare_and_scan(&home, &root)).await;
        match result {
            Ok(Ok(report)) => state.write().threat = Some(report),
            Ok(Err(text)) => {
                state.write().notice = Some(Notice {
                    ok: false,
                    text,
                    refresh: None,
                });
            }
            Err(_) => {
                state.write().notice = Some(Notice {
                    ok: false,
                    text: "the malware scan stopped before it finished".into(),
                    refresh: None,
                });
            }
        }
        finish_and_pump(state);
    });
}

pub fn trash_threat(mut state: Signal<AppState>, path: PathBuf) {
    if state.read().busy {
        return;
    }
    let home = state.read().home.clone();
    arm(&mut state.write());
    state.write().status = "cleaning";
    spawn(async move {
        let notice = match engine_trash_threat(&home, &path) {
            Ok(()) => Notice {
                ok: true,
                text: format!(
                    "Moved {} to the Trash. Open the Trash to put it back.",
                    path.display()
                ),
                refresh: Some(Screen::Malware),
            },
            Err(err) => Notice {
                ok: false,
                text: err,
                refresh: None,
            },
        };
        apply_notice(state, notice);
    });
}

pub fn ignore_threat(mut state: Signal<AppState>, path: PathBuf) {
    let home = state.read().home.clone();
    if let Err(err) = remember_ignore(&home, &path) {
        state.write().notice = Some(Notice {
            ok: false,
            text: err,
            refresh: None,
        });
        return;
    }
    if let Some(report) = state.write().threat.as_mut() {
        report.hits.retain(|hit| hit.path != path);
    }
    state.write().notice = Some(Notice {
        ok: true,
        text: "That file will be skipped on later scans.".into(),
        refresh: None,
    });
}

pub fn start_space(mut state: Signal<AppState>, force_rescan: bool) {
    let trimmed = state.read().space_root.trim().to_string();
    if trimmed.is_empty() {
        state.write().notice = Some(Notice {
            ok: false,
            text: "Choose a location first — try “This Mac” or Home.".into(),
            refresh: None,
        });
        return;
    }
    let folder = PathBuf::from(&trimmed);
    if !force_rescan {
        let cached = {
            let s = state.read();
            s.space_volume
                .as_ref()
                .and_then(|volume| space_report_at(volume, &folder))
        };
        if let Some(report) = cached {
            let mut s = state.write();
            s.space_root = report.root.display().to_string();
            s.space = Some(report);
            s.space_selected.clear();
            s.space_selected_bytes.clear();
            return;
        }
        if state.read().space_volume.is_some() {
            state.write().notice = Some(Notice {
                ok: false,
                text: "That folder is not in the current map. Press Scan to rebuild from the selected root.".into(),
                refresh: None,
            });
            return;
        }
    }
    if !folder.exists() {
        state.write().notice = Some(Notice {
            ok: false,
            text: format!("That folder does not exist: {}", folder.display()),
            refresh: None,
        });
        return;
    }
    {
        let mut s = state.write();
        s.space_root = normalize_space_path(&trimmed);
        s.access = folder_access(Path::new("/Library/Application Support"));
    }
    let blocked = {
        let s = state.read();
        space_scan_blocked_message(&s)
    };
    if let Some(message) = blocked {
        state.write().notice = Some(Notice {
            ok: false,
            text: message,
            refresh: None,
        });
        return;
    }
    if state.read().busy && !force_rescan {
        return;
    }

    let view_folder = folder.clone();
    let volume_scan_root = state
        .read()
        .space_volume
        .as_ref()
        .map(|volume| volume.scan_root.clone())
        .unwrap_or_else(|| view_folder.clone());
    let scan_id = {
        let mut s = state.write();
        if force_rescan {
            s.space_volume = None;
        }
        s.space = None;
        s.space_selected.clear();
        s.space_selected_bytes.clear();
        s.space_scan_id = s.space_scan_id.wrapping_add(1);
        s.busy = true;
        s.status = "scanning";
        s.space_scan_id
    };
    let scan_root = if force_rescan {
        volume_scan_root
    } else {
        view_folder.clone()
    };
    spawn(async move {
        let volume = scan_space_volume(scan_root).await;
        let report = space_report_at(&volume, &view_folder).unwrap_or(SpaceReport {
            root: view_folder,
            total: 0,
            children: Vec::new(),
        });
        {
            let mut s = state.write();
            if scan_id != s.space_scan_id {
                return;
            }
            s.space_volume = Some(volume);
            s.space_root = normalize_space_path(&report.root.display().to_string());
            s.space = Some(report);
            s.space_selected.clear();
            s.space_selected_bytes.clear();
        }
        finish_and_pump(state);
    });
}

fn space_scan_blocked_message(state: &AppState) -> Option<String> {
    let root = PathBuf::from(state.space_root.trim());
    let mac = computer_root();
    let needs_fda = root == mac || (root.starts_with(&mac) && !root.starts_with(&state.home));
    if needs_fda && state.access != AccessStatus::Granted {
        Some(
            "Full Disk Access is required to scan this drive. Open System Settings → Privacy & Security → Full Disk Access, enable Sable, then press Scan again.".into(),
        )
    } else {
        None
    }
}

pub fn space_open(mut state: Signal<AppState>, path: PathBuf) {
    let next = path.display().to_string();
    if next == state.read().space_root.trim() {
        return;
    }
    let ok = {
        let s = state.read();
        match s.space_volume.as_ref() {
            None => {
                drop(s);
                state.write().notice = Some(Notice {
                    ok: false,
                    text: "Press Scan once. Folders open from that map.".into(),
                    refresh: None,
                });
                return;
            }
            Some(volume) => volume_contains(volume, &path),
        }
    };
    if !ok {
        state.write().notice = Some(Notice {
            ok: false,
            text: "That folder is not in the current map. Press Scan to rebuild.".into(),
            refresh: None,
        });
        return;
    }
    {
        let mut s = state.write();
        let current = s.space_root.trim().to_string();
        s.space_future.clear();
        s.space_history.push(current);
        s.space_root = next;
        s.space_selected.clear();
        s.space_selected_bytes.clear();
    }
    start_space(state, false);
}

pub fn space_back(mut state: Signal<AppState>) {
    let prev = state.write().space_history.pop();
    let Some(prev) = prev else {
        return;
    };
    {
        let mut s = state.write();
        let current = s.space_root.trim().to_string();
        s.space_future.push(current);
        s.space_root = prev;
        s.space_selected.clear();
        s.space_selected_bytes.clear();
    }
    start_space(state, false);
}

pub fn space_forward(mut state: Signal<AppState>) {
    let next = state.write().space_future.pop();
    let Some(next) = next else {
        return;
    };
    {
        let mut s = state.write();
        let current = s.space_root.trim().to_string();
        s.space_history.push(current);
        s.space_root = next;
        s.space_selected.clear();
        s.space_selected_bytes.clear();
    }
    start_space(state, false);
}

pub fn start_scan(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    {
        let mut s = state.write();
        s.detections.clear();
        s.scanned = 0;
        s.detection_count = 0;
        s.errors = 0;
        s.scan_ran = true;
        s.last_path.clear();
    }
    let control = Arc::new(ExecutionControl::new());
    let watch = state.read().watch.clone();
    let detector = Arc::new(ExtensionWatchDetector::new(
        watch
            .split(',')
            .map(|s| s.trim().trim_start_matches('.'))
            .filter(|s| !s.is_empty()),
    ));
    let config = ScanConfig::new(state.read().root.trim());
    {
        let mut s = state.write();
        s.control = Some(Arc::clone(&control));
        s.busy = true;
        s.status = "scanning";
    }
    spawn(async move {
        let (inner_tx, mut inner_rx) = mpsc::channel(256);
        let control_task = Arc::clone(&control);
        tokio::spawn(async move {
            run_scan(config, detector, control_task, inner_tx).await;
        });
        while let Some(event) = inner_rx.recv().await {
            match event {
                ScanEvent::Progress { path } => {
                    state.write().last_path = path.display().to_string();
                }
                ScanEvent::Checked { path, .. } => {
                    let mut s = state.write();
                    s.scanned += 1;
                    s.last_path = path.display().to_string();
                }
                ScanEvent::Detection(hit) => {
                    let mut s = state.write();
                    s.scanned += 1;
                    s.detection_count += 1;
                    s.last_path = hit.path.display().to_string();
                    s.detections.push(hit);
                }
                ScanEvent::Error { path, message } => {
                    let mut s = state.write();
                    s.errors += 1;
                    s.last_path = format!("{} ({message})", path.display());
                }
                ScanEvent::Finished {
                    scanned,
                    detections,
                    errors,
                } => {
                    {
                        let mut s = state.write();
                        s.scanned = scanned;
                        s.detection_count = detections;
                        s.errors = errors;
                    }
                    finish_and_pump(state);
                    break;
                }
            }
        }
    });
}

pub fn perform_confirm(state: Signal<AppState>, kind: ConfirmKind) {
    match kind {
        ConfirmKind::TrashRoot(path) => trash(state, path),
        ConfirmKind::TrashRoots(paths) => trash_roots(state, paths),
        ConfirmKind::TrashDownload(path) => trash_download(state, path),
        ConfirmKind::TrashSpace(paths) => trash_space(state, paths),
        ConfirmKind::TrashThreat(path) => trash_threat(state, path),
        ConfirmKind::Reindex => run_tool(state, Tool::Reindex),
    }
}

fn apply_notice(mut state: Signal<AppState>, notice: Notice) {
    let rescan_space = notice.refresh == Some(Screen::Space);
    let on_space = state.read().screen == Screen::Space;
    {
        let mut s = state.write();
        if let Some(screen) = notice.refresh {
            s.loaded.remove(&screen);
            s.loaded.remove(&Screen::Care);
        }
        s.notice = Some(notice);
        s.finish();
    }
    pump_care_queue(state);
    if rescan_space && on_space {
        state.write().space_volume = None;
        start_space(state, true);
    }
}
