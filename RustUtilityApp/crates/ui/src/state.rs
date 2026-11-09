use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use engine::{
    folder_access, AccessStatus, CareReport, CloudFolder, Detection, DuplicateReport, HostPoint,
    InstalledApp, JunkRoot, LargeFile, LaunchAgent, LeftoverHit, MachineSample, MonitorAnalytics,
    ProcessPoint, SnapshotReport, SpaceReport, SpaceVolumeScan, TelemetrySummary, ThreatReport,
    ExecutionControl,
};

use crate::assets::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Screen {
    Care,
    Cleanup,
    Apps,
    Performance,
    Protection,
    Malware,
    Clutter,
    Space,
    Cloud,
    Assistant,
    Duplicates,
    Snapshots,
}

impl Screen {
    pub fn label(self) -> &'static str {
        match self {
            Self::Care => "Home",
            Self::Cleanup => "Free Up Space",
            Self::Apps => "Apps",
            Self::Performance => "Tune-Up",
            Self::Protection => "File Finder",
            Self::Malware => "Threat Scan",
            Self::Clutter => "Big Downloads",
            Self::Space => "Space Lens",
            Self::Cloud => "Cloud Storage",
            Self::Assistant => "System Monitor",
            Self::Duplicates => "Duplicates",
            Self::Snapshots => "Snapshots",
        }
    }

    #[allow(dead_code)]
    pub fn tagline(self) -> &'static str {
        match self {
            Self::Care => "Health check of your Mac",
            Self::Cleanup => "Caches and logs you can reclaim",
            Self::Apps => "Installed apps and leftovers",
            Self::Performance => "Startup items and quick fixes",
            Self::Protection => "Find installers and archives by type",
            Self::Malware => "Compare files to known malware hashes",
            Self::Clutter => "Large files sitting in Downloads",
            Self::Space => "Visual map of what uses your disk",
            Self::Cloud => "Local iCloud and cloud folder sizes",
            Self::Assistant => "Live CPU, memory, and top processes",
            Self::Duplicates => "Same-content files wasting disk space",
            Self::Snapshots => "APFS and Time Machine local snapshots",
        }
    }

    pub fn auto_loads(self) -> bool {
        matches!(
            self,
            Self::Care | Self::Cleanup | Self::Cloud | Self::Assistant
        )
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Care => ICON_HOME,
            Self::Cleanup => ICON_FREE_UP_SPACE,
            Self::Apps => ICON_APPS,
            Self::Performance => ICON_TUNE_UP,
            Self::Protection => ICON_FILE_FINDER,
            Self::Malware => ICON_THREAT_SCAN,
            Self::Clutter => ICON_BIG_DOWNLOADS,
            Self::Space => ICON_SPACE_LENS,
            Self::Cloud => ICON_CLOUD_STORAGE,
            Self::Assistant => ICON_NOW_PLAYING,
            Self::Duplicates => ICON_DUPLICATES,
            Self::Snapshots => ICON_SNAPSHOTS,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Notice {
    pub ok: bool,
    pub text: String,
    pub refresh: Option<Screen>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum ConfirmKind {
    TrashRoot(PathBuf),
    TrashRoots(Vec<PathBuf>),
    TrashDownload(PathBuf),
    #[allow(dead_code)]
    TrashSpace(Vec<PathBuf>),
    TrashThreat(PathBuf),
    Reindex,
    InstallUpdate {
        tag: String,
        download_url: String,
        fallback_url: Option<String>,
        is_dmg: bool,
    },
}

#[derive(Clone)]
pub struct Confirm {
    pub heading: String,
    pub body: String,
    pub action: &'static str,
    pub kind: ConfirmKind,
}

#[derive(Clone, Copy)]
pub enum Tool {
    Dns,
    SpotlightStatus,
    Reindex,
}

#[derive(Clone)]
pub struct AppState {
    pub screen: Screen,
    pub home: PathBuf,
    pub root: String,
    pub filename_query: String,
    pub watch: String,
    pub exclude_dirs: String,
    pub status: &'static str,
    pub notice: Option<Notice>,
    pub confirm: Option<Confirm>,
    pub loaded: HashSet<Screen>,
    pub last_path: String,
    pub scanned: u64,
    pub detection_count: u64,
    pub errors: u64,
    pub scan_ran: bool,
    pub detections: Vec<Detection>,
    pub care: Option<CareReport>,
    pub junk: Vec<JunkRoot>,
    pub dev_junk: Vec<JunkRoot>,
    pub cleanup_tab: usize,
    pub agents: Vec<LaunchAgent>,
    pub apps: Vec<InstalledApp>,
    pub app_filter: String,
    pub leftovers: Vec<LeftoverHit>,
    pub leftovers_done: bool,
    pub selected_app: Option<usize>,
    pub sample: Option<MachineSample>,
    pub space_root: String,
    pub space: Option<SpaceReport>,
    pub space_volume: Option<SpaceVolumeScan>,
    pub space_selected: HashSet<PathBuf>,
    pub space_selected_bytes: HashMap<PathBuf, u64>,
    pub threat_root: String,
    pub threat: Option<ThreatReport>,
    pub clutter: Vec<LargeFile>,
    pub cloud: Vec<CloudFolder>,
    pub duplicates_root: String,
    pub duplicates: Option<DuplicateReport>,
    pub snapshots: Option<SnapshotReport>,
    pub access: AccessStatus,
    pub control: Option<Arc<ExecutionControl>>,
    pub busy: bool,
    pub space_scan_id: u64,
    pub care_picks: HashSet<Screen>,
    pub care_followups: Vec<Screen>,
    pub last_refresh: Option<Instant>,
    pub space_history: Vec<String>,
    pub space_future: Vec<String>,
    pub space_filter: String,
    pub space_exclude_dirs: String,
    pub space_sort: usize, // 0 = Size desc, 1 = Name asc, 2 = Type
    pub apps_tab: usize,
    pub perf_tab: usize,
    pub clutter_tab: usize,
    pub clutter_filter: String,
    pub auto_poll_monitor: bool,
    pub monitor_sampling: bool,
    pub monitor_analytics_loading: bool,
    pub monitor_log_enabled: bool,
    pub monitor_tab: usize,
    pub monitor_range_hours: u32,
    pub monitor_history: Vec<HostPoint>,
    pub monitor_selected_host_id: Option<i64>,
    pub monitor_selected_processes: Vec<ProcessPoint>,
    pub monitor_process_focus: Option<String>,
    pub monitor_process_series: Vec<ProcessPoint>,
    pub monitor_summary: Option<TelemetrySummary>,
    pub monitor_analytics: Option<MonitorAnalytics>,
    pub monitor_spark_cpu: Vec<f32>,
    pub monitor_spark_mem: Vec<f32>,
    pub monitor_spark_swap: Vec<f32>,
    #[allow(dead_code)]
    pub malware_show_skipped: bool,
    pub space_progress_bytes: u64,
    pub space_progress_total_bytes: u64,
    pub space_progress_items: u64,
    pub space_progress_current: String,
}

impl AppState {
    pub fn new() -> Self {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        let threat_root = home.join("Downloads").display().to_string();
        let space_root = home.display().to_string();
        let duplicates_root = space_root.clone();
        let access = folder_access(std::path::Path::new("/Library/Application Support"));
        Self {
            screen: Screen::Care,
            root: home.join("Downloads").display().to_string(),
            filename_query: String::new(),
            home,
            watch: "dmg,pkg".into(),
            exclude_dirs: "node_modules, .git, target, build, .gradle, npm".into(),
            status: "idle",
            notice: None,
            confirm: None,
            loaded: HashSet::new(),
            last_path: String::new(),
            scanned: 0,
            detection_count: 0,
            errors: 0,
            scan_ran: false,
            detections: Vec::new(),
            care: None,
            junk: Vec::new(),
            dev_junk: Vec::new(),
            cleanup_tab: 0,
            agents: Vec::new(),
            apps: Vec::new(),
            app_filter: String::new(),
            leftovers: Vec::new(),
            leftovers_done: false,
            selected_app: None,
            sample: None,
            space_root,
            space_exclude_dirs: "node_modules, .gradle, npm, target, build, .git".into(),
            space: None,
            space_volume: None,
            space_selected: HashSet::new(),
            space_selected_bytes: HashMap::new(),
            threat_root,
            threat: None,
            clutter: Vec::new(),
            cloud: Vec::new(),
            duplicates_root,
            duplicates: None,
            snapshots: None,
            access,
            control: None,
            busy: false,
            space_scan_id: 0,
            care_picks: HashSet::from([
                Screen::Cleanup,
                Screen::Malware,
                Screen::Performance,
                Screen::Clutter,
            ]),
            care_followups: Vec::new(),
            last_refresh: None,
            space_history: Vec::new(),
            space_future: Vec::new(),
            space_filter: String::new(),
            space_sort: 0,
            apps_tab: 0,
            perf_tab: 0,
            clutter_tab: 0,
            clutter_filter: String::new(),
            auto_poll_monitor: true,
            monitor_sampling: false,
            monitor_analytics_loading: false,
            monitor_log_enabled: false,
            monitor_tab: 0,
            monitor_range_hours: 6,
            monitor_history: Vec::new(),
            monitor_selected_host_id: None,
            monitor_selected_processes: Vec::new(),
            monitor_process_focus: None,
            monitor_process_series: Vec::new(),
            monitor_summary: None,
            monitor_analytics: None,
            monitor_spark_cpu: Vec::new(),
            monitor_spark_mem: Vec::new(),
            monitor_spark_swap: Vec::new(),
            malware_show_skipped: false,
            space_progress_bytes: 0,
            space_progress_total_bytes: 0,
            space_progress_items: 0,
            space_progress_current: String::new(),
        }
    }

    pub fn go(&mut self, screen: Screen) {
        self.screen = screen;
        self.notice = None;
    }

    pub fn finish(&mut self) {
        let scanned = self.status == "scanning";
        self.busy = false;
        self.status = "idle";
        self.control = None;
        if scanned {
            self.last_refresh = Some(Instant::now());
        }
    }

    #[allow(dead_code)]
    pub fn cancel_work(&mut self) {
        if let Some(control) = &self.control {
            control.cancel();
        }
        self.space_scan_id = self.space_scan_id.wrapping_add(1);
        self.care_followups.clear();
        self.busy = false;
        self.status = "idle";
        self.control = None;
    }
}

pub fn rail_groups() -> [(&'static str, &'static [Screen]); 4] {
    [
        ("Made for you", &[Screen::Care]),
        (
            "Library",
            &[
                Screen::Cleanup,
                Screen::Clutter,
                Screen::Space,
                Screen::Cloud,
                Screen::Duplicates,
                Screen::Snapshots,
            ],
        ),
        (
            "Your Mac",
            &[Screen::Apps, Screen::Performance, Screen::Assistant],
        ),
        ("Safety", &[Screen::Protection, Screen::Malware]),
    ]
}
