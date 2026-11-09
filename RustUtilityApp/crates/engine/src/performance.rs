use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, ProcessesToUpdate, RefreshKind, System,
    MINIMUM_CPU_UPDATE_INTERVAL,
};

fn monitor_system() -> &'static Mutex<System> {
    static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        Mutex::new(System::new_with_specifics(
            RefreshKind::nothing()
                .with_memory(MemoryRefreshKind::everything())
                .with_cpu(CpuRefreshKind::everything()),
        ))
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessSnap {
    pub name: String,
    pub pid: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VolumeBytes {
    pub total: u64,
    pub free: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MachineSample {
    pub used_memory: u64,
    pub total_memory: u64,
    pub available_memory: u64,
    pub cpu_percent: f32,
    pub swap_used: u64,
    pub swap_total: u64,
    pub process_count: usize,
    pub uptime_secs: u64,
    pub load_avg_1m: f32,
    pub load_avg_5m: f32,
    pub load_avg_15m: f32,
    pub boot_volume: VolumeBytes,
    pub top_cpu: Vec<ProcessSnap>,
    pub top_mem: Vec<ProcessSnap>,
}

fn top_processes(system: &System, by_cpu: bool, limit: usize) -> Vec<ProcessSnap> {
    let mut rows: Vec<ProcessSnap> = system
        .processes()
        .iter()
        .map(|(pid, process)| ProcessSnap {
            name: process.name().to_string_lossy().into_owned(),
            pid: pid.as_u32(),
            cpu_percent: process.cpu_usage(),
            memory_bytes: process.memory(),
        })
        .collect();
    if by_cpu {
        rows.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    } else {
        rows.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
    }
    rows.truncate(limit);
    rows
}

pub fn boot_volume_bytes() -> VolumeBytes {
    volume_bytes(std::path::Path::new("/"))
}

fn volume_bytes(path: &std::path::Path) -> VolumeBytes {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let empty = VolumeBytes { total: 0, free: 0 };
    let c_path = match CString::new(path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(_) => return empty,
    };
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
    let res = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if res != 0 {
        return empty;
    }
    let stat = unsafe { stat.assume_init() };
    let total = (stat.f_blocks as u64).saturating_mul(stat.f_frsize as u64);
    let free = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
    VolumeBytes { total, free }
}

#[cfg(target_os = "macos")]
fn load_averages() -> (f32, f32, f32) {
    use std::ffi::CString;
    let name = CString::new("vm.loadavg").expect("vm.loadavg");
    let mut load: [libc::c_double; 3] = [0.0; 3];
    let mut size = (load.len() * std::mem::size_of::<libc::c_double>()) as libc::size_t;
    let res = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            load.as_mut_ptr() as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if res == 0 {
        (
            load[0] as f32,
            load[1] as f32,
            load[2] as f32,
        )
    } else {
        (0.0, 0.0, 0.0)
    }
}

#[cfg(not(target_os = "macos"))]
fn load_averages() -> (f32, f32, f32) {
    (0.0, 0.0, 0.0)
}

/// One memory read and a short CPU sample. This loop is new. The original binary did not expose one.
pub fn machine_sample() -> MachineSample {
    let mut system = monitor_system()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // sysinfo CPU is a delta; needs two refreshes on the same System handle.
    system.refresh_memory_specifics(MemoryRefreshKind::everything());
    system.refresh_cpu_usage();
    system.refresh_processes(ProcessesToUpdate::All, true);
    std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
    system.refresh_memory_specifics(MemoryRefreshKind::everything());
    system.refresh_cpu_usage();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let cpu_percent = system.global_cpu_usage();
    let (load_avg_1m, load_avg_5m, load_avg_15m) = load_averages();
    let process_count = system.processes().len();
    MachineSample {
        used_memory: system.used_memory(),
        total_memory: system.total_memory(),
        available_memory: system.available_memory(),
        cpu_percent,
        swap_used: system.used_swap(),
        swap_total: system.total_swap(),
        process_count,
        uptime_secs: System::uptime(),
        load_avg_1m,
        load_avg_5m,
        load_avg_15m,
        boot_volume: boot_volume_bytes(),
        top_cpu: top_processes(&system, true, 12),
        top_mem: top_processes(&system, false, 12),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchAgent {
    pub path: PathBuf,
    pub label: String,
}

/// User and local LaunchAgents directories. Reads `Label` from each plist.
pub fn launch_agents(home: &Path) -> Vec<LaunchAgent> {
    let dirs = [
        home.join("Library/LaunchAgents"),
        PathBuf::from("/Library/LaunchAgents"),
    ];
    let mut agents = Vec::new();
    for dir in dirs {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("plist") {
                continue;
            }
            let label = plist_label(&path).unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("agent")
                    .to_string()
            });
            agents.push(LaunchAgent { path, label });
            if agents.len() >= 80 {
                return agents;
            }
        }
    }
    agents
}

fn plist_label(path: &Path) -> Option<String> {
    let output = Command::new("plutil")
        .args(["-extract", "Label", "raw", "-o", "-"])
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

/// Documented user DNS cache flush. Not a copied privileged script.
pub fn flush_dns_cache() -> Result<String, String> {
    let output = Command::new("dscacheutil")
        .arg("-flushcache")
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok("DNS cache flush finished.".into())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(if err.trim().is_empty() {
            "DNS cache flush failed.".into()
        } else {
            err.trim().to_string()
        })
    }
}

/// Read-only Spotlight status for the startup volume.
pub fn spotlight_status() -> Result<String, String> {
    run_mdutil(["-s", "/"])
}

/// Asks `mdutil` to erase and rebuild the index. Often needs an administrator.
pub fn reindex_spotlight() -> Result<String, String> {
    run_mdutil(["-E", "/"])
}

fn run_mdutil(args: [&str; 2]) -> Result<String, String> {
    let output = Command::new("mdutil")
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !err.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&err);
    }
    if output.status.success() {
        Ok(if text.is_empty() {
            "Spotlight command finished.".into()
        } else {
            text
        })
    } else {
        Err(if text.is_empty() {
            "Spotlight command failed.".into()
        } else {
            text
        })
    }
}
