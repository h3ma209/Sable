use dioxus::prelude::*;

use engine::{
    HostPoint, MachineSample, MonitorAnalytics, MonitorBucket, ProcessAggregate, ProcessGroup,
    ProcessPoint,
};
use crate::util::{
    format_bytes, format_count, format_proc_time, format_rate, format_unix_time, format_uptime,
    gauge_class, monitor_area_path, monitor_chart_max, monitor_dynamic_sparkline_points,
    monitor_health_label, monitor_health_score, monitor_sparkline_points,
};

pub fn cpu_heat_class(cpu_pct: f32) -> &'static str {
    if cpu_pct >= 80.0 {
        "val-hot"
    } else if cpu_pct >= 25.0 {
        "val-warm"
    } else {
        ""
    }
}

pub fn mem_heat_class(mem_pct: f32) -> &'static str {
    if mem_pct >= 15.0 {
        "val-hot"
    } else if mem_pct >= 5.0 {
        "val-warm"
    } else {
        ""
    }
}

pub fn proc_monogram(name: &str) -> String {
    let mut uppers = name.chars().filter(|c| c.is_ascii_uppercase());
    if let (Some(a), Some(b)) = (uppers.next(), uppers.next()) {
        return format!("{a}{b}");
    }
    let clean: String = name.chars().filter(|c| c.is_alphanumeric()).take(2).collect();
    if clean.is_empty() {
        "?".to_string()
    } else {
        clean.to_uppercase()
    }
}

pub fn proc_monogram_style(name: &str) -> String {
    let mut hash: u32 = 5381;
    for byte in name.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    let hue = hash % 360;
    format!(
        "background: linear-gradient(135deg, hsla({hue}, 70%, 52%, 0.34), hsla({hue}, 70%, 36%, 0.18)); border-color: hsla({hue}, 75%, 62%, 0.42); color: hsl({hue}, 90%, 84%);"
    )
}

#[component]
pub fn GroupAvatar(name: String, icon: Option<String>) -> Element {
    if let Some(src) = icon {
        return rsx! {
            img { class: "proc-app-icon", src: "{src}", alt: "" }
        };
    }
    let initials = proc_monogram(&name);
    let style = proc_monogram_style(&name);
    rsx! {
        span { class: "proc-monogram", style: "{style}", "{initials}" }
    }
}

#[component]
pub fn MonitorHealthBadge(score: u8) -> Element {
    let label = monitor_health_label(score);
    let class = if score >= 85 {
        "great"
    } else if score >= 65 {
        "ok"
    } else if score >= 40 {
        "warn"
    } else {
        "bad"
    };
    rsx! {
        div { class: "monitor-health-badge {class}",
            span { class: "monitor-health-score", "{score}" }
            div { class: "monitor-health-copy",
                span { class: "monitor-health-title", "System health" }
                span { class: "monitor-health-label", "{label}" }
            }
        }
    }
}

pub fn format_core_topology(sample: &MachineSample) -> String {
    if sample.perf_cores > 0 && sample.eff_cores > 0 {
        format!(
            "{}c ({}P + {}E)",
            sample.logical_cores, sample.perf_cores, sample.eff_cores
        )
    } else if sample.physical_cores > 0 {
        format!("{}c / {}t", sample.physical_cores, sample.logical_cores)
    } else {
        format!("{} cores", sample.logical_cores)
    }
}

