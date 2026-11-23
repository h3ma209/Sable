use dioxus::prelude::*;
use engine::{MachineSample, ProcessGroup};

use crate::actions::{inspect_process, run_checked_care, sample_machine};
use crate::components::{
    cpu_heat_class, format_core_topology, mem_heat_class, GroupAvatar, MonitorHealthBadge,
    NoticeBanner,
};
use crate::state::{AppState, ProcessViewMode, Screen};
use crate::util::{
    format_bytes, format_count, format_rate, format_uptime, monitor_health_score,
    monitor_sparkline_points, scan_age,
};

#[component]
pub fn CareScreen(mut state: Signal<AppState>) -> Element {
    let (reclaim, threats, agents, apps, leftovers, downloads, picks, busy, age, sample, spark_cpu, spark_mem, care_loaded) = {
        let s = state.read();
        let reclaim = if s.junk.is_empty() {
            s.care.as_ref().map(|r| r.cleanup_bytes)
        } else {
            Some(s.junk.iter().map(|r| r.bytes).sum())
        };
        let threats = s.threat.as_ref().map(|r| r.hits.len());
        let agents = s
            .care
            .as_ref()
            .map(|r| r.agent_count)
            .unwrap_or(s.agents.len());
        let apps = s
            .care
            .as_ref()
            .map(|r| r.app_count)
            .unwrap_or(s.apps.len());
        let leftovers = if s.leftovers_done {
            Some(s.leftovers.len())
        } else {
            None
        };
        let downloads = if s.loaded.contains(&Screen::Clutter) {
            Some(s.clutter.len())
        } else {
            None
        };
        let sample = s
            .sample
            .clone()
            .or_else(|| s.care.as_ref().map(|c| c.sample.clone()));
        (
            reclaim,
            threats,
            agents,
            apps,
            leftovers,
            downloads,
            s.care_picks.clone(),
            s.busy,
            scan_age(s.last_refresh),
            sample,
            s.monitor_spark_cpu.clone(),
            s.monitor_spark_mem.clone(),
            s.care.is_some(),
        )
    };

    let threats_clear = matches!(threats, Some(0));
    let selected = [
        Screen::Cleanup,
        Screen::Performance,
        Screen::Clutter,
        Screen::Malware,
    ]
    .iter()
    .filter(|s| picks.contains(s) && !(**s == Screen::Malware && threats_clear))
    .count();

    let auto_poll = state.read().auto_poll_monitor;
    let loading_initial = !care_loaded && sample.is_none() && busy;

    rsx! {
        NoticeBanner { state }
        if loading_initial {
            div { class: "card loading",
                div { class: "spinner" }
                "Scanning your Mac…"
            }
        } else {
            {
                let mut snap = sample.unwrap_or_default();
                let groups = std::mem::take(&mut snap.groups);
                let group_count = groups.len();
                snap.processes.clear();
                rsx! {
                    div { class: "home-command-center",
                        HomeHeader {
                            state,
                            sample: snap.clone(),
                            age: age.clone(),
                            busy,
                            selected,
                            auto_poll,
                        }
                        div { class: "home-bento",
                            HomeStorageCard {
                                state,
                                sample: snap.clone(),
                                reclaim,
                                downloads,
                                apps,
                                leftovers,
                            }
                            HomePulseCard {
                                state,
                                sample: snap,
                                group_count,
                                spark_cpu,
                                spark_mem,
                                auto_poll,
                            }
                        }
                        div { class: "home-lower-grid",
                            HomeActiveAppsCard {
                                state,
                                groups,
                                auto_poll,
                            }
                            HomeAuditQueueCard {
                                state,
                                reclaim,
                                threats,
                                threats_clear,
                                agents,
                                downloads,
                                selected,
                                busy,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HomeHeader(
    mut state: Signal<AppState>,
    sample: MachineSample,
    age: String,
    busy: bool,
    selected: usize,
    auto_poll: bool,
) -> Element {
    let total_mem = sample.total_memory.max(1) as f64;
    let mem_frac = (sample.used_memory as f64 / total_mem).clamp(0.0, 1.0) as f32;
    let swap_frac = if sample.swap_total > 0 {
        (sample.swap_used as f32 / sample.swap_total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let health = monitor_health_score((sample.cpu_percent / 100.0).clamp(0.0, 1.0), mem_frac, swap_frac);
    let has_sample = sample.total_memory > 0;
    let host = if sample.host_name.is_empty() {
        "Your Mac".to_string()
    } else {
        sample.host_name.clone()
    };
    let cpu_label = if sample.cpu_brand.is_empty() {
        "Command Center".to_string()
    } else {
        sample.cpu_brand.clone()
    };
    let os_label = if sample.os_version.is_empty() {
        "macOS".to_string()
    } else {
        sample.os_version.clone()
    };
    let topo = if sample.logical_cores > 0 {
        format_core_topology(&sample)
    } else {
        String::new()
    };

    rsx! {
        header { class: "home-header",
            div { class: "home-identity",
                div { class: "home-eyebrow",
                    span { class: if auto_poll { "home-live-dot" } else { "home-live-dot off" } }
                    button {
                        class: if auto_poll { "home-monitor-pill on" } else { "home-monitor-pill off" },
                        title: if auto_poll { "Pause live monitoring" } else { "Start live monitoring" },
                        onclick: move |_| {
                            let next = !state.read().auto_poll_monitor;
                            state.write().auto_poll_monitor = next;
                            if next {
                                sample_machine(state);
                            }
                        },
                        if auto_poll { "Monitor: Live" } else { "Monitor: Off · Start" }
                    }
                    span { class: "home-chip-brand", "{cpu_label}" }
                    if !topo.is_empty() {
                        span { class: "monitor-topo-pill", "{topo}" }
                    }
                    if sample.uptime_secs > 0 {
                        span { class: "home-meta-muted", "Up {format_uptime(sample.uptime_secs)}" }
                    }
                    if let Some(ref bat) = sample.battery {
                        {
                            let bat_cls = if bat.on_ac {
                                "monitor-battery-pill ac"
                            } else if bat.percent <= 20.0 {
                                "monitor-battery-pill low"
                            } else {
                                "monitor-battery-pill"
                            };
                            rsx! {
                                span { class: "{bat_cls}",
                                    span { class: "bat-dot" }
                                    span { class: "bat-pct", "{bat.percent:.0}%" }
                                    span { class: "bat-meta", "{bat.status}" }
                                }
                            }
                        }
                    }
                }
                h1 { class: "home-title", "{host}" }
                p { class: "home-subtitle", "{os_label} · Last audit {age}" }
            }

            div { class: "home-header-actions",
                if has_sample {
                    MonitorHealthBadge { score: health }
                }
                button {
                    class: "btn btn-primary btn-lg",
                    disabled: busy || selected == 0,
                    onclick: move |_| run_checked_care(state),
                    if busy { "Scanning…" } else { "Run Smart Audit" }
                }
            }
        }
    }
}

#[component]
fn HomeStorageCard(
    mut state: Signal<AppState>,
    sample: MachineSample,
    reclaim: Option<u64>,
    downloads: Option<usize>,
    apps: usize,
    leftovers: Option<usize>,
) -> Element {
    let total = sample.boot_volume.total;
    let free = sample.boot_volume.free;
    let used = total.saturating_sub(free);
    let used_pct = if total > 0 {
        (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let junk_bytes = reclaim.unwrap_or(0).min(used);
    let base_used = used.saturating_sub(junk_bytes);
    let base_w = if total > 0 {
        (base_used as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let junk_w = if total > 0 && junk_bytes > 0 {
        (junk_bytes as f64 / total as f64 * 100.0)
            .max(1.2)
            .min((100.0 - base_w).max(0.0))
    } else {
        0.0
    };

    let junk_val = reclaim.map(format_bytes).unwrap_or_else(|| "—".into());
    let clutter_val = match downloads {
        Some(n) => format!("{n} files"),
        None => "Inspect".into(),
    };
    let apps_val = match leftovers {
        Some(n) if n > 0 => format!("{n} leftovers"),
        _ => format!("{apps} apps"),
    };

    rsx! {
        section { class: "card home-storage-card",
            div { class: "home-card-head",
                div {
                    span { class: "home-card-title", "Macintosh HD" }
                    if total > 0 {
                        span { class: "home-card-sub",
                            "{format_bytes(free)} free of {format_bytes(total)}"
                        }
                    }
                }
                button {
                    class: "home-link-btn",
                    onclick: move |_| state.write().try_go(Screen::Space),
                    "Space Lens →"
                }
            }

            div { class: "home-storage-hero",
                span { class: "home-storage-used",
                    if total > 0 {
                        "{format_bytes(used)} used"
                    } else {
                        "Checking disk…"
                    }
                }
                if total > 0 {
                    span { class: "home-storage-pct", "{used_pct:.0}%" }
                }
            }

            div { class: "home-storage-bar",
                div { class: "home-storage-seg-used", style: "width: {base_w:.2}%;" }
                if junk_w > 0.0 {
                    div { class: "home-storage-seg-junk", style: "width: {junk_w:.2}%;" }
                }
            }

            div { class: "home-storage-legend",
                span { class: "home-legend-item",
                    span { class: "home-swatch used" }
                    "System & Data"
                }
                span { class: "home-legend-item",
                    span { class: "home-swatch junk" }
                    "Reclaimable ({junk_val})"
                }
            }

            div { class: "home-storage-stats",
                button {
                    class: "home-stat-chip",
                    onclick: move |_| state.write().try_go(Screen::Cleanup),
                    span { class: "home-stat-k", "Reclaimable Junk" }
                    span { class: "home-stat-v accent", "{junk_val}" }
                }
                button {
                    class: "home-stat-chip",
                    onclick: move |_| state.write().try_go(Screen::Clutter),
                    span { class: "home-stat-k", "Downloads Clutter" }
                    span { class: "home-stat-v", "{clutter_val}" }
                }
                button {
                    class: "home-stat-chip",
                    onclick: move |_| state.write().try_go(Screen::Apps),
                    span { class: "home-stat-k", "Applications" }
                    span { class: "home-stat-v", "{apps_val}" }
                }
            }
        }
    }
}

#[component]
fn HomePulseCard(
    mut state: Signal<AppState>,
    sample: MachineSample,
    group_count: usize,
    spark_cpu: Vec<f32>,
    spark_mem: Vec<f32>,
    auto_poll: bool,
) -> Element {
    let total_mem = sample.total_memory.max(1) as f64;
    let mem_pct = (sample.used_memory as f64 / total_mem * 100.0).clamp(0.0, 100.0) as f32;
    let pressure_lbl = match sample.memory.pressure_level {
        1 => "Normal",
        2 => "Warn",
        l if l >= 4 => "Critical",
        _ => "Nominal",
    };
    let cpu_pts = monitor_sparkline_points(&spark_cpu, 160.0, 28.0);
    let mem_pts = monitor_sparkline_points(&spark_mem, 160.0, 28.0);
    let cpu_cls = cpu_heat_class(sample.cpu_percent);
    let mem_cls = mem_heat_class(mem_pct);

    rsx! {
        section { class: "card home-pulse-card",
            div { class: "home-card-head",
                div {
                    span { class: "home-card-title", "Live Telemetry" }
                    span { class: "home-card-sub",
                        if auto_poll {
                            "Real-time host stats"
                        } else {
                            "Monitoring off · startup snapshot"
                        }
                    }
                }
                div { class: "home-head-controls",
                    button {
                        class: "home-link-btn",
                        onclick: move |_| {
                            let next = !state.read().auto_poll_monitor;
                            state.write().auto_poll_monitor = next;
                            if next {
                                sample_machine(state);
                            }
                        },
                        if auto_poll { "Pause" } else { "Start Live" }
                    }
                    button {
                        class: "home-link-btn",
                        onclick: move |_| {
                            let mut w = state.write();
                            w.monitor_tab = 0;
                            w.try_go(Screen::Assistant);
                        },
                        "System Monitor →"
                    }
                }
            }

            div { class: "home-pulse-grid",
                div { class: "home-pulse-tile",
                    div { class: "home-pulse-top",
                        span { class: "home-pulse-k", "CPU" }
                        span { class: "home-pulse-v {cpu_cls}", "{sample.cpu_percent:.1}%" }
                    }
                    div { class: "home-pulse-sub",
                        "Usr {sample.cpu_user_pct:.0}% · Sys {sample.cpu_sys_pct:.0}%"
                    }
                    svg { class: "home-spark-svg", view_box: "0 0 160 28", preserve_aspect_ratio: "none",
                        if !cpu_pts.is_empty() {
                            polyline {
                                points: "{cpu_pts}",
                                fill: "none",
                                stroke: "#1ed760",
                                stroke_width: "2.0",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                            }
                        }
                    }
                }

                div { class: "home-pulse-tile",
                    div { class: "home-pulse-top",
                        span { class: "home-pulse-k", "Memory" }
                        span { class: "home-pulse-v {mem_cls}", "{format_bytes(sample.used_memory)}" }
                    }
                    div { class: "home-pulse-sub",
                        "{mem_pct:.0}% · Pressure: {pressure_lbl}"
                    }
                    svg { class: "home-spark-svg", view_box: "0 0 160 28", preserve_aspect_ratio: "none",
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

                div { class: "home-pulse-tile",
                    div { class: "home-pulse-top",
                        span { class: "home-pulse-k", "Network I/O" }
                        span { class: "home-pulse-v", "↓ {format_rate(sample.network.rx_bytes_per_sec)}" }
                    }
                    div { class: "home-pulse-sub",
                        "↑ {format_rate(sample.network.tx_bytes_per_sec)} up"
                    }
                }

                div { class: "home-pulse-tile",
                    div { class: "home-pulse-top",
                        span { class: "home-pulse-k", "Activity" }
                        if group_count > 0 {
                            span { class: "home-pulse-v", "{format_count(group_count as u64)} groups" }
                        } else {
                            span { class: "home-pulse-v", "Load {sample.load_avg_1m:.2}" }
                        }
                    }
                    div { class: "home-pulse-sub",
                        if group_count > 0 {
                            "{format_count(sample.process_count as u64)} procs · Load {sample.load_avg_1m:.2}"
                        } else {
                            "5m {sample.load_avg_5m:.2} · {format_bytes(sample.available_memory)} avail"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HomeActiveAppsCard(
    mut state: Signal<AppState>,
    mut groups: Vec<ProcessGroup>,
    auto_poll: bool,
) -> Element {
    let mut sort_by_cpu = use_signal(|| false);
    let cpu_mode = *sort_by_cpu.read();

    if cpu_mode {
        groups.sort_by(|a, b| {
            b.cpu_percent
                .total_cmp(&a.cpu_percent)
                .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
        });
    } else {
        groups.sort_by(|a, b| {
            b.memory_bytes
                .cmp(&a.memory_bytes)
                .then_with(|| b.cpu_percent.total_cmp(&a.cpu_percent))
        });
    }
    groups.truncate(5);

    let max_val = if cpu_mode {
        groups.first().map(|g| g.cpu_percent).unwrap_or(1.0).max(1.0)
    } else {
        groups.first().map(|g| g.memory_bytes as f32).unwrap_or(1.0).max(1.0)
    };

    rsx! {
        section { class: "card home-active-apps",
            div { class: "home-card-head",
                div {
                    span { class: "home-card-title", "Active Consumers" }
                    span { class: "home-card-sub", "Grouped by app bundle & children" }
                }
                div { class: "home-head-controls",
                    button {
                        class: if !cpu_mode { "chip active" } else { "chip" },
                        onclick: move |_| sort_by_cpu.set(false),
                        "RAM"
                    }
                    button {
                        class: if cpu_mode { "chip active" } else { "chip" },
                        onclick: move |_| sort_by_cpu.set(true),
                        "CPU"
                    }
                    button {
                        class: "home-link-btn",
                        onclick: move |_| {
                            let mut w = state.write();
                            w.monitor_tab = 1;
                            w.try_go(Screen::Assistant);
                        },
                        "All →"
                    }
                }
            }

            if groups.is_empty() {
                div { class: "home-apps-empty",
                    if auto_poll {
                        div { class: "spinner" }
                        span { "Sampling active apps…" }
                    } else {
                        span { "Live monitoring is off — enable it to inspect active app consumers." }
                        button {
                            class: "btn btn-secondary",
                            onclick: move |_| {
                                state.write().auto_poll_monitor = true;
                                sample_machine(state);
                            },
                            "Start Live Monitor"
                        }
                    }
                }
            } else {
                div { class: "home-consumer-list",
                    for g in groups {
                        {
                            let gkey = g.key.clone();
                            let pid = g.primary_pid;
                            let n_procs = g.members.len();
                            let fill_pct = if cpu_mode {
                                (g.cpu_percent / max_val * 100.0).clamp(4.0, 100.0)
                            } else {
                                (g.memory_bytes as f32 / max_val * 100.0).clamp(4.0, 100.0)
                            };
                            let bar_style = format!("width: {fill_pct:.1}%;");
                            let fill_cls = if cpu_mode { "home-consumer-fill cpu" } else { "home-consumer-fill mem" };
                            let pri_val = if cpu_mode {
                                format!("{:.1}%", g.cpu_percent)
                            } else {
                                format_bytes(g.memory_bytes)
                            };
                            let pri_cls = if cpu_mode {
                                cpu_heat_class(g.cpu_percent)
                            } else {
                                mem_heat_class(g.mem_percent)
                            };
                            let sec_sub = if cpu_mode {
                                format!("{} · {} thr", format_bytes(g.memory_bytes), g.total_threads)
                            } else {
                                format!("{:.1}% CPU · {} thr", g.cpu_percent, g.total_threads)
                            };

                            rsx! {
                                button {
                                    key: "home-grp-{g.key}",
                                    class: "home-consumer-row",
                                    onclick: move |_| {
                                        {
                                            let mut w = state.write();
                                            w.monitor_tab = 1;
                                            w.monitor_proc_view = ProcessViewMode::Grouped;
                                            w.monitor_expanded_groups.insert(gkey.clone());
                                            w.try_go(Screen::Assistant);
                                        }
                                        if state.read().screen == Screen::Assistant {
                                            inspect_process(state, pid);
                                        }
                                    },
                                    GroupAvatar {
                                        name: g.name.clone(),
                                        icon: g.icon_data_url.clone(),
                                    }
                                    div { class: "home-consumer-main",
                                        div { class: "home-consumer-name-line",
                                            span { class: "home-consumer-name", "{g.name}" }
                                            if n_procs > 1 {
                                                span { class: "monitor-group-count-pill", "{n_procs} procs" }
                                            }
                                        }
                                        div { class: "home-consumer-track",
                                            span { class: "{fill_cls}", style: "{bar_style}" }
                                        }
                                    }
                                    div { class: "home-consumer-val",
                                        span { class: "home-consumer-pri {pri_cls}", "{pri_val}" }
                                        span { class: "home-consumer-sec", "{sec_sub}" }
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
fn HomeAuditQueueCard(
    mut state: Signal<AppState>,
    reclaim: Option<u64>,
    threats: Option<usize>,
    threats_clear: bool,
    agents: usize,
    downloads: Option<usize>,
    selected: usize,
    busy: bool,
) -> Element {
    let cleanup_metric = reclaim.map(format_bytes).unwrap_or_else(|| "Not scanned".into());
    let malware_metric = match threats {
        Some(0) => "Clear".into(),
        Some(n) => format!("{n} threats"),
        None => "Not scanned".into(),
    };
    let clutter_metric = match downloads {
        Some(n) => format!("{n} large files"),
        None => "Large Downloads".into(),
    };

    rsx! {
        section { class: "card home-audit-card",
            div { class: "home-card-head",
                div {
                    span { class: "home-card-title", "Smart Audit Queue" }
                    span { class: "home-card-sub", "Toggle tasks or jump straight to a tool" }
                }
                span { class: "home-queue-badge", "{selected} queued" }
            }

            div { class: "home-queue-list",
                HomeQueueRow {
                    state,
                    screen: Screen::Cleanup,
                    title: "Free Up Space",
                    metric: cleanup_metric,
                    selectable: true,
                    success: false,
                }
                HomeQueueRow {
                    state,
                    screen: Screen::Malware,
                    title: "Threat Scan",
                    metric: malware_metric,
                    selectable: !threats_clear,
                    success: threats_clear,
                }
                HomeQueueRow {
                    state,
                    screen: Screen::Performance,
                    title: "Launch Agents",
                    metric: format!("{agents} items"),
                    selectable: true,
                    success: false,
                }
                HomeQueueRow {
                    state,
                    screen: Screen::Clutter,
                    title: "Big Downloads",
                    metric: clutter_metric,
                    selectable: true,
                    success: false,
                }
            }

            div { class: "home-queue-foot",
                span { class: "home-meta-muted", "{selected} of 4 tasks queued" }
                button {
                    class: "btn btn-secondary",
                    disabled: busy || selected == 0,
                    onclick: move |_| run_checked_care(state),
                    if busy { "Running…" } else { "Run Selected" }
                }
            }

            div { class: "home-quick-strip",
                for target in [Screen::Space, Screen::Duplicates, Screen::Snapshots, Screen::Apps] {
                    button {
                        class: "home-quick-btn",
                        onclick: move |_| state.write().try_go(target),
                        img {
                            src: target.icon(),
                            alt: "",
                            width: "16",
                            height: "16",
                        }
                        span { "{target.label()}" }
                    }
                }
            }
        }
    }
}

#[component]
fn HomeQueueRow(
    mut state: Signal<AppState>,
    screen: Screen,
    title: &'static str,
    metric: String,
    selectable: bool,
    success: bool,
) -> Element {
    let picked = selectable && state.read().care_picks.contains(&screen);
    let nav_locked = state.read().navigation_locked();
    let row_cls = if picked {
        "home-queue-row selected"
    } else if !selectable {
        "home-queue-row static"
    } else {
        "home-queue-row"
    };

    rsx! {
        div {
            class: "{row_cls}",
            onclick: move |_| {
                if !selectable {
                    return;
                }
                let mut s = state.write();
                if s.care_picks.contains(&screen) {
                    s.care_picks.remove(&screen);
                } else {
                    s.care_picks.insert(screen);
                }
            },
            if selectable {
                span { class: if picked { "task-check on" } else { "task-check" },
                    if picked { "✓" }
                }
            } else {
                span { class: if success { "task-check on" } else { "task-check" }, "✓" }
            }
            img {
                class: "home-queue-icon",
                src: screen.icon(),
                alt: "",
                width: "22",
                height: "22",
            }
            div { class: "home-queue-info",
                span { class: "home-queue-title", "{title}" }
                span { class: if success { "home-queue-metric ok" } else { "home-queue-metric" }, "{metric}" }
            }
            button {
                class: "home-open-btn",
                disabled: nav_locked,
                onclick: move |evt| {
                    evt.stop_propagation();
                    state.write().try_go(screen);
                },
                "Open →"
            }
        }
    }
}
