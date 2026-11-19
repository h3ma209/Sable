use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use sysinfo::{
    Components, CpuRefreshKind, Disks, MemoryRefreshKind, Networks, Pid, ProcessRefreshKind,
    ProcessStatus, ProcessesToUpdate, RefreshKind, System, UpdateKind, Users,
    MINIMUM_CPU_UPDATE_INTERVAL,
};

struct MonitorState {
    system: System,
    networks: Networks,
    disks: Disks,
    users: Users,
    components: Components,
    prev_core_ticks: Option<Vec<[u32; 4]>>,
    last_sample_at: Option<Instant>,
    processes_initialized: bool,
    cpu_brand: String,
    os_version: String,
    host_name: String,
    perf_cores: usize,
    eff_cores: usize,
    physical_cores: usize,
}

impl MonitorState {
    fn new() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_memory(MemoryRefreshKind::everything())
                .with_cpu(CpuRefreshKind::everything()),
        );
        let cpu_brand = sysctl_string("machdep.cpu.brand_string")
            .or_else(|| {
                system
                    .cpus()
                    .first()
                    .map(|c| c.brand().trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "Unknown CPU".to_string());
        let os_name = System::name().unwrap_or_else(|| "macOS".into());
        let os_ver = System::os_version().unwrap_or_default();
        let os_version = if os_ver.is_empty() {
            os_name
        } else {
            format!("{os_name} {os_ver}")
        };
        let host_name = System::host_name().unwrap_or_else(|| "localhost".into());
        let perf_cores = sysctl_u32("hw.perflevel0.logicalcpu").unwrap_or(0) as usize;
        let eff_cores = sysctl_u32("hw.perflevel1.logicalcpu").unwrap_or(0) as usize;
        let physical_cores = system
            .physical_core_count()
            .unwrap_or_else(|| system.cpus().len());

        Self {
            system,
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            users: Users::new_with_refreshed_list(),
            components: Components::new_with_refreshed_list(),
            prev_core_ticks: None,
            last_sample_at: None,
            processes_initialized: false,
            cpu_brand,
            os_version,
            host_name,
            perf_cores,
            eff_cores,
            physical_cores,
        }
    }
}

