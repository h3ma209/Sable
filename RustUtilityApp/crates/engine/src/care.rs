use std::path::{Path, PathBuf};

use crate::access::{folder_access, AccessStatus};
use crate::apps::installed_app_count;
use crate::cleanup::junk_roots;
use crate::performance::{launch_agents, machine_summary_sample};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CareState {
    Idle,
    Scanning,
    ScanResults,
}

impl CareState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Scanning => "scanning",
            Self::ScanResults => "scan results",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleResult {
    pub name: String,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CareReport {
    pub modules: Vec<ModuleResult>,
    pub access: AccessStatus,
    pub cleanup_bytes: u64,
    pub app_count: usize,
    pub agent_count: usize,
}

/// Fan-out across cleanup, protection access, performance, and applications.
/// Does not delete files.
pub async fn run_care(home: PathBuf) -> CareReport {
    let home_cleanup = home.clone();
    let home_apps = home.clone();
    let home_agents = home.clone();
    let (junk, app_count, agents, sample) = tokio::join!(
        tokio::task::spawn_blocking(move || junk_roots(&home_cleanup)),
        tokio::task::spawn_blocking(move || installed_app_count(&home_apps)),
        tokio::task::spawn_blocking(move || launch_agents(&home_agents)),
        tokio::task::spawn_blocking(machine_summary_sample),
    );
    let junk = junk.unwrap_or_default();
    let app_count = app_count.unwrap_or(0);
    let agents = agents.unwrap_or_default();
    let sample = sample.unwrap_or(crate::performance::MachineSample {
        used_memory: 0,
        total_memory: 0,
        available_memory: 0,
        cpu_percent: 0.0,
        swap_used: 0,
        swap_total: 0,
        process_count: 0,
        uptime_secs: 0,
        load_avg_1m: 0.0,
        load_avg_5m: 0.0,
        load_avg_15m: 0.0,
        boot_volume: crate::performance::VolumeBytes { total: 0, free: 0 },
        top_cpu: Vec::new(),
        top_mem: Vec::new(),
    });
    let cache_bytes = junk.iter().fold(0u64, |sum, root| sum.saturating_add(root.bytes));
    let access = folder_access(Path::new("/Library/Application Support"));
    CareReport {
        access,
        cleanup_bytes: cache_bytes,
        app_count,
        agent_count: agents.len(),
        modules: vec![
            ModuleResult {
                name: "Cleanup".into(),
                summary: format!(
                    "{} roots, {} bytes",
                    junk.iter().filter(|root| root.exists).count(),
                    cache_bytes
                ),
            },
            ModuleResult {
                name: "Protection".into(),
                summary: format!("folder access {}", access.label()),
            },
            ModuleResult {
                name: "Performance".into(),
                summary: format!(
                    "{} launch agents, {:.0}% cpu, {} / {} bytes memory",
                    agents.len(),
                    sample.cpu_percent,
                    sample.used_memory,
                    sample.total_memory
                ),
            },
            ModuleResult {
                name: "Applications".into(),
                summary: format!("{} apps", app_count),
            },
        ],
    }
}
