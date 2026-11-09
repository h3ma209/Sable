use dioxus::prelude::*;

use engine::{HostPoint, MachineSample, MonitorAnalytics, MonitorBucket, ProcessAggregate, ProcessPoint};
use crate::util::{
    format_bytes, format_count, format_unix_time, gauge_class, monitor_area_path, monitor_chart_max,
    monitor_health_label, monitor_ring_color, monitor_sparkline_points,
};

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

#[component]
pub fn MonitorStatTile(title: &'static str, value: String, hint: String) -> Element {
    rsx! {
        div { class: "monitor-stat-tile",
            div { class: "monitor-stat-title", "{title}" }
            div { class: "monitor-stat-value", "{value}" }
            div { class: "monitor-stat-hint", "{hint}" }
        }
    }
}

#[component]
pub fn MonitorRing(
    label: &'static str,
    fraction: f32,
    caption: String,
    display_value: String,
) -> Element {
    let class = gauge_class(fraction);
    let color = monitor_ring_color(class);
    let pct = fraction.clamp(0.0, 1.0);
    let ring_style = format!("--monitor-pct: {pct:.4}; --monitor-ring: {color};");
    rsx! {
        div { class: "monitor-ring-wrap",
            div { class: "monitor-ring {class}", style: "{ring_style}",
                div { class: "monitor-ring-core",
                    span { class: "monitor-ring-value", "{display_value}" }
                    span { class: "monitor-ring-unit", "{label}" }
                }
            }
            p { class: "monitor-ring-caption", "{caption}" }
        }
    }
}

#[component]
pub fn MonitorSparkStrip(
    title: &'static str,
    values: Vec<f32>,
    stroke: &'static str,
) -> Element {
    let points = monitor_sparkline_points(&values, 320.0, 56.0);
    let has_data = !points.is_empty();
    rsx! {
        div { class: "monitor-spark-card",
            div { class: "monitor-spark-head",
                span { class: "monitor-spark-title", "{title}" }
                span { class: "monitor-spark-live", if has_data { "live" } else { "—" } }
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
pub fn MonitorProcessDeck(sample: MachineSample) -> Element {
    let max_cpu = sample
        .top_cpu
        .first()
        .map(|p| p.cpu_percent)
        .unwrap_or(1.0)
        .max(1.0);
    let max_mem = sample
        .top_mem
        .first()
        .map(|p| p.memory_bytes)
        .unwrap_or(1)
        .max(1);
    rsx! {
        div { class: "monitor-process-deck",
            div { class: "monitor-deck-col monitor-deck-glow-cpu",
                div { class: "monitor-deck-head",
                    span { "CPU leaders" }
                    span { class: "monitor-deck-badge", "TOP {sample.top_cpu.len()}" }
                }
                for (rank, proc) in sample.top_cpu.iter().enumerate() {
                    {
                        let width = (proc.cpu_percent / max_cpu * 100.0).clamp(4.0, 100.0);
                        let bar = format!("width: {width:.1}%;");
                        rsx! {
                            div { class: "monitor-proc-row",
                                span { class: "monitor-proc-rank", "#{rank + 1}" }
                                div { class: "monitor-proc-main",
                                    div { class: "monitor-proc-name", "{proc.name}" }
                                    div { class: "monitor-proc-sub", "PID {proc.pid}" }
                                    div { class: "monitor-proc-track",
                                        span { class: "monitor-proc-fill cpu", style: "{bar}" }
                                    }
                                }
                                span { class: "monitor-proc-val", "{proc.cpu_percent:.1}%" }
                            }
                        }
                    }
                }
            }
            div { class: "monitor-deck-col monitor-deck-glow-mem",
                div { class: "monitor-deck-head",
                    span { "Memory leaders" }
                    span { class: "monitor-deck-badge", "TOP {sample.top_mem.len()}" }
                }
                for (rank, proc) in sample.top_mem.iter().enumerate() {
                    {
                        let width = (proc.memory_bytes as f32 / max_mem as f32 * 100.0).clamp(4.0, 100.0);
                        let bar = format!("width: {width:.1}%;");
                        rsx! {
                            div { class: "monitor-proc-row",
                                span { class: "monitor-proc-rank", "#{rank + 1}" }
                                div { class: "monitor-proc-main",
                                    div { class: "monitor-proc-name", "{proc.name}" }
                                    div { class: "monitor-proc-sub", "PID {proc.pid}" }
                                    div { class: "monitor-proc-track",
                                        span { class: "monitor-proc-fill mem", style: "{bar}" }
                                    }
                                }
                                span { class: "monitor-proc-val", "{format_bytes(proc.memory_bytes)}" }
                            }
                        }
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
