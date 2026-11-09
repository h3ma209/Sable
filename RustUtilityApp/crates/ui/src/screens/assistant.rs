use dioxus::prelude::*;

use crate::actions::{
    clear_monitor_history, export_monitor_csv, focus_monitor_process, load_monitor_analytics,
    load_monitor_history, select_monitor_snapshot, toggle_monitor_logging,
};
use crate::components::{
    MonitorAnalyticsBoard, MonitorHealthBadge, MonitorHistoryRow, MonitorProcessDeck,
    MonitorProcessHistoryRows, MonitorRing, MonitorSparkStrip, MonitorStatTile, NoticeBanner,
    PageHeader, Safety,
};
use crate::state::{AppState, Screen};
use crate::util::{
    format_bytes, format_count, format_unix_time, format_uptime, monitor_health_score,
};

#[component]
pub fn AssistantScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let monitor_sampling = state.read().monitor_sampling;
    let sample = state.read().sample.clone();
    let auto_poll = state.read().auto_poll_monitor;
    let tab = state.read().monitor_tab;
    let logging = state.read().monitor_log_enabled;
    let summary = state.read().monitor_summary.clone();
    let spark_cpu = state.read().monitor_spark_cpu.clone();
    let spark_mem = state.read().monitor_spark_mem.clone();
    let spark_swap = state.read().monitor_spark_swap.clone();
    let history = state.read().monitor_history.clone();
    let range = state.read().monitor_range_hours;
    let selected = state.read().monitor_selected_host_id;
    let detail = state.read().monitor_selected_processes.clone();
    let focus = state.read().monitor_process_focus.clone();
    let series = state.read().monitor_process_series.clone();
    let analytics = state.read().monitor_analytics.clone();
    let analytics_loading = state.read().monitor_analytics_loading;

    use_effect(move || {
        let need = state.read().monitor_tab == 2
            && state.read().monitor_analytics.is_none()
            && !state.read().monitor_analytics_loading;
        if need {
            load_monitor_analytics(state);
        }
    });

    let range_chips = rsx! {
        div { class: "monitor-range-row",
            for (label, hours) in [("1h", 1u32), ("6h", 6), ("24h", 24), ("7d", 168)] {
                button {
                    class: if range == hours { "chip active" } else { "chip" },
                    disabled: busy,
                    onclick: move |_| {
                        state.write().monitor_range_hours = hours;
                        if state.read().monitor_tab == 2 {
                            load_monitor_analytics(state);
                        } else {
                            load_monitor_history(state);
                        }
                    },
                    "{label}"
                }
            }
        }
    };

    rsx! {
        PageHeader {
            screen: Screen::Assistant,
            subtitle: "Live vitals, SQLite history, and analytics across CPU, RAM, swap, disk, and processes.",
            safety: Safety::ReadOnly,
            action_label: Some(if auto_poll { "Pause Live".into() } else { "Resume Live".into() }),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| {
                let current = state.read().auto_poll_monitor;
                state.write().auto_poll_monitor = !current;
            })),
        }
        NoticeBanner { state }

        div { class: "monitor-toolbar",
            div { class: "tabs monitor-tabs",
                button {
                    class: if tab == 0 { "tab active" } else { "tab" },
                    onclick: move |_| state.write().monitor_tab = 0,
                    "Live"
                }
                button {
                    class: if tab == 1 { "tab active" } else { "tab" },
                    onclick: move |_| {
                        state.write().monitor_tab = 1;
                        load_monitor_history(state);
                    },
                    "History"
                }
                button {
                    class: if tab == 2 { "tab active" } else { "tab" },
                    onclick: move |_| {
                        state.write().monitor_tab = 2;
                        load_monitor_analytics(state);
                    },
                    "Analytics"
                }
            }
            button {
                class: if logging { "monitor-log-btn on" } else { "monitor-log-btn" },
                disabled: busy,
                onclick: move |_| toggle_monitor_logging(state),
                span { class: "monitor-log-dot" }
                if logging { "Logging on" } else { "Enable history" }
            }
        }

        if let Some(ref sum) = summary {
            div { class: "monitor-meta-bar",
                span { "{format_count(sum.sample_count)} samples" }
                span { "{format_count(sum.process_row_count)} process rows" }
                span { "{format_bytes(sum.db_bytes)} database" }
                if logging {
                    span { class: "monitor-meta-live", "recording every 2s" }
                }
            }
        }

        if tab == 0 {
            if let Some(sample) = sample {
                {
                    let mem_pct = if sample.total_memory > 0 {
                        sample.used_memory as f32 / sample.total_memory as f32
                    } else {
                        0.0
                    };
                    let cpu = (sample.cpu_percent / 100.0).clamp(0.0, 1.0);
                    let swap_pct = if sample.swap_total > 0 {
                        sample.swap_used as f32 / sample.swap_total as f32
                    } else {
                        0.0
                    };
                    let disk_used_pct = if sample.boot_volume.total > 0 {
                        1.0 - (sample.boot_volume.free as f32 / sample.boot_volume.total as f32)
                    } else {
                        0.0
                    };
                    let health = monitor_health_score(cpu, mem_pct, swap_pct);
                    let disk_free = format_bytes(sample.boot_volume.free);
                    let disk_total = format_bytes(sample.boot_volume.total);
                    rsx! {
                        div { class: "monitor-live-header",
                            MonitorHealthBadge { score: health }
                            div { class: "monitor-stat-grid",
                                MonitorStatTile {
                                    title: "Processes",
                                    value: format_count(sample.process_count as u64),
                                    hint: String::from("running now"),
                                }
                                MonitorStatTile {
                                    title: "Uptime",
                                    value: format_uptime(sample.uptime_secs),
                                    hint: String::from("since last boot"),
                                }
                                MonitorStatTile {
                                    title: "Load avg",
                                    value: format!("{:.2}", sample.load_avg_1m),
                                    hint: format!("5m {:.2} · 15m {:.2}", sample.load_avg_5m, sample.load_avg_15m),
                                }
                                MonitorStatTile {
                                    title: "Boot volume",
                                    value: format!("{:.0}% full", disk_used_pct * 100.0),
                                    hint: format!("{disk_free} free of {disk_total}"),
                                }
                                MonitorStatTile {
                                    title: "Available RAM",
                                    value: format_bytes(sample.available_memory),
                                    hint: String::from("ready for apps"),
                                }
                            }
                        }
                        div { class: "monitor-hero",
                            div { class: "monitor-pulse", aria_hidden: "true" }
                            div { class: "monitor-rings",
                                MonitorRing {
                                    label: "CPU",
                                    fraction: cpu,
                                    caption: String::from("Processor load"),
                                    display_value: format!("{:.0}%", sample.cpu_percent),
                                }
                                MonitorRing {
                                    label: "RAM",
                                    fraction: mem_pct,
                                    caption: format!(
                                        "{} / {}",
                                        format_bytes(sample.used_memory),
                                        format_bytes(sample.total_memory)
                                    ),
                                    display_value: format!("{:.0}%", mem_pct * 100.0),
                                }
                                MonitorRing {
                                    label: "SWAP",
                                    fraction: swap_pct,
                                    caption: if sample.swap_total > 0 {
                                        format!(
                                            "{} / {}",
                                            format_bytes(sample.swap_used),
                                            format_bytes(sample.swap_total)
                                        )
                                    } else {
                                        "—".into()
                                    },
                                    display_value: format!("{:.0}%", swap_pct * 100.0),
                                }
                            }
                        }
                        div { class: "monitor-spark-grid",
                            MonitorSparkStrip { title: "CPU", values: spark_cpu, stroke: "#1ed760" }
                            MonitorSparkStrip { title: "Memory", values: spark_mem, stroke: "#5bb8ff" }
                            MonitorSparkStrip { title: "Swap", values: spark_swap, stroke: "#c084fc" }
                        }
                        MonitorProcessDeck { sample: sample.clone() }
                    }
                }
            } else if monitor_sampling {
                div { class: "card loading monitor-loading", div { class: "spinner" } "Sampling…" }
            } else {
                div { class: "card empty",
                    h3 { "Waiting for first sample" }
                    p { "Live mode polls every 2 seconds." }
                }
            }
        } else if tab == 1 {
            div { class: "monitor-history-panel",
                {range_chips}
                div { class: "monitor-range-row",
                    button {
                        class: "btn btn-ghost",
                        disabled: busy,
                        onclick: move |_| load_monitor_history(state),
                        "Refresh"
                    }
                    button {
                        class: "btn btn-ghost",
                        disabled: busy,
                        onclick: move |_| export_monitor_csv(state),
                        "Export CSV"
                    }
                    button {
                        class: "btn btn-ghost",
                        disabled: busy,
                        onclick: move |_| clear_monitor_history(state),
                        "Clear all"
                    }
                }
                if history.is_empty() {
                    div { class: "card empty",
                        h3 { "No history yet" }
                        p { "Enable history — samples record every 2s in the background while Sable runs." }
                    }
                } else {
                    div { class: "monitor-history-layout",
                        div { class: "card monitor-history-list",
                            div { class: "task-title", "Timeline" }
                            for point in history.iter().rev().take(150) {
                                {
                                    let id = point.id;
                                    let mem_pct = if point.mem_total > 0 {
                                        point.mem_used as f32 / point.mem_total as f32
                                    } else {
                                        0.0
                                    };
                                    rsx! {
                                        MonitorHistoryRow {
                                            key: "{id}",
                                            time_label: format_unix_time(point.recorded_at),
                                            cpu: point.cpu_percent,
                                            mem_pct,
                                            selected: selected == Some(id),
                                            on_select: EventHandler::new(move |_| {
                                                select_monitor_snapshot(state, id);
                                            }),
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "card monitor-history-side",
                            if let Some(id) = selected {
                                div { class: "task-title", "Snapshot #{id}" }
                                if detail.is_empty() {
                                    p { class: "row-sub", "Loading processes…" }
                                } else {
                                    MonitorProcessHistoryRows { processes: detail.clone() }
                                    for proc in detail {
                                        {
                                            let name = proc.name.clone();
                                            rsx! {
                                                button {
                                                    class: if focus.as_deref() == Some(name.as_str()) {
                                                        "monitor-proc-chip active"
                                                    } else {
                                                        "monitor-proc-chip"
                                                    },
                                                    onclick: move |_| focus_monitor_process(state, name.clone()),
                                                    "{proc.name}"
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "empty",
                                    h3 { "Pick a moment" }
                                    p { "Select a timeline row to inspect processes at that sample." }
                                }
                            }
                            if let Some(name) = focus {
                                div { class: "monitor-process-series",
                                    div { class: "task-title", "CPU · {name}" }
                                    if series.is_empty() {
                                        p { class: "row-sub", "No points for this process in range." }
                                    } else {
                                        MonitorSparkStrip {
                                            title: "CPU %",
                                            values: series.iter().map(|p| p.cpu_percent / 100.0).collect(),
                                            stroke: "#f5b942",
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            div { class: "monitor-analytics-panel",
                {range_chips}
                div { class: "monitor-range-row",
                    button {
                        class: "btn btn-ghost",
                        disabled: analytics_loading,
                        onclick: move |_| load_monitor_analytics(state),
                        "Refresh"
                    }
                    button {
                        class: "btn btn-ghost",
                        disabled: busy,
                        onclick: move |_| export_monitor_csv(state),
                        "Export CSV"
                    }
                    if !logging {
                        button {
                            class: "btn btn-primary",
                            disabled: busy,
                            onclick: move |_| toggle_monitor_logging(state),
                            "Enable history logging"
                        }
                    }
                }
                if analytics_loading {
                    div { class: "card loading monitor-dash-loading",
                        div { class: "spinner" }
                        "Building dashboard from SQLite history…"
                    }
                } else if let Some(analytics) = analytics {
                    if analytics.samples == 0 {
                        div { class: "card empty",
                            h3 { "No data in this range" }
                            if let Some(sum) = summary.clone() {
                                if sum.sample_count > 0 {
                                    p { "{format_count(sum.sample_count)} total samples in database — widen range (24h or 7d) or refresh after more logging." }
                                } else {
                                    p { "Turn on history logging and leave Sable open a few minutes. Samples save every 2s." }
                                }
                            } else {
                                p { "Turn on history logging and leave Sable open a few minutes." }
                            }
                            if !analytics.insights.is_empty() {
                                ul { class: "monitor-insights",
                                    for line in analytics.insights.clone() {
                                        li { "{line}" }
                                    }
                                }
                            }
                        }
                    } else {
                        MonitorAnalyticsBoard {
                            analytics: analytics.clone(),
                            history: history.clone(),
                            range_label: format!("Last {range}h"),
                        }
                    }
                } else {
                    div { class: "card empty",
                        h3 { "Loading analytics" }
                        p { "Opening analytics…" }
                    }
                }
            }
        }
    }
}
