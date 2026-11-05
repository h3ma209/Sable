use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use dioxus::prelude::*;
use engine::{
    folder_access, AccessStatus, CareReport, CloudFolder, Detection, InstalledApp, JunkRoot,
    LargeFile, LaunchAgent, LeftoverHit, MachineSample, SpaceReport, SpaceVolumeScan, ThreatReport,
    ExecutionControl,
};

const ICON_HOME: Asset = asset!("/assets/sidebar/home.svg");
const ICON_FREE_UP_SPACE: Asset = asset!("/assets/sidebar/free-up-space.svg");
const ICON_APPS: Asset = asset!("/assets/sidebar/apps.svg");
const ICON_TUNE_UP: Asset = asset!("/assets/sidebar/tune-up.svg");
const ICON_FILE_FINDER: Asset = asset!("/assets/sidebar/file-finder.svg");
const ICON_THREAT_SCAN: Asset = asset!("/assets/sidebar/threat-scan.svg");
const ICON_BIG_DOWNLOADS: Asset = asset!("/assets/sidebar/big-downloads.svg");
const ICON_SPACE_LENS: Asset = asset!("/assets/sidebar/space-lens.svg");
const ICON_CLOUD_STORAGE: Asset = asset!("/assets/sidebar/cloud-storage.svg");
const ICON_NOW_PLAYING: Asset = asset!("/assets/sidebar/now-playing.svg");

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
            Self::Assistant => "Now Playing",
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
            Self::Assistant => "Live CPU and memory readings",
        }
    }

    pub fn auto_loads(self) -> bool {
        matches!(
            self,
            Self::Care | Self::Cleanup | Self::Cloud | Self::Assistant
        )
    }

    pub fn icon(self) -> Asset {
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
pub enum ConfirmKind {
    TrashRoot(PathBuf),
    TrashRoots(Vec<PathBuf>),
    TrashDownload(PathBuf),
    #[allow(dead_code)]
    TrashSpace(Vec<PathBuf>),
    TrashThreat(PathBuf),
    Reindex,
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
    pub watch: String,
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
    pub space_sort: usize, // 0 = Size desc, 1 = Name asc, 2 = Type
    pub apps_tab: usize,
    pub perf_tab: usize,
    pub clutter_tab: usize,
    pub clutter_filter: String,
    pub auto_poll_monitor: bool,
    #[allow(dead_code)]
    pub malware_show_skipped: bool,
}

impl AppState {
    pub fn new() -> Self {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        let threat_root = home.join("Downloads").display().to_string();
        let space_root = home.display().to_string();
        let access = folder_access(std::path::Path::new("/Library/Application Support"));
        Self {
            screen: Screen::Care,
            root: home.join("Downloads").display().to_string(),
            home,
            watch: "dmg,pkg".into(),
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
            agents: Vec::new(),
            apps: Vec::new(),
            app_filter: String::new(),
            leftovers: Vec::new(),
            leftovers_done: false,
            selected_app: None,
            sample: None,
            space_root,
            space: None,
            space_volume: None,
            space_selected: HashSet::new(),
            space_selected_bytes: HashMap::new(),
            threat_root,
            threat: None,
            clutter: Vec::new(),
            cloud: Vec::new(),
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
            malware_show_skipped: false,
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
            ],
        ),
        (
            "Your Mac",
            &[Screen::Apps, Screen::Performance, Screen::Assistant],
        ),
        ("Safety", &[Screen::Protection, Screen::Malware]),
    ]
}