fn monitor_state() -> &'static Mutex<MonitorState> {
    static STATE: OnceLock<Mutex<MonitorState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(MonitorState::new()))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CoreSnap {
    pub id: usize,
    pub user_pct: f32,
    pub sys_pct: f32,
    pub nice_pct: f32,
    pub total_pct: f32,
    pub freq_mhz: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryBreakdown {
    pub app_bytes: u64,
    pub wired_bytes: u64,
    pub compressed_bytes: u64,
    pub cached_bytes: u64,
    pub free_bytes: u64,
    pub active_bytes: u64,
    pub inactive_bytes: u64,
    pub pageins: u64,
    pub pageouts: u64,
    pub pressure_level: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetInterfaceSnap {
    pub name: String,
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
    pub total_rx: u64,
    pub total_tx: u64,
    pub ip: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkSnap {
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
    pub interfaces: Vec<NetInterfaceSnap>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiskVolumeSnap {
    pub name: String,
    pub mount_point: String,
    pub fs: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub is_removable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThermalSnap {
    pub label: String,
    pub temp_c: f32,
    pub max_c: Option<f32>,
    pub critical_c: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessSnap {
    pub name: String,
    pub pid: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub ppid: Option<u32>,
    pub user: String,
    pub status: String,
    pub virtual_bytes: u64,
    pub mem_percent: f32,
    pub threads: u32,
    pub nice: Option<i32>,
    pub run_time_secs: u64,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
    pub cmd: String,
    pub exe: Option<PathBuf>,
    pub is_root: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatterySnap {
    pub percent: f32,
    pub status: String,
    pub time_remaining: String,
    pub on_ac: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessGroupMember {
    pub proc: ProcessSnap,
    pub role: String,
    pub is_primary: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessGroup {
    pub key: String,
    pub name: String,
    pub is_app: bool,
    pub is_root: bool,
    pub bundle_path: Option<PathBuf>,
    pub icon_data_url: Option<String>,
    pub primary_pid: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub mem_percent: f32,
    pub total_threads: u32,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
    pub max_run_time_secs: u64,
    pub members: Vec<ProcessGroupMember>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VolumeBytes {
    pub total: u64,
    pub free: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
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
    pub cpu_brand: String,
    pub os_version: String,
    pub host_name: String,
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub perf_cores: usize,
    pub eff_cores: usize,
    pub cpu_user_pct: f32,
    pub cpu_sys_pct: f32,
    pub cpu_nice_pct: f32,
    pub cpu_idle_pct: f32,
    pub cores: Vec<CoreSnap>,
    pub memory: MemoryBreakdown,
    pub network: NetworkSnap,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
    pub volumes: Vec<DiskVolumeSnap>,
    pub thermals: Vec<ThermalSnap>,
    pub battery: Option<BatterySnap>,
    pub total_threads: u64,
    pub running_tasks: usize,
    pub processes: Vec<ProcessSnap>,
    pub groups: Vec<ProcessGroup>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessDetail {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub user: String,
    pub status: String,
    pub nice: Option<i32>,
    pub threads: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub virtual_bytes: u64,
    pub run_time_secs: u64,
    pub start_time: u64,
    pub cmd: String,
    pub exe: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub ancestry: Vec<(u32, String)>,
    pub children: Vec<(u32, String)>,
    pub sockets: Vec<String>,
    pub environ: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessSignal {
    Term,
    Kill,
    Stop,
    Cont,
    Hup,
}

impl ProcessSignal {
    pub fn label(self) -> &'static str {
        match self {
            Self::Term => "SIGTERM",
            Self::Kill => "SIGKILL",
            Self::Stop => "SIGSTOP",
            Self::Cont => "SIGCONT",
            Self::Hup => "SIGHUP",
        }
    }
}

fn format_status(status: ProcessStatus) -> String {
    match status {
        ProcessStatus::Run => "Run".into(),
        ProcessStatus::Sleep => "Sleep".into(),
        ProcessStatus::Idle => "Idle".into(),
        ProcessStatus::Stop => "Stop".into(),
        ProcessStatus::Zombie => "Zombie".into(),
        ProcessStatus::UninterruptibleDiskSleep => "Disk".into(),
        ProcessStatus::Tracing => "Trace".into(),
        ProcessStatus::Wakekill => "Wake".into(),
        ProcessStatus::Waking => "Waking".into(),
        ProcessStatus::Parked => "Parked".into(),
        ProcessStatus::LockBlocked => "Locked".into(),
        ProcessStatus::Dead => "Dead".into(),
        _ => "Other".into(),
    }
}

fn collect_processes(
    system: &System,
    users: &Users,
    total_memory: u64,
    dt_secs: f64,
) -> (Vec<ProcessSnap>, u64, usize) {
    let mut total_threads = 0u64;
    let mut running_tasks = 0usize;
    let mut list = Vec::with_capacity(system.processes().len());

    for (pid, proc_) in system.processes() {
        let pid_u32 = pid.as_u32();
        let (threads, nice) = read_task_threads_and_nice(pid_u32);
        total_threads = total_threads.saturating_add(threads as u64);

        let st = proc_.status();
        if st == ProcessStatus::Run {
            running_tasks += 1;
        }
        let user = proc_
            .user_id()
            .and_then(|uid| users.get_user_by_id(uid))
            .map(|u| u.name().to_string())
            .or_else(|| proc_.user_id().map(|uid| uid.to_string()))
            .unwrap_or_else(|| "-".into());
        let is_root = user == "root";

        let mem = proc_.memory();
        let mem_percent = if total_memory > 0 {
            (mem as f64 / total_memory as f64 * 100.0) as f32
        } else {
            0.0
        };
        let du = proc_.disk_usage();
        let disk_r = (du.read_bytes as f64 / dt_secs).round() as u64;
        let disk_w = (du.written_bytes as f64 / dt_secs).round() as u64;
        let cmd = proc_
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");

        list.push(ProcessSnap {
            name: proc_.name().to_string_lossy().into_owned(),
            pid: pid_u32,
            cpu_percent: proc_.cpu_usage(),
            memory_bytes: mem,
            ppid: proc_.parent().map(|p| p.as_u32()),
            user,
            status: format_status(st),
            virtual_bytes: proc_.virtual_memory(),
            mem_percent,
            threads,
            nice,
            run_time_secs: proc_.run_time(),
            disk_read_bytes_per_sec: disk_r,
            disk_write_bytes_per_sec: disk_w,
            cmd,
            exe: proc_.exe().map(Path::to_path_buf),
            is_root,
        });
    }

    (list, total_threads, running_tasks)
}

fn top_processes_from(all: &[ProcessSnap], by_cpu: bool, limit: usize) -> Vec<ProcessSnap> {
    let mut rows = all.to_vec();
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

#[cfg(target_os = "macos")]
fn sysctl_u32(key: &str) -> Option<u32> {
    use std::ffi::CString;
    let c_key = CString::new(key).ok()?;
    let mut val: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as libc::size_t;
    let res = unsafe {
        libc::sysctlbyname(
            c_key.as_ptr(),
            &mut val as *mut u32 as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if res == 0 {
        Some(val)
    } else {
        None
    }
}

#[cfg(not(target_os = "macos"))]
fn sysctl_u32(_key: &str) -> Option<u32> {
    None
}

#[cfg(target_os = "macos")]
fn sysctl_string(key: &str) -> Option<String> {
    use std::ffi::CString;
    let c_key = CString::new(key).ok()?;
    let mut size: libc::size_t = 0;
    let res = unsafe {
        libc::sysctlbyname(
            c_key.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if res != 0 || size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let res = unsafe {
        libc::sysctlbyname(
            c_key.as_ptr(),
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if res != 0 {
        return None;
    }
    let trimmed = buf
        .split(|&b| b == 0)
        .next()
        .unwrap_or(&[])
        .to_vec();
    let s = String::from_utf8_lossy(&trimmed).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(not(target_os = "macos"))]
fn sysctl_string(_key: &str) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn read_core_ticks() -> Option<Vec<[u32; 4]>> {
    let mut cpu_count: libc::natural_t = 0;
    let mut info_ptr: libc::processor_info_array_t = std::ptr::null_mut();
    let mut info_cnt: libc::mach_msg_type_number_t = 0;
    let host = unsafe { libc::mach_host_self() };
    let kr = unsafe {
        libc::host_processor_info(
            host,
            libc::PROCESSOR_CPU_LOAD_INFO,
            &mut cpu_count,
            &mut info_ptr,
            &mut info_cnt,
        )
    };
    if kr != libc::KERN_SUCCESS || info_ptr.is_null() {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(info_ptr as *const libc::integer_t, info_cnt as usize) };
    let n = cpu_count as usize;
    let stride = libc::CPU_STATE_MAX as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let base = i * stride;
        if base + stride <= slice.len() {
            out.push([
                slice[base + libc::CPU_STATE_USER as usize] as u32,
                slice[base + libc::CPU_STATE_SYSTEM as usize] as u32,
                slice[base + libc::CPU_STATE_IDLE as usize] as u32,
                slice[base + libc::CPU_STATE_NICE as usize] as u32,
            ]);
        }
    }
    unsafe {
        let _ = libc::vm_deallocate(
            libc::mach_task_self(),
            info_ptr as libc::vm_address_t,
            (info_cnt as usize * std::mem::size_of::<libc::integer_t>()) as libc::vm_size_t,
        );
    }
    Some(out)
}

#[cfg(not(target_os = "macos"))]
fn read_core_ticks() -> Option<Vec<[u32; 4]>> {
    None
}

fn compute_core_snaps(
    cpus: &[sysinfo::Cpu],
    prev_ticks: &mut Option<Vec<[u32; 4]>>,
) -> (Vec<CoreSnap>, f32, f32, f32, f32) {
    if let Some(curr) = read_core_ticks() {
        let mut cores = Vec::with_capacity(curr.len());
        let mut sum_user = 0.0f64;
        let mut sum_sys = 0.0f64;
        let mut sum_nice = 0.0f64;
        let mut sum_idle = 0.0f64;

        for (idx, c) in curr.iter().enumerate() {
            let freq_mhz = cpus.get(idx).map(|cpu| cpu.frequency()).unwrap_or(0);
            let (user_pct, sys_pct, nice_pct, idle_pct, total_pct) =
                if let Some(prev) = prev_ticks.as_ref().and_then(|v| v.get(idx)) {
                    let du = c[0].wrapping_sub(prev[0]) as f64;
                    let ds = c[1].wrapping_sub(prev[1]) as f64;
                    let di = c[2].wrapping_sub(prev[2]) as f64;
                    let dn = c[3].wrapping_sub(prev[3]) as f64;
                    let tot = du + ds + di + dn;
                    if tot > 0.0 {
                        let u = (du / tot * 100.0) as f32;
                        let s = (ds / tot * 100.0) as f32;
                        let n = (dn / tot * 100.0) as f32;
                        let idle = (di / tot * 100.0) as f32;
                        let t = ((du + ds + dn) / tot * 100.0).clamp(0.0, 100.0) as f32;
                        (u, s, n, idle, t)
                    } else {
                        (0.0, 0.0, 0.0, 100.0, 0.0)
                    }
                } else {
                    let t = cpus
                        .get(idx)
                        .map(|cpu| cpu.cpu_usage().clamp(0.0, 100.0))
                        .unwrap_or(0.0);
                    (t, 0.0, 0.0, (100.0 - t).max(0.0), t)
                };

            sum_user += user_pct as f64;
            sum_sys += sys_pct as f64;
            sum_nice += nice_pct as f64;
            sum_idle += idle_pct as f64;

            cores.push(CoreSnap {
                id: idx,
                user_pct,
                sys_pct,
                nice_pct,
                total_pct,
                freq_mhz,
            });
        }
        *prev_ticks = Some(curr);
        let n = cores.len().max(1) as f64;
        return (
            cores,
            (sum_user / n) as f32,
            (sum_sys / n) as f32,
            (sum_nice / n) as f32,
            (sum_idle / n) as f32,
        );
    }

    let cores: Vec<CoreSnap> = cpus
        .iter()
        .enumerate()
        .map(|(id, cpu)| {
            let total_pct = cpu.cpu_usage().clamp(0.0, 100.0);
            CoreSnap {
                id,
                user_pct: total_pct,
                sys_pct: 0.0,
                nice_pct: 0.0,
                total_pct,
                freq_mhz: cpu.frequency(),
            }
        })
        .collect();
    let n = cores.len().max(1) as f32;
    let avg = cores.iter().map(|c| c.total_pct).sum::<f32>() / n;
    (cores, avg, 0.0, 0.0, (100.0 - avg).max(0.0))
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn read_memory_breakdown() -> Option<MemoryBreakdown> {
    let page_raw = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page_size = if page_raw > 0 { page_raw as u64 } else { 16384 };
    let mut vm: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let kr = unsafe {
        libc::host_statistics64(
            libc::mach_host_self(),
            libc::HOST_VM_INFO64,
            &mut vm as *mut libc::vm_statistics64 as libc::host_info64_t,
            &mut count,
        )
    };
    if kr != libc::KERN_SUCCESS {
        return None;
    }

    let internal = vm.internal_page_count as u64;
    let purgeable = vm.purgeable_count as u64;
    let wired = (vm.wire_count as u64).saturating_mul(page_size);
    let compressed = (vm.compressor_page_count as u64).saturating_mul(page_size);
    let app_bytes = internal.saturating_sub(purgeable).saturating_mul(page_size);
    let cached_bytes = (vm.external_page_count as u64)
        .saturating_add(purgeable)
        .saturating_mul(page_size);
    let free_bytes = (vm.free_count as u64)
        .saturating_sub(vm.speculative_count as u64)
        .saturating_mul(page_size);
    let active_bytes = (vm.active_count as u64).saturating_mul(page_size);
    let inactive_bytes = (vm.inactive_count as u64).saturating_mul(page_size);
    let pressure_level = sysctl_u32("kern.memorystatus_vm_pressure_level")
        .unwrap_or(1)
        .min(255) as u8;

    Some(MemoryBreakdown {
        app_bytes,
        wired_bytes: wired,
        compressed_bytes: compressed,
        cached_bytes,
        free_bytes,
        active_bytes,
        inactive_bytes,
        pageins: vm.pageins as u64,
        pageouts: vm.pageouts as u64,
        pressure_level,
    })
}

#[cfg(not(target_os = "macos"))]
fn read_memory_breakdown() -> Option<MemoryBreakdown> {
    None
}

#[cfg(target_os = "macos")]
fn read_task_threads_and_nice(pid: u32) -> (u32, Option<i32>) {
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    let ret = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTASKINFO,
            0,
            &mut info as *mut libc::proc_taskinfo as *mut libc::c_void,
            size,
        )
    };
    let threads = if ret == size && info.pti_threadnum > 0 {
        info.pti_threadnum as u32
    } else {
        0
    };

    let nice = unsafe {
        let err_ptr = libc::__error();
        if !err_ptr.is_null() {
            *err_ptr = 0;
        }
        let prio = libc::getpriority(libc::PRIO_PROCESS, pid as libc::id_t);
        if prio == -1 && !err_ptr.is_null() && *err_ptr != 0 {
            None
        } else {
            Some(prio)
        }
    };
    (threads, nice)
}

#[cfg(not(target_os = "macos"))]
fn read_task_threads_and_nice(_pid: u32) -> (u32, Option<i32>) {
    (0, None)
}

fn collect_network_snap(networks: &Networks, dt_secs: f64) -> NetworkSnap {
    let mut interfaces = Vec::new();
    let mut total_rx_rate = 0u64;
    let mut total_tx_rate = 0u64;
    let mut total_rx_all = 0u64;
    let mut total_tx_all = 0u64;

    for (name, data) in networks.list() {
        let rx_delta = data.received();
        let tx_delta = data.transmitted();
        let rx_rate = (rx_delta as f64 / dt_secs).round() as u64;
        let tx_rate = (tx_delta as f64 / dt_secs).round() as u64;
        let tot_rx = data.total_received();
        let tot_tx = data.total_transmitted();
        let ip = data
            .ip_networks()
            .iter()
            .find(|n| n.addr.is_ipv4())
            .or_else(|| data.ip_networks().first())
            .map(|n| n.addr.to_string())
            .unwrap_or_default();

        if !name.starts_with("lo") {
            total_rx_rate = total_rx_rate.saturating_add(rx_rate);
            total_tx_rate = total_tx_rate.saturating_add(tx_rate);
            total_rx_all = total_rx_all.saturating_add(tot_rx);
            total_tx_all = total_tx_all.saturating_add(tot_tx);
        }

        if tot_rx > 0 || tot_tx > 0 || !ip.is_empty() {
            interfaces.push(NetInterfaceSnap {
                name: name.clone(),
                rx_bytes_per_sec: rx_rate,
                tx_bytes_per_sec: tx_rate,
                total_rx: tot_rx,
                total_tx: tot_tx,
                ip,
            });
        }
    }
    interfaces.sort_by(|a, b| {
        (b.rx_bytes_per_sec + b.tx_bytes_per_sec)
            .cmp(&(a.rx_bytes_per_sec + a.tx_bytes_per_sec))
            .then_with(|| (b.total_rx + b.total_tx).cmp(&(a.total_rx + a.total_tx)))
    });
    interfaces.truncate(8);

    NetworkSnap {
        rx_bytes_per_sec: total_rx_rate,
        tx_bytes_per_sec: total_tx_rate,
        total_rx_bytes: total_rx_all,
        total_tx_bytes: total_tx_all,
        interfaces,
    }
}

fn collect_volumes(disks: &Disks) -> Vec<DiskVolumeSnap> {
    let mut seen_mounts = HashSet::new();
    let mut out = Vec::new();
    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy().into_owned();
        if mount.starts_with("/System/Volumes/Preboot")
            || mount.starts_with("/System/Volumes/VM")
            || mount.starts_with("/System/Volumes/Update")
            || mount.starts_with("/System/Volumes/xarts")
            || mount.starts_with("/System/Volumes/iSCPreboot")
            || mount.starts_with("/System/Volumes/Hardware")
            || mount == "/dev"
        {
            continue;
        }
        let total = disk.total_space();
        if total == 0 || !seen_mounts.insert(mount.clone()) {
            continue;
        }
        let name = disk.name().to_string_lossy().trim().to_string();
        out.push(DiskVolumeSnap {
            name: if name.is_empty() { mount.clone() } else { name },
            mount_point: mount,
            fs: disk.file_system().to_string_lossy().into_owned(),
            total_bytes: total,
            available_bytes: disk.available_space(),
            is_removable: disk.is_removable(),
        });
    }
    out.sort_by(|a, b| a.mount_point.len().cmp(&b.mount_point.len()));
    out.truncate(6);
    out
}

fn collect_thermals(components: &Components) -> Vec<ThermalSnap> {
    let mut out = Vec::new();
    for comp in components.list() {
        if let Some(temp_c) = comp.temperature() {
            if temp_c.is_finite() && temp_c > 0.0 && temp_c < 140.0 {
                out.push(ThermalSnap {
                    label: comp.label().to_string(),
                    temp_c,
                    max_c: comp.max().filter(|v| v.is_finite() && *v > 0.0),
                    critical_c: comp.critical().filter(|v| v.is_finite() && *v > 0.0),
                });
            }
        }
    }
    out.truncate(12);
    out
}

/// Live machine telemetry (per-core CPU, memory pressure, network/disk rates, process table).
pub fn machine_sample() -> MachineSample {
    machine_sample_inner(true)
}

/// Fast CPU + memory sample without enumerating all processes (used on Care startup).
pub fn machine_summary_sample() -> MachineSample {
    machine_sample_inner(false)
}

fn proc_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_user(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_exe(UpdateKind::OnlyIfNotSet)
}

pub fn parse_pmset_battery(output: &str) -> Option<BatterySnap> {
    let mut on_ac = false;
    let mut percent: Option<f32> = None;
    let mut status = String::new();
    let mut time_remaining = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("'AC Power'") {
            on_ac = true;
        }
        if let Some(pct_idx) = trimmed.find('%') {
            let before = &trimmed[..pct_idx];
            let num_str: String = before
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if let Ok(p) = num_str.parse::<f32>() {
                percent = Some(p.clamp(0.0, 100.0));
                let after = trimmed[pct_idx + 1..].trim_start_matches([';', ' ', '\t']);
                let parts: Vec<&str> = after.split(';').map(str::trim).filter(|s| !s.is_empty()).collect();
                if let Some(raw_st) = parts.first() {
                    let lower = raw_st.to_ascii_lowercase();
                    status = if lower.contains("discharging") {
                        "Discharging".into()
                    } else if lower.contains("finishing charge") {
                        "Finishing".into()
                    } else if lower.contains("charging") {
                        "Charging".into()
                    } else if lower.contains("charged") {
                        "Charged".into()
                    } else if lower.contains("ac attached") {
                        "AC Power".into()
                    } else {
                        (*raw_st).to_string()
                    };
                }
                for part in parts.iter().skip(1) {
                    if let Some(rem) = part.split_whitespace().next() {
                        if rem.contains(':') && rem != "0:00" {
                            time_remaining = rem.to_string();
                            break;
                        }
                    }
                }
                break;
            }
        }
    }

    let pct = percent?;
    if status.is_empty() {
        status = if on_ac { "AC Power".into() } else { "Battery".into() };
    }
    Some(BatterySnap {
        percent: pct,
        status,
        time_remaining,
        on_ac,
    })
}

fn read_battery_cached() -> Option<BatterySnap> {
    static BATTERY_CACHE: OnceLock<Mutex<(Option<BatterySnap>, Option<Instant>)>> = OnceLock::new();
    let cell = BATTERY_CACHE.get_or_init(|| Mutex::new((None, None)));
    {
        if let Ok(guard) = cell.lock() {
            if let Some(at) = guard.1 {
                if at.elapsed() < Duration::from_secs(15) {
                    return guard.0.clone();
                }
            }
        }
    }
    let output = Command::new("/usr/bin/pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    let snap = if output.status.success() {
        parse_pmset_battery(&String::from_utf8_lossy(&output.stdout))
    } else {
        None
    };
    if let Ok(mut guard) = cell.lock() {
        guard.0 = snap.clone();
        guard.1 = Some(Instant::now());
    }
    snap
}

pub fn app_bundle_from_exe(exe: &Path) -> Option<(String, PathBuf)> {
    let mut acc = PathBuf::new();
    for comp in exe.components() {
        acc.push(comp);
        if let Component::Normal(seg) = comp {
            let s = seg.to_string_lossy();
            if s.len() > 4 && s[s.len() - 4..].eq_ignore_ascii_case(".app") {
                let name = s[..s.len() - 4].to_string();
                return Some((name, acc));
            }
        }
    }
    None
}

pub fn infer_process_role(proc: &ProcessSnap, is_primary: bool) -> &'static str {
    if is_primary {
        return "Main";
    }
    let name_l = proc.name.to_ascii_lowercase();
    let cmd_l = proc.cmd.to_ascii_lowercase();
    if name_l.contains("renderer") || cmd_l.contains("--type=renderer") || cmd_l.contains("helper (renderer)") {
        "Renderer"
    } else if name_l.contains("gpu") || cmd_l.contains("--type=gpu") {
        "GPU"
    } else if name_l.contains("network") || cmd_l.contains("utility-sub-type=network") {
        "Network"
    } else if name_l.contains("plugin") || cmd_l.contains("--type=plugin") {
        "Plugin"
    } else if cmd_l.contains(".appex/") || name_l.contains("extension") {
        "Extension"
    } else if name_l.contains("helper") {
        "Helper"
    } else if [
        "rust-analyzer",
        "gopls",
        "language_server",
        "tsserver",
        "pyright",
        "clangd",
        "sourcekit-lsp",
        "lsp",
    ]
    .iter()
    .any(|k| name_l.contains(k) || cmd_l.contains(k))
    {
        "LSP"
    } else if matches!(name_l.as_str(), "zsh" | "bash" | "fish" | "sh" | "tcsh") {
        "Shell"
    } else if matches!(
        name_l.as_str(),
        "node" | "python" | "python3" | "cargo" | "rustc" | "go" | "git" | "rg" | "esbuild" | "bun" | "deno"
    ) {
        "Tool"
    } else {
        "Subprocess"
    }
}

fn normalized_fallback_group(proc: &ProcessSnap) -> (String, String) {
    let raw = proc.name.trim();
    if raw.starts_with("com.apple.WebKit") {
        return ("fallback:webkit".into(), "WebKit".into());
    }
    let mut clean = raw.to_string();
    for suffix in [
        " Helper (Renderer)",
        " Helper (GPU)",
        " Helper (Plugin)",
        " Helper (Alerts)",
        " Helper",
        " Networking",
        " Web Content",
    ] {
        if let Some(stripped) = clean.strip_suffix(suffix) {
            if !stripped.trim().is_empty() {
                clean = stripped.trim().to_string();
                break;
            }
        }
    }
    (format!("proc:{}", clean.to_ascii_lowercase()), clean)
}

pub fn group_processes(processes: &[ProcessSnap], total_memory: u64) -> Vec<ProcessGroup> {
    if processes.is_empty() {
        return Vec::new();
    }
    let by_pid: HashMap<u32, &ProcessSnap> = processes.iter().map(|p| (p.pid, p)).collect();
    let direct_app: HashMap<u32, (String, PathBuf)> = processes
        .iter()
        .filter_map(|p| {
            p.exe
                .as_deref()
                .and_then(app_bundle_from_exe)
                .map(|app| (p.pid, app))
        })
        .collect();

    struct Bucket {
        key: String,
        name: String,
        is_app: bool,
        bundle_path: Option<PathBuf>,
        procs: Vec<ProcessSnap>,
    }

    let mut buckets: HashMap<String, Bucket> = HashMap::new();

    for p in processes {
        let mut resolved = direct_app.get(&p.pid).cloned();
        if resolved.is_none() {
            let mut visited = HashSet::from([p.pid]);
            let mut cur = p.ppid;
            for _ in 0..12 {
                let Some(ppid) = cur else { break };
                if ppid <= 1 || !visited.insert(ppid) {
                    break;
                }
                if let Some(app) = direct_app.get(&ppid) {
                    resolved = Some(app.clone());
                    break;
                }
                cur = by_pid.get(&ppid).and_then(|parent| parent.ppid);
            }
        }

        let (key, name, is_app, bundle_path) = if let Some((app_name, bundle)) = resolved {
            let k = format!("app:{}", bundle.to_string_lossy().to_ascii_lowercase());
            (k, app_name, true, Some(bundle))
        } else {
            let (k, n) = normalized_fallback_group(p);
            (k, n, false, None)
        };

        buckets
            .entry(key.clone())
            .or_insert_with(|| Bucket {
                key,
                name,
                is_app,
                bundle_path,
                procs: Vec::new(),
            })
            .procs
            .push(p.clone());
    }

    let mut groups: Vec<ProcessGroup> = buckets
        .into_values()
        .map(|b| {
            let group_pids: HashSet<u32> = b.procs.iter().map(|p| p.pid).collect();
            let primary_pid = b
                .procs
                .iter()
                .filter(|p| {
                    let parent_outside = p.ppid.is_none_or(|pp| !group_pids.contains(&pp));
                    let not_helper_exe = p.exe.as_ref().is_none_or(|e| {
                        let s = e.to_string_lossy();
                        !s.contains("/Frameworks/") && !s.contains("Helper")
                    });
                    parent_outside && not_helper_exe
                })
                .min_by_key(|p| p.pid)
                .or_else(|| b.procs.iter().max_by_key(|p| p.memory_bytes))
                .map(|p| p.pid)
                .unwrap_or(0);

            let mut cpu_sum = 0.0f32;
            let mut mem_sum = 0u64;
            let mut threads_sum = 0u32;
            let mut dr_sum = 0u64;
            let mut dw_sum = 0u64;
            let mut max_rt = 0u64;
            let mut is_root = !b.procs.is_empty() && b.procs.iter().all(|p| p.is_root);

            let mut members: Vec<ProcessGroupMember> = b
                .procs
                .into_iter()
                .map(|p| {
                    cpu_sum += p.cpu_percent;
                    mem_sum = mem_sum.saturating_add(p.memory_bytes);
                    threads_sum = threads_sum.saturating_add(p.threads);
                    dr_sum = dr_sum.saturating_add(p.disk_read_bytes_per_sec);
                    dw_sum = dw_sum.saturating_add(p.disk_write_bytes_per_sec);
                    max_rt = max_rt.max(p.run_time_secs);
                    if !p.is_root {
                        is_root = false;
                    }
                    let is_primary = p.pid == primary_pid;
                    let role = infer_process_role(&p, is_primary).to_string();
                    ProcessGroupMember {
                        proc: p,
                        role,
                        is_primary,
                    }
                })
                .collect();

            members.sort_by(|a, c| {
                c.is_primary
                    .cmp(&a.is_primary)
                    .then_with(|| c.proc.memory_bytes.cmp(&a.proc.memory_bytes))
                    .then_with(|| {
                        c.proc
                            .cpu_percent
                            .partial_cmp(&a.proc.cpu_percent)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });

            let mem_percent = if total_memory > 0 {
                (mem_sum as f64 / total_memory as f64 * 100.0) as f32
            } else {
                0.0
            };
            let icon_data_url = b
                .bundle_path
                .as_deref()
                .and_then(crate::apps::cached_bundle_icon);

            ProcessGroup {
                key: b.key,
                name: b.name,
                is_app: b.is_app,
                is_root,
                bundle_path: b.bundle_path,
                icon_data_url,
                primary_pid,
                cpu_percent: cpu_sum,
                memory_bytes: mem_sum,
                mem_percent,
                total_threads: threads_sum,
                disk_read_bytes_per_sec: dr_sum,
                disk_write_bytes_per_sec: dw_sum,
                max_run_time_secs: max_rt,
                members,
            }
        })
        .collect();

    groups.sort_by(|a, b| {
        b.memory_bytes
            .cmp(&a.memory_bytes)
            .then_with(|| {
                b.cpu_percent
                    .partial_cmp(&a.cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    groups
}

fn machine_sample_inner(include_processes: bool) -> MachineSample {
    let battery = read_battery_cached();
    let mut st = monitor_state()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let pk = proc_refresh_kind();

    let need_warmup = st.last_sample_at.is_none() || (include_processes && !st.processes_initialized);
    if need_warmup {
        if st.last_sample_at.is_none() {
            st.prev_core_ticks = read_core_ticks();
            st.system.refresh_cpu_usage();
            st.networks.refresh(true);
        }
        if include_processes && !st.processes_initialized {
            st.system.refresh_processes_specifics(ProcessesToUpdate::All, true, pk);
        }
        std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
        st.last_sample_at = Some(Instant::now() - MINIMUM_CPU_UPDATE_INTERVAL);
    }

    let dt_secs = st
        .last_sample_at
        .map(|t| t.elapsed().as_secs_f64())
        .unwrap_or(1.0)
        .max(0.1);
    st.last_sample_at = Some(Instant::now());

    st.system.refresh_memory_specifics(MemoryRefreshKind::everything());
    st.system.refresh_cpu_usage();
    st.networks.refresh(true);
    st.disks.refresh(true);
    st.components.refresh(true);
    if include_processes {
        st.users.refresh();
        st.system.refresh_processes_specifics(ProcessesToUpdate::All, true, pk);
        st.processes_initialized = true;
    }

    let total_memory = st.system.total_memory();
    let available_memory = st.system.available_memory();
    let breakdown = read_memory_breakdown().unwrap_or_else(|| {
        let used = st.system.used_memory();
        MemoryBreakdown {
            app_bytes: used,
            free_bytes: st.system.free_memory(),
            pressure_level: 1,
            ..MemoryBreakdown::default()
        }
    });
    let used_memory = if breakdown.app_bytes > 0 || breakdown.wired_bytes > 0 {
        breakdown
            .app_bytes
            .saturating_add(breakdown.wired_bytes)
            .saturating_add(breakdown.compressed_bytes)
            .min(total_memory.max(1))
    } else {
        st.system.used_memory()
    };

    let (cores, cpu_user_pct, cpu_sys_pct, cpu_nice_pct, cpu_idle_pct) = {
        let state_ref = &mut *st;
        compute_core_snaps(state_ref.system.cpus(), &mut state_ref.prev_core_ticks)
    };
    let cpu_percent = (cpu_user_pct + cpu_sys_pct + cpu_nice_pct)
        .clamp(0.0, 100.0);
    let logical_cores = cores.len();
    let (load_avg_1m, load_avg_5m, load_avg_15m) = load_averages();

    let (processes, total_threads, running_tasks) = if include_processes {
        collect_processes(&st.system, &st.users, total_memory, dt_secs)
    } else {
        (Vec::new(), 0, 0)
    };

    let top_cpu = top_processes_from(&processes, true, 12);
    let top_mem = top_processes_from(&processes, false, 12);
    let disk_read_bytes_per_sec = processes
        .iter()
        .fold(0u64, |acc, p| acc.saturating_add(p.disk_read_bytes_per_sec));
    let disk_write_bytes_per_sec = processes
        .iter()
        .fold(0u64, |acc, p| acc.saturating_add(p.disk_write_bytes_per_sec));

    let network = collect_network_snap(&st.networks, dt_secs);
    let volumes = collect_volumes(&st.disks);
    let thermals = collect_thermals(&st.components);
    let boot_volume = boot_volume_bytes();
    let proc_count = if include_processes { processes.len() } else { 0 };
    let swap_used = st.system.used_swap();
    let swap_total = st.system.total_swap();
    let cpu_brand = st.cpu_brand.clone();
    let os_version = st.os_version.clone();
    let host_name = st.host_name.clone();
    let physical_cores = st.physical_cores;
    let perf_cores = st.perf_cores;
    let eff_cores = st.eff_cores;
    drop(st);

    let groups = if include_processes {
        group_processes(&processes, total_memory)
    } else {
        Vec::new()
    };

    MachineSample {
        used_memory,
        total_memory,
        available_memory,
        cpu_percent,
        swap_used,
        swap_total,
        process_count: proc_count,
        uptime_secs: System::uptime(),
        load_avg_1m,
        load_avg_5m,
        load_avg_15m,
        boot_volume,
        top_cpu,
        top_mem,
        cpu_brand,
        os_version,
        host_name,
        physical_cores,
        logical_cores,
        perf_cores,
        eff_cores,
        cpu_user_pct,
        cpu_sys_pct,
        cpu_nice_pct,
        cpu_idle_pct,
        cores,
        memory: breakdown,
        network,
        disk_read_bytes_per_sec,
        disk_write_bytes_per_sec,
        volumes,
        thermals,
        battery,
        total_threads,
        running_tasks,
        processes,
        groups,
    }
}

pub fn signal_process(pid: u32, signal: ProcessSignal) -> Result<String, String> {
    if pid == 0 || pid == 1 {
        return Err(format!("Refusing to send {} to critical system PID {pid}.", signal.label()));
    }
    if pid == std::process::id() {
        return Err(format!("Refusing to send {} to Sable itself.", signal.label()));
    }
    let sig = match signal {
        ProcessSignal::Term => libc::SIGTERM,
        ProcessSignal::Kill => libc::SIGKILL,
        ProcessSignal::Stop => libc::SIGSTOP,
        ProcessSignal::Cont => libc::SIGCONT,
        ProcessSignal::Hup => libc::SIGHUP,
    };
    let rc = unsafe { libc::kill(pid as libc::pid_t, sig) };
    if rc == 0 {
        Ok(format!("Sent {} to PID {pid}.", signal.label()))
    } else {
        let err = std::io::Error::last_os_error();
        let hint = match err.raw_os_error() {
            Some(libc::EPERM) => "Permission denied — owned by another user or protected by SIP.",
            Some(libc::ESRCH) => "Process already exited.",
            _ => "OS rejected signal.",
        };
        Err(format!("Failed to send {} to PID {pid}: {hint} ({err})", signal.label()))
    }
}

pub fn signal_process_group(pids: &[u32], signal: ProcessSignal) -> Result<String, String> {
    let self_pid = std::process::id();
    let mut seen = HashSet::new();
    let mut ok_count = 0usize;
    let mut last_err: Option<String> = None;

    for &pid in pids {
        if pid <= 1 || pid == self_pid || !seen.insert(pid) {
            continue;
        }
        match signal_process(pid, signal) {
            Ok(_) => ok_count += 1,
            Err(e) => last_err = Some(e),
        }
    }

    if ok_count > 0 {
        Ok(format!("Sent {} to {ok_count} processes.", signal.label()))
    } else {
        Err(last_err.unwrap_or_else(|| format!("No eligible processes to send {}.", signal.label())))
    }
}

pub fn renice_process(pid: u32, nice: i32) -> Result<String, String> {
    if pid == 0 || pid == 1 {
        return Err(format!("Refusing to renice critical system PID {pid}."));
    }
    let clamped = nice.clamp(-20, 20);
    let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, clamped) };
    if rc == 0 {
        Ok(format!("Set priority of PID {pid} to {clamped}."))
    } else {
        let err = std::io::Error::last_os_error();
        let hint = match err.raw_os_error() {
            Some(libc::EPERM) | Some(libc::EACCES) => {
                "Permission denied — raising priority below 0 requires root."
            }
            Some(libc::ESRCH) => "Process already exited.",
            _ => "Failed to change priority.",
        };
        Err(format!("Could not renice PID {pid}: {hint} ({err})"))
    }
}

fn should_redact_env_key(key: &str) -> bool {
    let up = key.to_ascii_uppercase();
    ["SECRET", "TOKEN", "KEY", "PASS", "AUTH", "CREDENTIAL", "COOKIE", "PRIVATE"]
        .iter()
        .any(|needle| up.contains(needle))
}

pub fn redact_env_pair(key: &str, val: &str) -> (String, String) {
    if should_redact_env_key(key) {
        (key.to_string(), "•••• (redacted)".to_string())
    } else {
        (key.to_string(), val.to_string())
    }
}

pub fn inspect_process_details(pid: u32) -> Option<ProcessDetail> {
    let mut detail = {
        let mut st = monitor_state()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let target_pid = Pid::from_u32(pid);
        st.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[target_pid]),
            false,
            ProcessRefreshKind::everything(),
        );
        let proc_ = st.system.process(target_pid)?;

        let (threads, nice) = read_task_threads_and_nice(pid);
        let user = proc_
            .user_id()
            .and_then(|uid| st.users.get_user_by_id(uid))
            .map(|u| u.name().to_string())
            .or_else(|| proc_.user_id().map(|uid| uid.to_string()))
            .unwrap_or_else(|| "-".into());
        let cmd = proc_
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");

        let mut ancestry = Vec::new();
        let mut visited = HashSet::from([pid]);
        let mut cursor = proc_.parent();
        for _ in 0..16 {
            let Some(p) = cursor else {
                break;
            };
            let pu = p.as_u32();
            if !visited.insert(pu) {
                break;
            }
            if let Some(parent_proc) = st.system.process(p) {
                ancestry.push((pu, parent_proc.name().to_string_lossy().into_owned()));
                cursor = parent_proc.parent();
            } else {
                break;
            }
        }
        ancestry.reverse();

        let mut children: Vec<(u32, String)> = st
            .system
            .processes()
            .iter()
            .filter_map(|(cpid, cp)| {
                if cp.parent() == Some(target_pid) {
                    Some((cpid.as_u32(), cp.name().to_string_lossy().into_owned()))
                } else {
                    None
                }
            })
            .collect();
        children.sort_by_key(|(cpid, _)| *cpid);
        children.truncate(40);

        let mut environ = Vec::new();
        for raw in proc_.environ().iter().take(50) {
            let text = raw.to_string_lossy();
            if let Some((k, v)) = text.split_once('=') {
                environ.push(redact_env_pair(k, v));
            }
        }

        ProcessDetail {
            pid,
            ppid: proc_.parent().map(|p| p.as_u32()),
            name: proc_.name().to_string_lossy().into_owned(),
            user,
            status: format_status(proc_.status()),
            nice,
            threads,
            cpu_percent: proc_.cpu_usage(),
            memory_bytes: proc_.memory(),
            virtual_bytes: proc_.virtual_memory(),
            run_time_secs: proc_.run_time(),
            start_time: proc_.start_time(),
            cmd,
            exe: proc_.exe().map(Path::to_path_buf),
            cwd: proc_.cwd().map(Path::to_path_buf),
            ancestry,
            children,
            sockets: Vec::new(),
            environ,
        }
    };

    // Run lsof strictly outside monitor_state mutex!
    if let Ok(output) = Command::new("lsof")
        .args(["-nP", "-a", "-p", &pid.to_string(), "-i"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            detail.sockets = stdout
                .lines()
                .skip(1)
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .take(40)
                .map(str::to_string)
                .collect();
        }
    }

    Some(detail)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_sample_populates_cores_memory_and_processes() {
        let sample = machine_sample();
        assert!(sample.logical_cores > 0, "logical_cores should be > 0");
        assert_eq!(sample.cores.len(), sample.logical_cores);
        assert!(sample.total_memory > 0, "total_memory should be > 0");
        assert!(!sample.processes.is_empty(), "processes should be populated");
        assert!(sample.top_cpu.len() <= 12);
        assert!(sample.memory.app_bytes > 0 || sample.used_memory > 0);
    }

    #[test]
    fn signal_process_refuses_protected_pids() {
        assert!(signal_process(0, ProcessSignal::Term).is_err());
        assert!(signal_process(1, ProcessSignal::Kill).is_err());
        assert!(signal_process(std::process::id(), ProcessSignal::Term).is_err());
        assert!(renice_process(0, 10).is_err());
        assert!(renice_process(1, 10).is_err());
    }

    #[test]
    fn inspect_process_details_redacts_secrets() {
        let (k, v) = redact_env_pair("OPENAI_API_KEY", "sk-12345");
        assert_eq!(k, "OPENAI_API_KEY");
        assert_eq!(v, "•••• (redacted)");

        let (_, token_val) = redact_env_pair("db_auth_token", "secret");
        assert_eq!(token_val, "•••• (redacted)");

        let (_, safe_val) = redact_env_pair("HOME", "/Users/test");
        assert_eq!(safe_val, "/Users/test");

        let _ = machine_sample();
        let me = inspect_process_details(std::process::id()).expect("current pid should inspect");
        assert_eq!(me.pid, std::process::id());
        assert!(!me.name.is_empty());
    }

    #[test]
    fn parse_pmset_battery_variants() {
        let charged = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=7471203)\t100%; charged; 0:00 remaining present: true\n";
        let b1 = parse_pmset_battery(charged).unwrap();
        assert!(b1.on_ac);
        assert_eq!(b1.percent, 100.0);
        assert_eq!(b1.status, "Charged");
        assert!(b1.time_remaining.is_empty());

        let discharging = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=7471203)\t76%; discharging; 4:20 remaining present: true\n";
        let b2 = parse_pmset_battery(discharging).unwrap();
        assert!(!b2.on_ac);
        assert_eq!(b2.percent, 76.0);
        assert_eq!(b2.status, "Discharging");
        assert_eq!(b2.time_remaining, "4:20");

        let charging = "Now drawing from 'AC Power'\n -InternalBattery-0\t42%; charging; 1:15 remaining present: true\n";
        let b3 = parse_pmset_battery(charging).unwrap();
        assert!(b3.on_ac);
        assert_eq!(b3.status, "Charging");
        assert_eq!(b3.time_remaining, "1:15");

        assert!(parse_pmset_battery("").is_none());
    }

    #[test]
    fn group_processes_groups_app_bundle_and_spawned_children() {
        let procs = vec![
            ProcessSnap {
                pid: 100,
                ppid: Some(1),
                name: "zed".into(),
                cpu_percent: 4.0,
                memory_bytes: 300_000_000,
                threads: 12,
                exe: Some(PathBuf::from("/Applications/Zed.app/Contents/MacOS/zed")),
                ..ProcessSnap::default()
            },
            ProcessSnap {
                pid: 101,
                ppid: Some(100),
                name: "zed".into(),
                cpu_percent: 1.5,
                memory_bytes: 100_000_000,
                threads: 4,
                exe: Some(PathBuf::from("/Applications/Zed.app/Contents/MacOS/zed")),
                ..ProcessSnap::default()
            },
            ProcessSnap {
                pid: 102,
                ppid: Some(100),
                name: "gopls".into(),
                cpu_percent: 2.0,
                memory_bytes: 200_000_000,
                threads: 8,
                exe: Some(PathBuf::from("/Users/test/go/bin/gopls")),
                ..ProcessSnap::default()
            },
            ProcessSnap {
                pid: 200,
                ppid: Some(1),
                name: "Antigravity".into(),
                cpu_percent: 3.0,
                memory_bytes: 400_000_000,
                exe: Some(PathBuf::from("/Applications/Antigravity.app/Contents/MacOS/Antigravity")),
                ..ProcessSnap::default()
            },
            ProcessSnap {
                pid: 201,
                ppid: Some(200),
                name: "Antigravity Helper (Renderer)".into(),
                cpu_percent: 5.0,
                memory_bytes: 500_000_000,
                exe: Some(PathBuf::from("/Applications/Antigravity.app/Contents/Frameworks/Antigravity Helper (Renderer).app/Contents/MacOS/Antigravity Helper (Renderer)")),
                ..ProcessSnap::default()
            },
            ProcessSnap {
                pid: 202,
                ppid: Some(200),
                name: "node".into(),
                cpu_percent: 1.0,
                memory_bytes: 100_000_000,
                exe: Some(PathBuf::from("/opt/homebrew/bin/node")),
                ..ProcessSnap::default()
            },
            ProcessSnap {
                pid: 301,
                ppid: Some(1),
                name: "mdworker_shared".into(),
                cpu_percent: 0.5,
                memory_bytes: 20_000_000,
                ..ProcessSnap::default()
            },
            ProcessSnap {
                pid: 302,
                ppid: Some(1),
                name: "mdworker_shared".into(),
                cpu_percent: 0.5,
                memory_bytes: 30_000_000,
                ..ProcessSnap::default()
            },
        ];

        let groups = group_processes(&procs, 10_000_000_000);
        assert_eq!(groups.len(), 3);

        let ag = groups.iter().find(|g| g.name == "Antigravity").expect("Antigravity group");
        assert!(ag.is_app);
        assert_eq!(ag.members.len(), 3);
        assert_eq!(ag.primary_pid, 200);
        assert_eq!(ag.memory_bytes, 1_000_000_000);
        assert!((ag.cpu_percent - 9.0).abs() < 0.01);
        assert_eq!(ag.members.iter().find(|m| m.proc.pid == 201).unwrap().role, "Renderer");
        assert_eq!(ag.members.iter().find(|m| m.proc.pid == 202).unwrap().role, "Tool");

        let zed = groups.iter().find(|g| g.name == "Zed").expect("Zed group");
        assert!(zed.is_app);
        assert_eq!(zed.members.len(), 3);
        assert_eq!(zed.primary_pid, 100);
        assert_eq!(zed.memory_bytes, 600_000_000);
        assert!((zed.cpu_percent - 7.5).abs() < 0.01);
        assert_eq!(zed.total_threads, 24);
        assert_eq!(zed.members.iter().find(|m| m.proc.pid == 100).unwrap().role, "Main");
        assert_eq!(zed.members.iter().find(|m| m.proc.pid == 102).unwrap().role, "LSP");

        let md = groups.iter().find(|g| g.name == "mdworker_shared").expect("mdworker group");
        assert!(!md.is_app);
        assert_eq!(md.members.len(), 2);
        assert_eq!(md.memory_bytes, 50_000_000);
    }
}
