use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use dioxus::prelude::*;
use engine::{
    flush_dns_cache, folder_access, junk_roots, large_downloads, launch_agents, local_cloud_folders,
    machine_sample, move_download_to_trash, move_root_to_trash, prepare_and_scan, reindex_spotlight,
    remember_ignore, run_care, run_scan, scan_space_volume_with_options, space_report_at, spotlight_status,
    survey_leftovers, trash_selected, trash_threat as engine_trash_threat, volume_contains,
    computer_root, AccessStatus,
    ExecutionControl, ExtensionWatchDetector, MachineSample, ScanConfig, ScanEvent, SpaceProgress, SpaceReport,
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
    let home = state.read().home.clone();
    let volume_scan_root = state
        .read()
        .space_volume
        .as_ref()
        .and_then(|volume| {
            if view_folder.starts_with(&volume.scan_root)
                && view_folder != computer_root()
                && view_folder != Path::new("/")
                && view_folder != home
            {
                Some(volume.scan_root.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| view_folder.clone());
    let control = Arc::new(ExecutionControl::new());
    let control_for_scan = Arc::clone(&control);
    let scan_id = {
        let mut s = state.write();
        s.control = Some(control);
        if force_rescan {
            s.space_volume = None;
        }
        s.space = None;
        s.space_selected.clear();
        s.space_selected_bytes.clear();
        s.space_progress_bytes = 0;
        s.space_progress_total_bytes = 0;
        s.space_progress_items = 0;
        s.space_progress_current = String::new();
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

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SpaceProgress>();
    {
        let mut state_listener = state;
        spawn(async move {
            while let Some(prog) = rx.recv().await {
                let mut s = state_listener.write();
                if s.space_scan_id != scan_id {
                    break;
                }
                s.space_progress_bytes = prog.bytes_done;
                s.space_progress_total_bytes = prog.bytes_total;
                s.space_progress_items = prog.items_done;
                s.space_progress_current = prog.current_path;
            }
        });
    }

    let shallow_dirs: Vec<String> = state
        .read()
        .space_exclude_dirs
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    spawn(async move {
        let volume = scan_space_volume_with_options(
            scan_root,
            Some(control_for_scan),
            Some(tx),
            shallow_dirs,
        ).await;
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

pub fn stop_space(mut state: Signal<AppState>) {
    let mut s = state.write();
    s.cancel_work();
    s.notice = Some(Notice {
        ok: true,
        text: "Space scan stopped.".into(),
        refresh: None,
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
    let filename_query = state.read().filename_query.clone();
    let exclude_dirs = state.read().exclude_dirs.clone();
    let detector = Arc::new(ExtensionWatchDetector::with_names(
        watch
            .split(',')
            .map(|s| s.trim().trim_start_matches('.'))
            .filter(|s| !s.is_empty()),
        filename_query
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty()),
    ));
    let config = ScanConfig {
        exclude_dirs: exclude_dirs
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        ..ScanConfig::new(state.read().root.trim())
    };
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

pub fn stop_scan(mut state: Signal<AppState>) {
    let mut s = state.write();
    s.cancel_work();
    s.notice = Some(Notice {
        ok: true,
        text: "File search stopped.".into(),
        refresh: None,
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
        ConfirmKind::InstallUpdate {
            tag,
            download_url,
            fallback_url,
            is_dmg,
        } => {
            install_update(state, tag, download_url, fallback_url, is_dmg);
        }
    }
}

pub fn check_for_updates_on_startup(mut state: Signal<AppState>) {
    spawn(async move {
        let current_version = env!("CARGO_PKG_VERSION");
        let result = tokio::task::spawn_blocking(fetch_latest_release_info)
            .await
            .unwrap_or_else(|e| Err(format!("Task error: {e}")));

        if let Ok(info) = result {
            let latest_clean = info.tag.trim().trim_start_matches('v');
            if is_version_newer(latest_clean, current_version) {
                let mut s = state.write();
                s.confirm = Some(crate::state::Confirm {
                    heading: format!("Update Available: {}", info.tag),
                    body: format!(
                        "A newer version of Sable ({}) is available!\nYou are running v{}.\n\nIt will be installed directly to /Applications and the app will restart automatically.",
                        info.tag, current_version
                    ),
                    action: "Update & Restart",
                    kind: ConfirmKind::InstallUpdate {
                        tag: info.tag,
                        download_url: info.download_url,
                        fallback_url: info.fallback_url,
                        is_dmg: info.is_dmg,
                    },
                });
            }
        }
    });
}

pub fn check_for_updates(mut state: Signal<AppState>) {
    if state.read().busy {
        return;
    }
    state.write().busy = true;
    state.write().status = "Checking for updates…";

    spawn(async move {
        let current_version = env!("CARGO_PKG_VERSION");
        let result = tokio::task::spawn_blocking(|| {
            fetch_latest_release_info()
        })
        .await
        .unwrap_or_else(|e| Err(format!("Task error: {e}")));

        let mut s = state.write();
        s.busy = false;
        s.status = "Idle";

        match result {
            Ok(info) => {
                let latest_clean = info.tag.trim().trim_start_matches('v');
                let is_newer = is_version_newer(latest_clean, current_version);
                if is_newer {
                    s.confirm = Some(crate::state::Confirm {
                        heading: format!("Update Available: {}", info.tag),
                        body: format!(
                            "A newer version of Sable ({}) is available!\nYou are running v{}.\n\nIt will be installed directly to /Applications and the app will restart automatically.",
                            info.tag, current_version
                        ),
                        action: "Update & Restart",
                        kind: ConfirmKind::InstallUpdate {
                            tag: info.tag,
                            download_url: info.download_url,
                            fallback_url: info.fallback_url,
                            is_dmg: info.is_dmg,
                        },
                    });
                } else {
                    s.confirm = Some(crate::state::Confirm {
                        heading: "Sable is Up to Date".into(),
                        body: format!(
                            "You are currently running the latest version of Sable (v{}).\n\nWould you like to reinstall directly to /Applications and restart automatically?",
                            current_version
                        ),
                        action: "Reinstall & Restart",
                        kind: ConfirmKind::InstallUpdate {
                            tag: info.tag,
                            download_url: info.download_url,
                            fallback_url: info.fallback_url,
                            is_dmg: info.is_dmg,
                        },
                    });
                    s.notice = Some(Notice {
                        ok: true,
                        text: format!("Sable is up to date (v{}).", current_version),
                        refresh: None,
                    });
                }
            }
            Err(err) => {
                s.notice = Some(Notice {
                    ok: false,
                    text: format!("Update check failed: {err}"),
                    refresh: None,
                });
            }
        }
    });
}

pub fn install_update(
    mut state: Signal<AppState>,
    tag: String,
    download_url: String,
    fallback_url: Option<String>,
    is_dmg: bool,
) {
    if state.read().busy {
        return;
    }
    state.write().busy = true;
    state.write().status = "Downloading & installing update…";

    spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            perform_update_download_and_install(&tag, &download_url, fallback_url.as_deref(), is_dmg)
        })
        .await
        .unwrap_or_else(|e| Err(format!("Task error: {e}")));

        let mut s = state.write();
        s.busy = false;
        s.status = "Idle";

        match result {
            Ok(msg) => {
                s.notice = Some(Notice {
                    ok: true,
                    text: msg,
                    refresh: None,
                });
                let _ = Command::new("sh")
                    .args(["-c", "sleep 0.5 && open -n -a /Applications/Sable.app"])
                    .spawn();
                std::thread::sleep(std::time::Duration::from_millis(300));
                std::process::exit(0);
            }
            Err(err) => {
                s.notice = Some(Notice {
                    ok: false,
                    text: format!("Update failed: {err}"),
                    refresh: None,
                });
            }
        }
    });
}

struct ReleaseInfo {
    tag: String,
    download_url: String,
    fallback_url: Option<String>,
    is_dmg: bool,
}

fn fetch_latest_release_info() -> Result<ReleaseInfo, String> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--retry", "2",
            "-H", "User-Agent: Sable-App",
            "-H", "Accept: application/vnd.github.v3+json",
            "https://api.github.com/repos/h3ma209/Sable/releases/latest",
        ])
        .output()
        .map_err(|e| format!("Failed to reach update server: {e}"))?;

    if !output.status.success() {
        return Err("Could not reach update server".into());
    }

    let json = String::from_utf8(output.stdout)
        .map_err(|e| format!("Invalid server response: {e}"))?;

    let tag = extract_json_field(&json, "tag_name")
        .ok_or_else(|| "Could not parse release tag from server".to_string())?;

    let (download_url, fallback_url, is_dmg) = select_best_release_asset(&json, &tag);

    Ok(ReleaseInfo {
        tag,
        download_url,
        fallback_url,
        is_dmg,
    })
}

fn select_best_release_asset(json: &str, tag: &str) -> (String, Option<String>, bool) {
    let arch_keyword = if std::env::consts::ARCH == "x86_64" { "x64" } else { "arm64" };

    struct Candidate {
        api_url: String,
        browser_url: String,
        is_zip: bool,
        is_dmg: bool,
        is_arch: bool,
    }

    let mut candidates = Vec::new();

    let marker = "\"url\":";
    let mut slice = json;
    while let Some(idx) = slice.find(marker) {
        slice = &slice[idx + marker.len()..];
        let Some(qs) = slice.find('"') else { continue };
        let Some(qe) = slice[qs + 1..].find('"') else { continue };
        let url = slice[qs + 1..qs + 1 + qe].to_string();
        if !url.contains("/releases/assets/") {
            continue;
        }

        let block_end = slice.find('}').unwrap_or(slice.len());
        let block = &slice[..block_end];

        let browser_url = if let Some(b_idx) = block.find("\"browser_download_url\":") {
            let b_rest = &block[b_idx + 23..];
            if let Some(b_qs) = b_rest.find('"') {
                if let Some(b_qe) = b_rest[b_qs + 1..].find('"') {
                    b_rest[b_qs + 1..b_qs + 1 + b_qe].to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let is_arch = block.contains(arch_keyword);
        let is_zip = block.contains(".zip");
        let is_dmg = block.contains(".dmg");

        candidates.push(Candidate {
            api_url: url,
            browser_url,
            is_zip,
            is_dmg,
            is_arch,
        });
    }

    if let Some(c) = candidates.iter().find(|c| c.is_arch && c.is_zip) {
        let fallback = if !c.browser_url.is_empty() { Some(c.browser_url.clone()) } else { None };
        return (c.api_url.clone(), fallback, false);
    }

    if let Some(c) = candidates.iter().find(|c| c.is_arch && c.is_dmg) {
        let fallback = if !c.browser_url.is_empty() { Some(c.browser_url.clone()) } else { None };
        return (c.api_url.clone(), fallback, true);
    }

    if let Some(c) = candidates.iter().find(|c| c.is_zip) {
        let fallback = if !c.browser_url.is_empty() { Some(c.browser_url.clone()) } else { None };
        return (c.api_url.clone(), fallback, false);
    }

    let clean = tag.trim_start_matches('v');
    let fallback = format!("https://github.com/h3ma209/Sable/releases/download/{tag}/Sable-{clean}-macos-{arch_keyword}.zip");
    (fallback, None, false)
}

fn extract_json_field(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\":");
    let idx = json.find(&pattern)?;
    let rest = &json[idx + pattern.len()..];
    let quote_start = rest.find('"')?;
    let quote_end = rest[quote_start + 1..].find('"')?;
    Some(rest[quote_start + 1..quote_start + 1 + quote_end].to_string())
}

fn perform_update_download_and_install(
    tag: &str,
    primary_url: &str,
    fallback_url: Option<&str>,
    is_dmg: bool,
) -> Result<String, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_dir = std::env::temp_dir().join(format!("sable_update_{}_{}", std::process::id(), now));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let archive_path = tmp_dir.join("update.pkg");
    let unpack_dir = tmp_dir.join("unpacked");
    let _ = std::fs::create_dir_all(&unpack_dir);

    let mut urls_to_try = vec![primary_url];
    if let Some(fb) = fallback_url {
        if fb != primary_url {
            urls_to_try.push(fb);
        }
    }

    let mut downloaded = false;
    let mut last_err = String::new();

    for url in urls_to_try {
        let _ = std::fs::remove_file(&archive_path);
        let status = Command::new("curl")
            .args([
                "-fsSL",
                "--retry", "3",
                "--retry-delay", "1",
                "-H", "Accept: application/octet-stream",
                "-H", "User-Agent: Sable-Updater",
                "-o",
            ])
            .arg(&archive_path)
            .arg(url)
            .status();

        match status {
            Ok(s) if s.success() => {
                if let Ok(meta) = std::fs::metadata(&archive_path) {
                    if meta.len() >= 100_000 {
                        downloaded = true;
                        break;
                    } else {
                        last_err = format!("Downloaded file too small ({} bytes)", meta.len());
                    }
                } else {
                    last_err = "Archive missing after download".into();
                }
            }
            Ok(s) => {
                last_err = format!("curl failed with exit status {s}");
            }
            Err(e) => {
                last_err = format!("curl execute error: {e}");
            }
        }
    }

    if !downloaded {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(format!("Download failed: {last_err}"));
    }

    // Inspect magic bytes for ZIP: starts with PK\x03\x04
    let is_zip = std::fs::File::open(&archive_path)
        .and_then(|mut f| {
            use std::io::Read;
            let mut magic = [0u8; 4];
            f.read_exact(&mut magic)?;
            Ok(&magic == b"PK\x03\x04")
        })
        .unwrap_or(!is_dmg);

    let app_source = if is_zip {
        let status = Command::new("ditto")
            .args(["-x", "-k"])
            .arg(&archive_path)
            .arg(&unpack_dir)
            .status()
            .map_err(|e| format!("ditto extraction failed: {e}"))?;

        if !status.success() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err("Failed to extract update package".into());
        }

        find_app_bundle(&unpack_dir).ok_or_else(|| {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            "Sable.app not found in update package".to_string()
        })?
    } else {
        let mount_point = tmp_dir.join("mount");
        let _ = std::fs::create_dir_all(&mount_point);
        let status = Command::new("hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
            .arg(&mount_point)
            .arg(&archive_path)
            .status()
            .map_err(|e| format!("hdiutil attach failed: {e}"))?;

        if !status.success() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err("Failed to mount update DMG".into());
        }

        let app_in_dmg = find_app_bundle(&mount_point);
        let extracted_app = unpack_dir.join("Sable.app");

        if let Some(ref source) = app_in_dmg {
            let _ = Command::new("cp")
                .args(["-R"])
                .arg(source)
                .arg(&extracted_app)
                .status();
        }

        let _ = Command::new("hdiutil")
            .args(["detach", "-force"])
            .arg(&mount_point)
            .status();

        if !extracted_app.exists() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err("Sable.app not found inside DMG image".into());
        }
        extracted_app
    };

    let dest_dir = PathBuf::from("/Applications/Sable.app");
    if dest_dir.exists() {
        let _ = std::fs::remove_dir_all(&dest_dir);
    }

    let status = Command::new("cp")
        .args(["-R"])
        .arg(&app_source)
        .arg(&dest_dir)
        .status()
        .map_err(|e| format!("Failed to copy app to /Applications: {e}"))?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err("Failed to install to /Applications (check permissions)".into());
    }

    let _ = Command::new("xattr")
        .args(["-cr"])
        .arg(&dest_dir)
        .status();

    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(format!("Sable {tag} successfully installed to /Applications/Sable.app! Restarting…"))
}

fn find_app_bundle(dir: &Path) -> Option<PathBuf> {
    if dir.extension().and_then(|e| e.to_str()) == Some("app") {
        return Some(dir.to_path_buf());
    }
    let direct = dir.join("Sable.app");
    if direct.is_dir() {
        return Some(direct);
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("app") {
                return Some(path);
            }
            if path.is_dir() {
                let nested = path.join("Sable.app");
                if nested.is_dir() {
                    return Some(nested);
                }
            }
        }
    }
    None
}