#[component]
pub fn MonitorOverviewHero(
    sample: MachineSample,
    cpu_spark: Vec<f32>,
    mem_spark: Vec<f32>,
    net_rx: Vec<f32>,
    net_tx: Vec<f32>,
    disk_r: Vec<f32>,
    disk_w: Vec<f32>,
) -> Element {
    let total_mem = sample.total_memory.max(1) as f64;
    let mem_frac = (sample.used_memory as f64 / total_mem).clamp(0.0, 1.0) as f32;
    let swap_frac = if sample.swap_total > 0 {
        (sample.swap_used as f32 / sample.swap_total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let health = monitor_health_score((sample.cpu_percent / 100.0).clamp(0.0, 1.0), mem_frac, swap_frac);
    let core_topology = format_core_topology(&sample);

    let m = &sample.memory;
    let app_pct = (m.app_bytes as f64 / total_mem * 100.0).clamp(0.0, 100.0);
    let wired_pct = (m.wired_bytes as f64 / total_mem * 100.0).clamp(0.0, 100.0);
    let comp_pct = (m.compressed_bytes as f64 / total_mem * 100.0).clamp(0.0, 100.0);
    let cache_pct = (m.cached_bytes as f64 / total_mem * 100.0).clamp(0.0, 100.0);
    let used_pct = mem_frac * 100.0;

    let (pressure_cls, pressure_label) = match m.pressure_level {
        1 => ("normal", "Pressure: Normal"),
        2 => ("warn", "Pressure: Warn"),
        _ if m.pressure_level >= 4 => ("critical", "Pressure: Critical"),
        _ => ("normal", "Pressure: Nominal"),
    };

    let cpu_pts = monitor_sparkline_points(&cpu_spark, 320.0, 46.0);
    let mem_pts = monitor_sparkline_points(&mem_spark, 320.0, 40.0);
    let net_ceil = net_rx.iter().chain(net_tx.iter()).cloned().fold(1024.0f32, f32::max);
    let rx_pts = monitor_dynamic_sparkline_points(&net_rx, 320.0, 32.0, net_ceil);
    let tx_pts = monitor_dynamic_sparkline_points(&net_tx, 320.0, 32.0, net_ceil);
    let disk_ceil = disk_r.iter().chain(disk_w.iter()).cloned().fold(4096.0f32, f32::max);
    let dr_pts = monitor_dynamic_sparkline_points(&disk_r, 320.0, 32.0, disk_ceil);
    let dw_pts = monitor_dynamic_sparkline_points(&disk_w, 320.0, 32.0, disk_ceil);

    let user_w = sample.cpu_user_pct.clamp(0.0, 100.0);
    let sys_w = sample.cpu_sys_pct.clamp(0.0, 100.0);
    let nice_w = sample.cpu_nice_pct.clamp(0.0, 100.0);

    rsx! {
        div { class: "monitor-hero-shell",
            div { class: "monitor-host-strip",
                div { class: "monitor-host-block",
                    div { class: "monitor-cpu-brand", "{sample.cpu_brand}" }
                    div { class: "monitor-host-meta",
                        span { "{sample.host_name}" }
                        span { class: "monitor-dot-sep", "·" }
                        span { "{sample.os_version}" }
                        span { class: "monitor-dot-sep", "·" }
                        span { class: "monitor-topo-pill", "{core_topology}" }
                        span { class: "monitor-dot-sep", "·" }
                        span { "Up {format_uptime(sample.uptime_secs)}" }
                    }
                }
                div { class: "monitor-hero-badges",
                    if let Some(ref bat) = sample.battery {
                        {
                            let bat_cls = if bat.on_ac {
                                "monitor-battery-pill ac"
                            } else if bat.percent <= 20.0 {
                                "monitor-battery-pill low"
                            } else {
                                "monitor-battery-pill"
                            };
                            let suffix = if bat.time_remaining.is_empty() {
                                String::new()
                            } else {
                                format!(" · {}", bat.time_remaining)
                            };
                            rsx! {
                                div { class: "{bat_cls}",
                                    span { class: "bat-dot" }
                                    span { class: "bat-pct", "{bat.percent:.0}%" }
                                    span { class: "bat-meta", "{bat.status}{suffix}" }
                                }
                            }
                        }
                    }
                    MonitorHealthBadge { score: health }
                }
            }

            div { class: "monitor-hero-cards",
                div { class: "card monitor-hero-card cpu",
                    div { class: "monitor-card-head",
                        span { class: "monitor-chart-title", "CPU Utilization" }
                        span { class: "monitor-hero-chip",
                            "Usr {sample.cpu_user_pct:.0}% · Sys {sample.cpu_sys_pct:.0}%"
                        }
                    }
                    div { class: "hero-metric-row",
                        span { class: "hero-big-num", "{sample.cpu_percent:.1}%" }
                        span { class: "hero-big-sub",
                            "Load {sample.load_avg_1m:.2} · {sample.load_avg_5m:.2} · {sample.load_avg_15m:.2}"
                        }
                    }
                    div { class: "monitor-stacked-bar",
                        span { class: "seg-user", style: "width: {user_w:.2}%;" }
                        span { class: "seg-sys", style: "width: {sys_w:.2}%;" }
                        span { class: "seg-nice", style: "width: {nice_w:.2}%;" }
                    }
                    svg { class: "monitor-spark-svg hero-spark", view_box: "0 0 320 46", preserve_aspect_ratio: "none",
                        if !cpu_pts.is_empty() {
                            polyline {
                                points: "{cpu_pts}",
                                fill: "none",
                                stroke: "#1ed760",
                                stroke_width: "2.2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                            }
                        }
                    }
                }

                div { class: "card monitor-hero-card mem",
                    div { class: "monitor-card-head",
                        span { class: "monitor-chart-title", "Memory Architecture" }
                        span { class: "monitor-pressure-pill {pressure_cls}", "{pressure_label}" }
                    }
                    div { class: "hero-metric-row",
                        span { class: "hero-big-num", "{format_bytes(sample.used_memory)}" }
                        span { class: "hero-big-sub",
                            "of {format_bytes(sample.total_memory)} ({used_pct:.0}%) · Avail {format_bytes(sample.available_memory)}"
                        }
                    }
                    div { class: "monitor-stacked-bar mem",
                        span { class: "seg-app", style: "width: {app_pct:.2}%;" }
                        span { class: "seg-wired", style: "width: {wired_pct:.2}%;" }
                        span { class: "seg-comp", style: "width: {comp_pct:.2}%;" }
                        span { class: "seg-cache", style: "width: {cache_pct:.2}%;" }
                    }
                    div { class: "hero-mem-pills",
                        span { class: "hero-mem-tag", span { class: "legend-swatch seg-app" } "App {format_bytes(m.app_bytes)}" }
                        span { class: "hero-mem-tag", span { class: "legend-swatch seg-wired" } "Wired {format_bytes(m.wired_bytes)}" }
                        span { class: "hero-mem-tag", span { class: "legend-swatch seg-comp" } "Comp {format_bytes(m.compressed_bytes)}" }
                        span { class: "hero-mem-tag", span { class: "legend-swatch seg-io" } "Swap {format_bytes(sample.swap_used)}" }
                    }
                    svg { class: "monitor-spark-svg hero-spark-sm", view_box: "0 0 320 40", preserve_aspect_ratio: "none",
                        if !mem_pts.is_empty() {
                            polyline {
                                points: "{mem_pts}",
                                fill: "none",
                                stroke: "#5bb8ff",
                                stroke_width: "2.0",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                            }
                        }
                    }
                }

                div { class: "card monitor-hero-card io",
                    div { class: "monitor-card-head",
                        span { class: "monitor-chart-title", "Live I/O & Tasks" }
                        span { class: "monitor-hero-chip",
                            "{sample.groups.len()} groups · {format_count(sample.process_count as u64)} procs"
                        }
                    }
                    div { class: "hero-io-block",
                        div { class: "hero-io-row",
                            span { class: "hero-io-k", "Network" }
                            span { class: "hero-io-v",
                                "↓ {format_rate(sample.network.rx_bytes_per_sec)} · ↑ {format_rate(sample.network.tx_bytes_per_sec)}"
                            }
                        }
                        svg { class: "monitor-spark-svg hero-spark-xs", view_box: "0 0 320 32", preserve_aspect_ratio: "none",
                            if !rx_pts.is_empty() {
                                polyline { points: "{rx_pts}", fill: "none", stroke: "#1ed760", stroke_width: "1.9", stroke_linecap: "round", stroke_linejoin: "round" }
                            }
                            if !tx_pts.is_empty() {
                                polyline { points: "{tx_pts}", fill: "none", stroke: "#38bdf8", stroke_width: "1.9", stroke_linecap: "round", stroke_linejoin: "round" }
                            }
                        }
                    }
                    div { class: "hero-io-block",
                        div { class: "hero-io-row",
                            span { class: "hero-io-k", "Disk" }
                            span { class: "hero-io-v",
                                "R {format_rate(sample.disk_read_bytes_per_sec)} · W {format_rate(sample.disk_write_bytes_per_sec)}"
                            }
                        }
                        svg { class: "monitor-spark-svg hero-spark-xs", view_box: "0 0 320 32", preserve_aspect_ratio: "none",
                            if !dr_pts.is_empty() {
                                polyline { points: "{dr_pts}", fill: "none", stroke: "#c084fc", stroke_width: "1.9", stroke_linecap: "round", stroke_linejoin: "round" }
                            }
                            if !dw_pts.is_empty() {
                                polyline { points: "{dw_pts}", fill: "none", stroke: "#f5b942", stroke_width: "1.9", stroke_linecap: "round", stroke_linejoin: "round" }
                            }
                        }
                    }
                    div { class: "hero-io-foot",
                        "{sample.running_tasks} running · {format_count(sample.total_threads)} threads · Pg {format_count(m.pageouts)} out"
                    }
                }
            }
        }
    }
}

#[component]
pub fn MonitorSmartAlerts(
    sample: MachineSample,
    on_open_group: EventHandler<(String, u32)>,
) -> Element {
    let mut alerts: Vec<(&'static str, String, Option<(String, u32)>)> = Vec::new();

    if let Some(heavy_mem) = sample
        .groups
        .iter()
        .max_by(|a, b| a.mem_percent.total_cmp(&b.mem_percent))
        .filter(|g| g.mem_percent >= 20.0)
    {
        alerts.push((
            "warn",
            format!(
                "{} is using {} ({:.0}% RAM) across {} proc{}",
                heavy_mem.name,
                format_bytes(heavy_mem.memory_bytes),
                heavy_mem.mem_percent,
                heavy_mem.members.len(),
                if heavy_mem.members.len() == 1 { "" } else { "s" }
            ),
            Some((heavy_mem.key.clone(), heavy_mem.primary_pid)),
        ));
    }

    if let Some(hot_cpu) = sample
        .groups
        .iter()
        .max_by(|a, b| a.cpu_percent.total_cmp(&b.cpu_percent))
        .filter(|g| g.cpu_percent >= 120.0)
    {
        let already = alerts
            .first()
            .and_then(|(_, _, t)| t.as_ref().map(|(k, _)| k == &hot_cpu.key))
            .unwrap_or(false);
        if !already {
            alerts.push((
                "hot",
                format!(
                    "{} is spiking at {:.0}% CPU across {} proc{}",
                    hot_cpu.name,
                    hot_cpu.cpu_percent,
                    hot_cpu.members.len(),
                    if hot_cpu.members.len() == 1 { "" } else { "s" }
                ),
                Some((hot_cpu.key.clone(), hot_cpu.primary_pid)),
            ));
        }
    }

    if alerts.len() < 2
        && (sample.memory.pressure_level >= 2 || sample.swap_used >= 2 * 1024 * 1024 * 1024)
    {
        let target = sample
            .groups
            .iter()
            .max_by_key(|g| g.memory_bytes)
            .map(|g| (g.key.clone(), g.primary_pid));
        alerts.push((
            if sample.memory.pressure_level >= 4 { "hot" } else { "warn" },
            format!(
                "Elevated memory pressure · Swap used {}",
                format_bytes(sample.swap_used)
            ),
            target,
        ));
    }

    if alerts.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "monitor-alert-strip",
            for (kind, msg, target) in alerts.into_iter().take(2) {
                div { class: "monitor-alert-pill {kind}",
                    span { class: "alert-dot" }
                    span { class: "alert-msg", "{msg}" }
                    if let Some((gkey, pid)) = target {
                        button {
                            class: "alert-action",
                            onclick: move |_| on_open_group.call((gkey.clone(), pid)),
                            "Inspect →"
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn MonitorCoreMatrix(
    sample: MachineSample,
    expanded: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let user_w = sample.cpu_user_pct.clamp(0.0, 100.0);
    let sys_w = sample.cpu_sys_pct.clamp(0.0, 100.0);
    let nice_w = sample.cpu_nice_pct.clamp(0.0, 100.0);
    let topo = format_core_topology(&sample);

    rsx! {
        div { class: "card monitor-core-card",
            div { class: "monitor-card-head",
                div { class: "monitor-core-head-left",
                    span { class: "monitor-chart-title", "Per-Core CPU Load" }
                    span { class: "monitor-topo-pill", "{topo}" }
                }
                div { class: "monitor-core-head-right",
                    span { class: "monitor-card-strong", "{sample.cpu_percent:.1}%" }
                    button {
                        class: "chip",
                        onclick: move |_| on_toggle.call(()),
                        if expanded {
                            "Hide cores"
                        } else {
                            "Show all {sample.logical_cores} cores"
                        }
                    }
                }
            }
            div { class: "monitor-stacked-bar",
                span { class: "seg-user", style: "width: {user_w:.2}%;" }
                span { class: "seg-sys", style: "width: {sys_w:.2}%;" }
                span { class: "seg-nice", style: "width: {nice_w:.2}%;" }
            }
            div { class: "monitor-legend-row",
                span { class: "monitor-legend-item",
                    span { class: "legend-swatch seg-user" }
                    "User {sample.cpu_user_pct:.1}%"
                }
                span { class: "monitor-legend-item",
                    span { class: "legend-swatch seg-sys" }
                    "Sys {sample.cpu_sys_pct:.1}%"
                }
                span { class: "monitor-legend-item",
                    span { class: "legend-swatch seg-nice" }
                    "Nice {sample.cpu_nice_pct:.1}%"
                }
                span { class: "monitor-legend-item",
                    span { class: "legend-swatch seg-idle" }
                    "Idle {sample.cpu_idle_pct:.1}%"
                }
            }
            if expanded {
                div { class: "monitor-core-grid",
                    for core in sample.cores.iter() {
                        {
                            let cu = core.user_pct.clamp(0.0, 100.0);
                            let cs = core.sys_pct.clamp(0.0, 100.0);
                            let cn = core.nice_pct.clamp(0.0, 100.0);
                            let hot = gauge_class(core.total_pct / 100.0);
                            rsx! {
                                div { class: "monitor-core-cell {hot}", key: "{core.id}",
                                    div { class: "monitor-core-top",
                                        span { class: "monitor-core-id", "C{core.id}" }
                                        span { class: "monitor-core-pct", "{core.total_pct:.0}%" }
                                    }
                                    div { class: "monitor-core-bar",
                                        span { class: "seg-user", style: "width: {cu:.1}%;" }
                                        span { class: "seg-sys", style: "width: {cs:.1}%;" }
                                        span { class: "seg-nice", style: "width: {cn:.1}%;" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn MonitorStorageThermalRow(sample: MachineSample) -> Element {
    if sample.volumes.is_empty() && sample.thermals.is_empty() && sample.network.interfaces.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "monitor-env-row",
            if !sample.volumes.is_empty() {
                div { class: "card monitor-sub-card",
                    div { class: "monitor-chart-title", "Mounted Volumes" }
                    div { class: "monitor-vol-list",
                        for vol in sample.volumes.iter() {
                            {
                                let used = vol.total_bytes.saturating_sub(vol.available_bytes);
                                let frac = if vol.total_bytes > 0 {
                                    used as f32 / vol.total_bytes as f32
                                } else {
                                    0.0
                                };
                                let cls = gauge_class(frac);
                                let pct = (frac * 100.0).clamp(0.0, 100.0);
                                rsx! {
                                    div { class: "monitor-vol-item", key: "{vol.mount_point}",
                                        div { class: "monitor-vol-head",
                                            span { class: "monitor-vol-name", "{vol.name} ({vol.mount_point})" }
                                            span { class: "monitor-vol-meta",
                                                "{format_bytes(vol.available_bytes)} free / {format_bytes(vol.total_bytes)} · {vol.fs}"
                                            }
                                        }
                                        div { class: "meter",
                                            span { class: "vol-fill {cls}", style: "width: {pct:.1}%;" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !sample.network.interfaces.is_empty() || !sample.thermals.is_empty() {
                div { class: "card monitor-sub-card",
                    if !sample.network.interfaces.is_empty() {
                        div { class: "monitor-chart-title", "Interfaces" }
                        div { class: "monitor-iface-list",
                            for iface in sample.network.interfaces.iter().take(4) {
                                div { class: "monitor-iface-row", key: "{iface.name}",
                                    span { class: "monitor-iface-name", "{iface.name}" }
                                    span { class: "monitor-iface-ip",
                                        if iface.ip.is_empty() { "—" } else { "{iface.ip}" }
                                    }
                                    span { class: "monitor-iface-rates",
                                        "↓ {format_rate(iface.rx_bytes_per_sec)} · ↑ {format_rate(iface.tx_bytes_per_sec)}"
                                    }
                                }
                            }
                        }
                    }
                    if !sample.thermals.is_empty() {
                        div { class: "monitor-chart-title", style: "margin-top: 12px;", "Sensors" }
                        div { class: "monitor-thermal-chips",
                            for th in sample.thermals.iter() {
                                {
                                    let hot_frac = (th.temp_c / 100.0).clamp(0.0, 1.0);
                                    let cls = gauge_class(hot_frac);
                                    rsx! {
                                        span { class: "monitor-thermal-chip {cls}", key: "{th.label}",
                                            "{th.label}: {th.temp_c:.0}°C"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn MonitorSparkStrip(
    title: &'static str,
    values: Vec<f32>,
    stroke: &'static str,
    #[props(default)] badge: String,
) -> Element {
    let points = monitor_sparkline_points(&values, 320.0, 56.0);
    let has_data = !points.is_empty();
    let live_label = if !badge.is_empty() {
        badge
    } else if has_data {
        "live".to_string()
    } else {
        "—".to_string()
    };
    rsx! {
        div { class: "monitor-spark-card",
            div { class: "monitor-spark-head",
                span { class: "monitor-spark-title", "{title}" }
                span { class: "monitor-spark-live", "{live_label}" }
            }
            svg {
                class: "monitor-spark-svg",
                view_box: "0 0 320 56",
                preserve_aspect_ratio: "none",
                if has_data {
                    polyline {
                        points: "{points}",
                        fill: "none",
                        stroke: "{stroke}",
                        stroke_width: "2.2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                    }
                }
            }
        }
    }
}

#[component]
fn MonitorAreaChart(
    title: &'static str,
    subtitle: String,
    values: Vec<f32>,
    stroke: &'static str,
    fill: &'static str,
    unit: &'static str,
) -> Element {
    let floor = if unit == "%" { 15.0 } else { 0.5 };
    let max = monitor_chart_max(&values, floor);
    let norm: Vec<f32> = values.iter().map(|v| v / max).collect();
    let line = monitor_sparkline_points(&norm, 640.0, 120.0);
    let area = monitor_area_path(&norm, 640.0, 120.0, 1.0);
    let peak = values.iter().cloned().fold(0.0f32, f32::max);
    let avg = if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f32>() / values.len() as f32
    };
    let peak_label = if unit == "%" {
        format!("{peak:.0}%")
    } else {
        format!("{peak:.1}")
    };
    let avg_label = if unit == "%" {
        format!("{avg:.1}%")
    } else {
        format!("{avg:.2}")
    };
    rsx! {
        div { class: "monitor-chart-card",
            div { class: "monitor-chart-head",
                div {
                    span { class: "monitor-chart-title", "{title}" }
                    span { class: "monitor-chart-sub", "{subtitle}" }
                }
                div { class: "monitor-chart-legend",
                    span { "avg {avg_label}" }
                    span { "peak {peak_label}" }
                }
            }
            svg {
                class: "monitor-area-svg",
                view_box: "0 0 640 120",
                preserve_aspect_ratio: "none",
                if !area.is_empty() {
                    path { d: "{area}", fill: "{fill}", opacity: "0.35" }
                    polyline {
                        points: "{line}",
                        fill: "none",
                        stroke: "{stroke}",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                    }
                }
            }
        }
    }
}

#[component]
fn MonitorBucketChart(buckets: Vec<MonitorBucket>, metric: &'static str) -> Element {
    let vals: Vec<f32> = buckets
        .iter()
        .map(|b| {
            if metric == "cpu" {
                b.avg_cpu
            } else {
                b.avg_mem_pct
            }
        })
        .collect();
    let max = monitor_chart_max(&vals, 1.0);
    let n = buckets.len().max(1);
    let bar_w = 640.0 / n as f32 - 2.0;
    rsx! {
        div { class: "monitor-chart-card",
            div { class: "monitor-chart-head",
                div {
                    span { class: "monitor-chart-title",
                        if metric == "cpu" { "CPU by time bucket" } else { "Memory by time bucket" }
                    }
                    span { class: "monitor-chart-sub", "{n} buckets · avg per interval" }
                }
            }
            svg {
                class: "monitor-bar-svg",
                view_box: "0 0 640 120",
                preserve_aspect_ratio: "none",
                for (i, v) in vals.iter().enumerate() {
                    {
                        let h = (v / max * 100.0).clamp(2.0, 100.0);
                        let x = i as f32 * (640.0 / n as f32);
                        let y = 120.0 - h;
                        let fill = if metric == "cpu" { "#1ed760" } else { "#5bb8ff" };
                        rsx! {
                            rect {
                                x: "{x:.1}",
                                y: "{y:.1}",
                                width: "{bar_w:.1}",
                                height: "{h:.1}",
                                rx: "2",
                                fill: "{fill}",
                                opacity: "0.85",
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MonitorHeatmapStrip(history: Vec<HostPoint>) -> Element {
    let cells: Vec<f32> = history
        .iter()
        .map(|p| (p.cpu_percent / 100.0).clamp(0.0, 1.0))
        .collect();
    let step = (cells.len() / 72).max(1);
    let sampled: Vec<f32> = cells.iter().step_by(step).cloned().collect();
    rsx! {
        div { class: "monitor-heatmap-card",
            div { class: "monitor-chart-head",
                span { class: "monitor-chart-title", "CPU intensity map" }
                span { class: "monitor-chart-sub", "darker = busier · left = older" }
            }
            div { class: "monitor-heatmap",
                for (i, v) in sampled.iter().enumerate() {
                    {
                        let level = (v * 10.0).round() as u8;
                        rsx! {
                            span {
                                key: "{i}",
                                class: "monitor-heatmap-cell l{level}",
                                title: "{v * 100.0:.0}% CPU",
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn LeaderGroupRow(
    rank: usize,
    group: ProcessGroup,
    mem_mode: bool,
    max_val: f32,
    on_open_group: EventHandler<(String, u32)>,
) -> Element {
    let key = group.key.clone();
    let pid = group.primary_pid;
    let pct_width = if mem_mode {
        (group.memory_bytes as f32 / max_val.max(1.0) * 100.0).clamp(4.0, 100.0)
    } else {
        (group.cpu_percent / max_val.max(1.0) * 100.0).clamp(4.0, 100.0)
    };
    let bar = format!("width: {pct_width:.1}%;");
    let fill_cls = if mem_mode { "monitor-proc-fill mem" } else { "monitor-proc-fill cpu" };
    let val_str = if mem_mode {
        format_bytes(group.memory_bytes)
    } else {
        format!("{:.1}%", group.cpu_percent)
    };
    let sub_str = if mem_mode {
        format!(
            "PID {} · {:.1}% RAM · {} thr",
            group.primary_pid, group.mem_percent, group.total_threads
        )
    } else {
        format!(
            "PID {} · {} thr · {}",
            group.primary_pid,
            group.total_threads,
            format_proc_time(group.max_run_time_secs)
        )
    };
    let count = group.members.len();

    rsx! {
        button {
            class: "monitor-proc-row clickable",
            onclick: move |_| on_open_group.call((key.clone(), pid)),
            span { class: "monitor-proc-rank", "#{rank + 1}" }
            GroupAvatar { name: group.name.clone(), icon: group.icon_data_url.clone() }
            div { class: "monitor-proc-main",
                div { class: "monitor-proc-head-line",
                    span { class: "monitor-proc-name", "{group.name}" }
                    if count > 1 {
                        span { class: "monitor-group-count-pill", "{count} procs" }
                    }
                }
                div { class: "monitor-proc-sub", "{sub_str}" }
                div { class: "monitor-proc-track",
                    span { class: "{fill_cls}", style: "{bar}" }
                }
            }
            span { class: "monitor-proc-val", "{val_str}" }
        }
    }
}

#[component]
pub fn MonitorGroupedLeaders(
    sample: MachineSample,
    on_open_group: EventHandler<(String, u32)>,
) -> Element {
    let mut cpu_groups = sample.groups.clone();
    cpu_groups.sort_by(|a, b| {
        b.cpu_percent
            .total_cmp(&a.cpu_percent)
            .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
    });
    cpu_groups.truncate(6);
    let max_cpu = cpu_groups.first().map(|g| g.cpu_percent).unwrap_or(1.0).max(1.0);

    let mut mem_groups = sample.groups;
    mem_groups.sort_by(|a, b| {
        b.memory_bytes
            .cmp(&a.memory_bytes)
            .then_with(|| b.cpu_percent.total_cmp(&a.cpu_percent))
    });
    mem_groups.truncate(6);
    let max_mem = mem_groups.first().map(|g| g.memory_bytes as f32).unwrap_or(1.0).max(1.0);

    rsx! {
        div { class: "monitor-process-deck",
            div { class: "monitor-deck-col monitor-deck-glow-cpu",
                div { class: "monitor-deck-head",
                    span { "Top Apps & Groups · CPU" }
                    span { class: "monitor-deck-badge", "TOP {cpu_groups.len()}" }
                }
                for (rank, group) in cpu_groups.into_iter().enumerate() {
                    LeaderGroupRow {
                        rank,
                        group,
                        mem_mode: false,
                        max_val: max_cpu,
                        on_open_group,
                    }
                }
            }
            div { class: "monitor-deck-col monitor-deck-glow-mem",
                div { class: "monitor-deck-head",
                    span { "Top Apps & Groups · Memory" }
                    span { class: "monitor-deck-badge", "TOP {mem_groups.len()}" }
                }
                for (rank, group) in mem_groups.into_iter().enumerate() {
                    LeaderGroupRow {
                        rank,
                        group,
                        mem_mode: true,
                        max_val: max_mem,
                        on_open_group,
                    }
                }
            }
        }
    }
}

#[component]
pub fn MonitorHistoryRow(
    time_label: String,
    cpu: f32,
    mem_pct: f32,
    selected: bool,
    on_select: EventHandler<()>,
) -> Element {
    let mem_width = format!("width: {:.0}%;", mem_pct * 100.0);
    let cpu_width = format!("width: {cpu:.0}%;");
    rsx! {
        button {
            class: if selected { "monitor-hist-row selected" } else { "monitor-hist-row" },
            onclick: move |_| on_select.call(()),
            span { class: "monitor-hist-time", "{time_label}" }
            div { class: "monitor-hist-bars",
                span { class: "monitor-hist-track cpu",
                    span { style: "{cpu_width}" }
                }
                span { class: "monitor-hist-track mem",
                    span { style: "{mem_width}" }
                }
            }
            span { class: "monitor-hist-pct", "{cpu:.0}% CPU" }
        }
    }
}

#[component]
fn AggregateLeaderRow(proc: ProcessAggregate, mem_mode: bool, rank: usize, max_val: f32) -> Element {
    let val = if mem_mode {
        proc.peak_mem as f32
    } else {
        proc.peak_cpu
    };
    let width = (val / max_val.max(0.01) * 100.0).clamp(4.0, 100.0);
    let bar = format!("width: {width:.1}%;");
    rsx! {
        div { class: "monitor-proc-row",
            span { class: "monitor-proc-rank", "#{rank}" }
            div { class: "monitor-proc-main",
                div { class: "monitor-proc-name", "{proc.name}" }
                div { class: "monitor-proc-sub",
                    "{format_count(proc.samples)} samples · avg CPU {proc.avg_cpu:.1}%"
                }
                div { class: "monitor-proc-track",
                    span {
                        class: if mem_mode { "monitor-proc-fill mem" } else { "monitor-proc-fill cpu" },
                        style: "{bar}",
                    }
                }
            }
            span { class: "monitor-proc-val",
                if mem_mode {
                    "peak {format_bytes(proc.peak_mem)}"
                } else {
                    "peak {proc.peak_cpu:.0}%"
                }
            }
        }
    }
}

#[component]
pub fn MonitorAnalyticsBoard(
    analytics: MonitorAnalytics,
    history: Vec<HostPoint>,
    range_label: String,
) -> Element {
    let cpu_series: Vec<f32> = history.iter().map(|p| p.cpu_percent).collect();
    let mem_series: Vec<f32> = history
        .iter()
        .map(|p| {
            if p.mem_total > 0 {
                p.mem_used as f32 / p.mem_total as f32 * 100.0
            } else {
                0.0
            }
        })
        .collect();
    let load_series: Vec<f32> = history.iter().map(|p| p.load_avg_1m).collect();
    let swap_series: Vec<f32> = history
        .iter()
        .map(|p| {
            if p.swap_total > 0 {
                p.swap_used as f32 / p.swap_total as f32 * 100.0
            } else {
                0.0
            }
        })
        .collect();
    let span_label = match (analytics.first_at, analytics.last_at) {
        (Some(a), Some(b)) => format!(
            "{} → {}",
            format_unix_time(a),
            format_unix_time(b)
        ),
        _ => "—".into(),
    };
    let max_cpu_leader = analytics
        .top_cpu
        .iter()
        .map(|p| p.peak_cpu)
        .fold(1.0f32, f32::max);
    let max_mem_leader = analytics
        .top_mem
        .iter()
        .map(|p| p.peak_mem as f32)
        .fold(1.0, f32::max);

    rsx! {
        div { class: "monitor-dash-header",
            div {
                h3 { class: "monitor-dash-title", "Performance overview" }
                p { class: "monitor-dash-sub", "{range_label} · {span_label}" }
            }
            div { class: "monitor-dash-pill",
                "{format_count(analytics.samples)} samples"
            }
        }
        div { class: "monitor-analytics-grid",
            div { class: "monitor-metric-card",
                div { class: "monitor-metric-label", "CPU avg / p95" }
                div { class: "monitor-metric-value", "{analytics.avg_cpu:.1}% · {analytics.p95_cpu:.0}%" }
                div { class: "monitor-metric-sub", "peak {analytics.peak_cpu:.0}%" }
            }
            div { class: "monitor-metric-card",
                div { class: "monitor-metric-label", "Memory avg / p95" }
                div { class: "monitor-metric-value", "{analytics.avg_mem_pct:.0}% · {analytics.p95_mem_pct:.0}%" }
                div { class: "monitor-metric-sub", "peak {analytics.peak_mem_pct:.0}%" }
            }
            div { class: "monitor-metric-card",
                div { class: "monitor-metric-label", "Load (1m)" }
                div { class: "monitor-metric-value", "{analytics.avg_load:.2}" }
                div { class: "monitor-metric-sub", "peak {analytics.peak_load:.2}" }
            }
            div { class: "monitor-metric-card",
                div { class: "monitor-metric-label", "Swap" }
                div { class: "monitor-metric-value", "{analytics.avg_swap_pct:.0}%" }
                div { class: "monitor-metric-sub", "peak {analytics.peak_swap_pct:.0}%" }
            }
            div { class: "monitor-metric-card wide",
                div { class: "monitor-metric-label", "Pressure events" }
                div { class: "monitor-metric-value",
                    "{analytics.hot_cpu_samples} hot CPU · {analytics.hot_mem_samples} hot RAM"
                }
                div { class: "monitor-metric-sub",
                    "~{analytics.avg_process_count:.0} processes running (avg)"
                }
            }
        }
        if !analytics.insights.is_empty() {
            div { class: "monitor-insights-card",
                div { class: "monitor-chart-title", "Insights" }
                ul { class: "monitor-insights",
                    for line in analytics.insights.clone() {
                        li { "{line}" }
                    }
                }
            }
        }
        div { class: "monitor-dash-charts",
            MonitorAreaChart {
                title: "CPU",
                subtitle: String::from("Host processor %"),
                values: cpu_series,
                stroke: "#1ed760",
                fill: "#1ed760",
                unit: "%",
            }
            MonitorAreaChart {
                title: "Memory",
                subtitle: String::from("Used RAM %"),
                values: mem_series,
                stroke: "#5bb8ff",
                fill: "#5bb8ff",
                unit: "%",
            }
            MonitorAreaChart {
                title: "Load average",
                subtitle: String::from("1 minute"),
                values: load_series,
                stroke: "#f5b942",
                fill: "#f5b942",
                unit: "load",
            }
            MonitorAreaChart {
                title: "Swap",
                subtitle: String::from("Swap used %"),
                values: swap_series,
                stroke: "#c084fc",
                fill: "#c084fc",
                unit: "%",
            }
        }
        if !analytics.buckets.is_empty() {
            div { class: "monitor-dash-charts two-col",
                MonitorBucketChart { buckets: analytics.buckets.clone(), metric: "cpu" }
                MonitorBucketChart { buckets: analytics.buckets.clone(), metric: "mem" }
            }
        }
        MonitorHeatmapStrip { history: history.clone() }
        div { class: "monitor-process-deck",
            div { class: "monitor-deck-col",
                div { class: "monitor-deck-head",
                    span { "Top CPU (logged)" }
                }
                if analytics.top_cpu.is_empty() {
                    p { class: "row-sub", "Process CPU appears after history logging runs." }
                } else {
                    for (i, proc) in analytics.top_cpu.iter().enumerate() {
                        AggregateLeaderRow {
                            proc: proc.clone(),
                            mem_mode: false,
                            rank: i + 1,
                            max_val: max_cpu_leader,
                        }
                    }
                }
            }
            div { class: "monitor-deck-col",
                div { class: "monitor-deck-head",
                    span { "Top memory (logged)" }
                }
                if analytics.top_mem.is_empty() {
                    p { class: "row-sub", "Process memory appears after history logging runs." }
                } else {
                    for (i, proc) in analytics.top_mem.iter().enumerate() {
                        AggregateLeaderRow {
                            proc: proc.clone(),
                            mem_mode: true,
                            rank: i + 1,
                            max_val: max_mem_leader,
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn MonitorProcessHistoryRows(processes: Vec<ProcessPoint>) -> Element {
    rsx! {
        div { class: "monitor-snapshot-detail",
            for proc in processes {
                div { class: "monitor-proc-row compact",
                    div { class: "monitor-proc-main",
                        div { class: "monitor-proc-name", "{proc.name}" }
                        div { class: "monitor-proc-sub", "PID {proc.pid}" }
                    }
                    span { class: "monitor-proc-val", "{proc.cpu_percent:.1}% · {format_bytes(proc.memory_bytes)}" }
                }
            }
        }
    }
}