pub fn parse_version(s: &str) -> (u32, u32, u32) {
    let clean = s.trim().trim_start_matches('v');
    let mut parts = clean.split('.');
    let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|p| p.split('-').next()).and_then(|p| p.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

pub fn is_version_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        assert!(is_version_newer("0.3.7", "0.3.6"));
        assert!(is_version_newer("0.3.6", "0.3.5"));
        assert!(is_version_newer("0.3.5", "0.3.4"));
        assert!(is_version_newer("1.0.0", "0.3.1"));
        assert!(!is_version_newer("0.3.5", "0.3.5"));
        assert!(!is_version_newer("0.3.4", "0.3.5"));
    }

    #[test]
    fn test_select_best_release_asset() {
        let sample_json = r#"{
            "tag_name": "v0.3.0",
            "assets": [
                {
                    "url": "https://api.github.com/repos/h3ma209/Sable/releases/assets/723592428",
                    "name": "Sable-0.3.0-macos-arm64.dmg",
                    "browser_download_url": "https://github.com/h3ma209/Sable/releases/download/v0.3.0/Sable-0.3.0-macos-arm64.dmg"
                },
                {
                    "url": "https://api.github.com/repos/h3ma209/Sable/releases/assets/723592562",
                    "name": "Sable-0.3.0-macos-arm64.zip",
                    "browser_download_url": "https://github.com/h3ma209/Sable/releases/download/v0.3.0/Sable-0.3.0-macos-arm64.zip"
                }
            ]
        }"#;

        let (primary, fallback, is_dmg) = select_best_release_asset(sample_json, "v0.3.0");
        assert!(!is_dmg);
        #[cfg(target_arch = "aarch64")]
        {
            assert!(primary.contains("723592562"), "Primary URL should be direct API asset URL: {primary}");
            assert!(fallback.as_deref().unwrap_or("").contains("Sable-0.3.0-macos-arm64.zip"));
        }
    }
}
