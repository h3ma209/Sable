use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use engine::{MachineSample, ProcessGroup, ProcessSignal, ProcessSnap};

use super::monitor::{cpu_heat_class, mem_heat_class, GroupAvatar};
use crate::actions::{
    inspect_process, renice_monitor_process, request_signal_group, request_signal_process,
};
use crate::state::{AppState, Notice, ProcessScope, ProcessSort, ProcessViewMode};
use crate::util::{
    copy_to_clipboard, format_bytes, format_count, format_proc_time, format_rate, reveal,
};

fn matches_scope(
    scope: ProcessScope,
    is_app: bool,
    user_matches: bool,
    is_active: bool,
    mem_bytes: u64,
) -> bool {
    match scope {
        ProcessScope::All => true,
        ProcessScope::Apps => is_app,
        ProcessScope::Mine => user_matches,
        ProcessScope::Active => is_active,
        ProcessScope::HeavyRam => mem_bytes >= 128 * 1024 * 1024,
    }
}

fn proc_matches_query(proc: &ProcessSnap, query_lower: &str) -> bool {
    query_lower.is_empty()
        || proc.name.to_ascii_lowercase().contains(query_lower)
        || proc.user.to_ascii_lowercase().contains(query_lower)
        || proc.cmd.to_ascii_lowercase().contains(query_lower)
        || proc.pid.to_string().contains(query_lower)
}

fn matches_filter(
    proc: &ProcessSnap,
    query_lower: &str,
    scope: ProcessScope,
    user: &str,
    app_pids: &HashSet<u32>,
) -> bool {
    let scope_ok = matches_scope(
        scope,
        app_pids.contains(&proc.pid),
        !user.is_empty() && proc.user == user,
        proc.cpu_percent >= 0.5 || proc.status == "Run",
        proc.memory_bytes,
    );
    scope_ok && proc_matches_query(proc, query_lower)
}

fn matches_group(
    group: &ProcessGroup,
    query_lower: &str,
    scope: ProcessScope,
    user: &str,
) -> bool {
    let scope_ok = matches_scope(
        scope,
        group.is_app,
        !user.is_empty() && group.members.iter().any(|m| m.proc.user == user),
        group.cpu_percent >= 0.5 || group.members.iter().any(|m| m.proc.status == "Run"),
        group.memory_bytes,
    );
    if !scope_ok {
        return false;
    }
    if query_lower.is_empty() {
        return true;
    }
    group.name.to_ascii_lowercase().contains(query_lower)
        || group.members.iter().any(|m| {
            m.role.to_ascii_lowercase().contains(query_lower) || proc_matches_query(&m.proc, query_lower)
        })
}

fn compare_procs(a: &ProcessSnap, b: &ProcessSnap, sort: ProcessSort, asc: bool) -> std::cmp::Ordering {
    let ord = match sort {
        ProcessSort::Cpu => a
            .cpu_percent
            .partial_cmp(&b.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.memory_bytes.cmp(&b.memory_bytes)),
        ProcessSort::Memory => a.memory_bytes.cmp(&b.memory_bytes).then_with(|| a.pid.cmp(&b.pid)),
        ProcessSort::Pid => a.pid.cmp(&b.pid),
        ProcessSort::Name => a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()),
        ProcessSort::User => a
            .user
            .to_ascii_lowercase()
            .cmp(&b.user.to_ascii_lowercase())
            .then_with(|| a.pid.cmp(&b.pid)),
        ProcessSort::Threads => a.threads.cmp(&b.threads).then_with(|| a.pid.cmp(&b.pid)),
        ProcessSort::DiskIo => (a.disk_read_bytes_per_sec + a.disk_write_bytes_per_sec)
            .cmp(&(b.disk_read_bytes_per_sec + b.disk_write_bytes_per_sec)),
        ProcessSort::Runtime => a.run_time_secs.cmp(&b.run_time_secs),
    };
    if asc { ord } else { ord.reverse() }
}

fn group_primary_user(group: &ProcessGroup) -> &str {
    group
        .members
        .iter()
        .find(|m| m.is_primary)
        .or_else(|| group.members.first())
        .map(|m| m.proc.user.as_str())
        .unwrap_or("")
}

fn compare_groups(
    a: &ProcessGroup,
    b: &ProcessGroup,
    sort: ProcessSort,
    asc: bool,
) -> std::cmp::Ordering {
    let ord = match sort {
        ProcessSort::Cpu => a
            .cpu_percent
            .total_cmp(&b.cpu_percent)
            .then_with(|| a.memory_bytes.cmp(&b.memory_bytes)),
        ProcessSort::Memory => a
            .memory_bytes
            .cmp(&b.memory_bytes)
            .then_with(|| a.primary_pid.cmp(&b.primary_pid)),
        ProcessSort::Pid => a.primary_pid.cmp(&b.primary_pid),
        ProcessSort::Name => a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()),
        ProcessSort::User => group_primary_user(a)
            .to_ascii_lowercase()
            .cmp(&group_primary_user(b).to_ascii_lowercase())
            .then_with(|| a.primary_pid.cmp(&b.primary_pid)),
        ProcessSort::Threads => a
            .total_threads
            .cmp(&b.total_threads)
            .then_with(|| a.primary_pid.cmp(&b.primary_pid)),
        ProcessSort::DiskIo => (a.disk_read_bytes_per_sec + a.disk_write_bytes_per_sec)
            .cmp(&(b.disk_read_bytes_per_sec + b.disk_write_bytes_per_sec)),
        ProcessSort::Runtime => a.max_run_time_secs.cmp(&b.max_run_time_secs),
    };
    if asc { ord } else { ord.reverse() }
}

struct TreeRow<'a> {
    proc: &'a ProcessSnap,
    prefix: String,
}

fn build_process_tree<'a>(
    procs: &[&'a ProcessSnap],
    sort: ProcessSort,
    asc: bool,
) -> Vec<TreeRow<'a>> {
    let by_pid: HashMap<u32, &'a ProcessSnap> = procs.iter().map(|p| (p.pid, *p)).collect();
    let mut children_map: HashMap<Option<u32>, Vec<&'a ProcessSnap>> = HashMap::new();
    let mut roots: Vec<&'a ProcessSnap> = Vec::new();

    for &p in procs {
        let has_visible_parent = p.ppid.is_some_and(|pp| pp != p.pid && by_pid.contains_key(&pp));
        if has_visible_parent {
            children_map.entry(p.ppid).or_default().push(p);
        } else {
            roots.push(p);
        }
    }

    roots.sort_by(|a, b| compare_procs(a, b, sort, asc));
    for list in children_map.values_mut() {
        list.sort_by(|a, b| compare_procs(a, b, sort, asc));
    }

    let mut out = Vec::with_capacity(procs.len());
    let mut visited = HashSet::with_capacity(procs.len());

    fn walk<'b>(
        node: &'b ProcessSnap,
        prefix_ancestor: &str,
        is_last: bool,
        is_root: bool,
        children_map: &HashMap<Option<u32>, Vec<&'b ProcessSnap>>,
        visited: &mut HashSet<u32>,
        out: &mut Vec<TreeRow<'b>>,
    ) {
        if !visited.insert(node.pid) {
            return;
        }
        let line_prefix = if is_root {
            String::new()
        } else if is_last {
            format!("{prefix_ancestor}└─ ")
        } else {
            format!("{prefix_ancestor}├─ ")
        };
        out.push(TreeRow {
            proc: node,
            prefix: line_prefix,
        });

        let next_ancestor = if is_root {
            String::new()
        } else if is_last {
            format!("{prefix_ancestor}   ")
        } else {
            format!("{prefix_ancestor}│  ")
        };

        if let Some(kids) = children_map.get(&Some(node.pid)) {
            let len = kids.len();
            for (idx, kid) in kids.iter().enumerate() {
                walk(kid, &next_ancestor, idx + 1 == len, false, children_map, visited, out);
            }
        }
    }

    let root_len = roots.len();
    for (idx, root) in roots.into_iter().enumerate() {
        walk(root, "", idx + 1 == root_len, true, &children_map, &mut visited, &mut out);
    }

    out
}

fn state_badge_class(status: &str) -> &'static str {
    match status {
        "Run" => "proc-state-pill run",
        "Stop" => "proc-state-pill stop",
        "Zombie" | "Dead" => "proc-state-pill zombie",
        _ => "proc-state-pill sleep",
    }
}

fn role_badge_class(role: &str) -> &'static str {
    match role {
        "Main" => "proc-role-pill role-main",
        "Renderer" | "GPU" => "proc-role-pill role-render",
        "Network" => "proc-role-pill role-net",
        "LSP" | "Extension" | "Plugin" => "proc-role-pill role-ext",
        "Shell" | "Tool" => "proc-role-pill role-tool",
        _ => "proc-role-pill",
    }
}

fn summarize_group_roles(group: &ProcessGroup) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for member in &group.members {
        if let Some((_, n)) = counts.iter_mut().find(|(r, _)| *r == member.role) {
            *n += 1;
        } else {
            counts.push((member.role.as_str(), 1));
        }
    }
    counts
        .into_iter()
        .take(4)
        .map(|(role, n)| {
            if n == 1 {
                format!("1 {role}")
            } else {
                format!("{n} {role}s")
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn toggle_sort(mut state: Signal<AppState>, col: ProcessSort) {
    let mut w = state.write();
    if w.monitor_proc_sort == col {
        w.monitor_proc_asc = !w.monitor_proc_asc;
    } else {
        w.monitor_proc_sort = col;
        w.monitor_proc_asc = matches!(col, ProcessSort::Name | ProcessSort::User | ProcessSort::Pid);
    }
}

#[component]
fn ProcMetricCell(
    value: String,
    heat_cls: &'static str,
    bar_cls: &'static str,
    bar_pct: f32,
    sub: String,
) -> Element {
    let bar = format!("width: {:.1}%;", bar_pct.clamp(0.0, 100.0));
    rsx! {
        td { class: "num",
            div { class: "proc-metric-val {heat_cls}", "{value}" }
            div { class: "proc-micro-bar",
                span { class: "{bar_cls}", style: "{bar}" }
            }
            if !sub.is_empty() {
                div { class: "proc-share-sub", "{sub}" }
            }
        }
    }
}

#[component]
fn ProcLeafRow(
    mut state: Signal<AppState>,
    proc: ProcessSnap,
    selected_pid: Option<u32>,
    row_class: &'static str,
    tree_prefix: String,
    role_badge: String,
    cpu_share: String,
    mem_share: String,
) -> Element {
    let pid = proc.pid;
    let is_sel = selected_pid == Some(pid);
    let cls = if is_sel {
        format!("{row_class} selected")
    } else {
        row_class.to_string()
    };
    let state_cls = state_badge_class(&proc.status);
    let io_total = proc.disk_read_bytes_per_sec + proc.disk_write_bytes_per_sec;
    let kill_name = proc.name.clone();
    let role_cls = role_badge_class(&role_badge);

    rsx! {
        tr {
            class: "{cls}",
            onclick: move |_| inspect_process(state, pid),
            td { class: "mono pid-cell", "{pid}" }
            td { class: "proc-name-cell",
                div { class: "proc-title-line",
                    if !tree_prefix.is_empty() {
                        span { class: "proc-tree-prefix", "{tree_prefix}" }
                    }
                    span { class: "proc-main-name", "{proc.name}" }
                    if !role_badge.is_empty() {
                        span { class: "{role_cls}", "{role_badge}" }
                    }
                }
                if !proc.cmd.is_empty() {
                    div { class: "proc-cmd-sub", "{proc.cmd}" }
                }
            }
            td { class: if proc.is_root { "user-cell root" } else { "user-cell" }, "{proc.user}" }
            td {
                span { class: "{state_cls}", "{proc.status}" }
            }
            ProcMetricCell {
                value: format!("{:.1}%", proc.cpu_percent),
                heat_cls: cpu_heat_class(proc.cpu_percent),
                bar_cls: "cpu",
                bar_pct: proc.cpu_percent,
                sub: cpu_share,
            }
            ProcMetricCell {
                value: format_bytes(proc.memory_bytes),
                heat_cls: mem_heat_class(proc.mem_percent),
                bar_cls: "mem",
                bar_pct: proc.mem_percent,
                sub: mem_share,
            }
            td { class: "num mono col-secondary", "{proc.threads}" }
            td { class: "num mono io-cell col-secondary",
                if io_total > 0 {
                    "{format_rate(io_total)}"
                } else {
                    span { class: "muted", "—" }
                }
            }
            td { class: "num mono col-secondary", "{format_proc_time(proc.run_time_secs)}" }
            td { class: "act",
                button {
                    class: "proc-kill-mini",
                    title: "Terminate (SIGTERM)",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        request_signal_process(state, pid, kill_name.clone(), ProcessSignal::Term);
                    },
                    "End"
                }
            }
        }
    }
}

#[component]
pub fn MonitorProcessWorkspace(mut state: Signal<AppState>, sample: MachineSample) -> Element {
    let query = state.read().monitor_proc_query.clone();
    let sort = state.read().monitor_proc_sort;
    let asc = state.read().monitor_proc_asc;
    let view = state.read().monitor_proc_view;
    let expanded_groups = state.read().monitor_expanded_groups.clone();
    let scope = state.read().monitor_proc_scope;
    let show_all = state.read().monitor_proc_show_all;
    let selected_pid = state.read().monitor_selected_pid;
    let current_user = state.read().current_user.clone();
    let poll_ms = state.read().monitor_poll_ms;

    let q_lower = query.trim().to_ascii_lowercase();

    let app_pids: HashSet<u32> = sample
        .groups
        .iter()
        .filter(|g| g.is_app)
        .flat_map(|g| g.members.iter().map(|m| m.proc.pid))
        .collect();

    let mut filtered_groups: Vec<&ProcessGroup> = sample
        .groups
        .iter()
        .filter(|g| matches_group(g, &q_lower, scope, &current_user))
        .collect();
    filtered_groups.sort_by(|a, b| compare_groups(a, b, sort, asc));

    let mut filtered_procs: Vec<&ProcessSnap> = sample
        .processes
        .iter()
        .filter(|p| matches_filter(p, &q_lower, scope, &current_user, &app_pids))
        .collect();

    let flat_rows: Vec<TreeRow<'_>> = match view {
        ProcessViewMode::Tree => build_process_tree(&filtered_procs, sort, asc),
        ProcessViewMode::Flat => {
            filtered_procs.sort_by(|a, b| compare_procs(a, b, sort, asc));
            filtered_procs
                .iter()
                .map(|&p| TreeRow {
                    proc: p,
                    prefix: String::new(),
                })
                .collect()
        }
        ProcessViewMode::Grouped => Vec::new(),
    };

    let (total_matched, render_cap, count_label) = match view {
        ProcessViewMode::Grouped => {
            let g_count = filtered_groups.len();
            let p_count: usize = filtered_groups.iter().map(|g| g.members.len()).sum();
            let cap = if show_all { g_count } else { g_count.min(120) };
            let lbl = format!(
                "{} groups · {} / {}",
                format_count(g_count as u64),
                format_count(p_count as u64),
                format_count(sample.process_count as u64)
            );
            (g_count, cap, lbl)
        }
        ProcessViewMode::Flat | ProcessViewMode::Tree => {
            let total = flat_rows.len();
            let cap = if show_all { total } else { total.min(120) };
            let lbl = format!(
                "{} / {}",
                format_count(total as u64),
                format_count(sample.process_count as u64)
            );
            (total, cap, lbl)
        }
    };
    let truncated = total_matched > render_cap;

    let visible_expandable_keys: Vec<String> = filtered_groups
        .iter()
        .take(render_cap)
        .filter(|g| g.members.len() > 1)
        .map(|g| g.key.clone())
        .collect();

    let sort_arrow = |col: ProcessSort| -> &'static str {
        if sort == col {
            if asc { " ↑" } else { " ↓" }
        } else {
            ""
        }
    };

    let layout_cls = if selected_pid.is_some() {
        "monitor-proc-layout with-inspector"
    } else {
        "monitor-proc-layout"
    };

    let live_proc = selected_pid.and_then(|spid| {
        sample.processes.iter().find(|p| p.pid == spid).cloned()
    });
    let group_ctx = selected_pid.and_then(|spid| {
        sample.groups.iter().find_map(|g| {
            g.members
                .iter()
                .find(|m| m.proc.pid == spid)
                .map(|m| {
                    (
                        g.name.clone(),
                        m.role.clone(),
                        g.memory_bytes,
                        g.cpu_percent,
                        g.members.len(),
                    )
                })
        })
    });

    rsx! {
        div { class: "monitor-proc-toolbar",
            div { class: "monitor-toolbar-top",
                div { class: "monitor-proc-search-wrap",
                    input {
                        class: "search-input monitor-search",
                        placeholder: "Search app, process, role, command or PID…",
                        value: "{query}",
                        oninput: move |e| {
                            let mut w = state.write();
                            w.monitor_proc_query = e.value();
                            w.monitor_proc_show_all = false;
                        }
                    }
                    if !query.is_empty() {
                        button {
                            class: "chip",
                            onclick: move |_| state.write().monitor_proc_query.clear(),
                            "Clear"
                        }
                    }
                }

                div { class: "monitor-toolbar-right",
                    div { class: "monitor-mode-pills",
                        for (label, target_view, tip) in [
                            ("Grouped", ProcessViewMode::Grouped, "Group app & related subprocesses together"),
                            ("Flat", ProcessViewMode::Flat, "Flat sortable process list"),
                            ("Tree", ProcessViewMode::Tree, "Parent → child process hierarchy (htop F5)"),
                        ] {
                            button {
                                class: if view == target_view { "chip active" } else { "chip" },
                                title: "{tip}",
                                onclick: move |_| state.write().monitor_proc_view = target_view,
                                "{label}"
                            }
                        }
                        if view == ProcessViewMode::Grouped && !visible_expandable_keys.is_empty() {
                            button {
                                class: "chip",
                                title: "Expand all visible process groups",
                                onclick: move |_| {
                                    let mut w = state.write();
                                    for k in visible_expandable_keys.iter() {
                                        w.monitor_expanded_groups.insert(k.clone());
                                    }
                                },
                                "Expand All"
                            }
                            if !expanded_groups.is_empty() {
                                button {
                                    class: "chip",
                                    title: "Collapse all groups",
                                    onclick: move |_| state.write().monitor_expanded_groups.clear(),
                                    "Collapse"
                                }
                            }
                        }
                    }

                    div { class: "monitor-rate-pills",
                        for (label, ms) in [("1s", 1000u64), ("2s", 2000u64), ("5s", 5000u64)] {
                            button {
                                class: if poll_ms == ms { "chip active" } else { "chip" },
                                title: "Refresh cadence",
                                onclick: move |_| state.write().monitor_poll_ms = ms,
                                "{label}"
                            }
                        }
                    }
                }
            }

            div { class: "monitor-toolbar-sub",
                div { class: "monitor-scope-pills",
                    for (label, target) in [
                        ("All", ProcessScope::All),
                        ("Apps", ProcessScope::Apps),
                        ("My Procs", ProcessScope::Mine),
                        ("Active CPU", ProcessScope::Active),
                        ("Heavy RAM", ProcessScope::HeavyRam),
                    ] {
                        button {
                            class: if scope == target { "chip active" } else { "chip" },
                            onclick: move |_| {
                                let mut w = state.write();
                                w.monitor_proc_scope = target;
                                w.monitor_proc_show_all = false;
                            },
                            "{label}"
                        }
                    }
                }
                span { class: "monitor-count-pill", "{count_label}" }
            }
        }

        div { class: "{layout_cls}",
            div { class: "card monitor-table-card",
                div { class: "monitor-table-wrap",
                    table { class: "monitor-table",
                        thead {
                            tr {
                                th { onclick: move |_| toggle_sort(state, ProcessSort::Pid), "PID{sort_arrow(ProcessSort::Pid)}" }
                                th { onclick: move |_| toggle_sort(state, ProcessSort::Name), "Process{sort_arrow(ProcessSort::Name)}" }
                                th { onclick: move |_| toggle_sort(state, ProcessSort::User), "User{sort_arrow(ProcessSort::User)}" }
                                th { "State" }
                                th { class: "num", onclick: move |_| toggle_sort(state, ProcessSort::Cpu), "CPU %{sort_arrow(ProcessSort::Cpu)}" }
                                th { class: "num", onclick: move |_| toggle_sort(state, ProcessSort::Memory), "Memory{sort_arrow(ProcessSort::Memory)}" }
                                th { class: "num col-secondary", onclick: move |_| toggle_sort(state, ProcessSort::Threads), "THR{sort_arrow(ProcessSort::Threads)}" }
                                th { class: "num col-secondary", onclick: move |_| toggle_sort(state, ProcessSort::DiskIo), "Disk I/O{sort_arrow(ProcessSort::DiskIo)}" }
                                th { class: "num col-secondary", onclick: move |_| toggle_sort(state, ProcessSort::Runtime), "Time+{sort_arrow(ProcessSort::Runtime)}" }
                                th { class: "act", "Actions" }
                            }
                        }
                        tbody {
                            if total_matched == 0 {
                                tr {
                                    td { colspan: "10", class: "monitor-empty-cell",
                                        "No processes match current filter."
                                    }
                                }
                            } else if view == ProcessViewMode::Grouped {
                                for group in filtered_groups.into_iter().take(render_cap) {
                                    {
                                        let gkey = group.key.clone();
                                        let gname = group.name.clone();
                                        let gprimary = group.primary_pid;
                                        let count = group.members.len();
                                        let is_multi = count > 1;
                                        let is_exp = is_multi && (expanded_groups.contains(&gkey) || !q_lower.is_empty());

                                        let g_sel = if is_multi {
                                            !is_exp && group.members.iter().any(|m| selected_pid == Some(m.proc.pid))
                                        } else {
                                            selected_pid == Some(gprimary)
                                        };
                                        let row_cls = if g_sel {
                                            "monitor-tr monitor-group-row selected"
                                        } else {
                                            "monitor-tr monitor-group-row"
                                        };
                                        let p_user = group_primary_user(group).to_string();
                                        let running_n = group.members.iter().filter(|m| m.proc.status == "Run").count();
                                        let primary_status = group
                                            .members
                                            .first()
                                            .map(|m| m.proc.status.as_str())
                                            .unwrap_or("Sleep");
                                        let status_label = if is_multi && running_n > 0 {
                                            format!("{running_n} Run")
                                        } else {
                                            primary_status.to_string()
                                        };
                                        let status_cls = if is_multi && running_n > 0 {
                                            "proc-state-pill run"
                                        } else {
                                            state_badge_class(primary_status)
                                        };
                                        let io_total = group.disk_read_bytes_per_sec + group.disk_write_bytes_per_sec;
                                        let role_summary = if is_multi {
                                            summarize_group_roles(group)
                                        } else {
                                            group.members.first().map(|m| m.proc.cmd.clone()).unwrap_or_default()
                                        };
                                        let quit_pids: Vec<u32> = group.members.iter().map(|m| m.proc.pid).collect();
                                        let gkey_click = gkey.clone();
                                        let gkey_chev = gkey.clone();
                                        let gname_quit = gname.clone();

                                        rsx! {
                                            tr {
                                                key: "grp-{gkey}",
                                                class: "{row_cls}",
                                                onclick: move |_| {
                                                    if is_multi {
                                                        let mut w = state.write();
                                                        if w.monitor_expanded_groups.contains(&gkey_click) {
                                                            w.monitor_expanded_groups.remove(&gkey_click);
                                                        } else {
                                                            w.monitor_expanded_groups.insert(gkey_click.clone());
                                                        }
                                                    } else {
                                                        inspect_process(state, gprimary);
                                                    }
                                                },
                                                td { class: "mono pid-cell",
                                                    if is_multi {
                                                        button {
                                                            class: "proc-expand-btn",
                                                            title: if is_exp { "Collapse group" } else { "Expand subprocesses" },
                                                            onclick: move |evt| {
                                                                evt.stop_propagation();
                                                                let mut w = state.write();
                                                                if w.monitor_expanded_groups.contains(&gkey_chev) {
                                                                    w.monitor_expanded_groups.remove(&gkey_chev);
                                                                } else {
                                                                    w.monitor_expanded_groups.insert(gkey_chev.clone());
                                                                }
                                                            },
                                                            if is_exp { "▾" } else { "▸" }
                                                        }
                                                    }
                                                    "{gprimary}"
                                                }
                                                td { class: "proc-name-cell",
                                                    div { class: "proc-title-line group",
                                                        GroupAvatar {
                                                            name: gname.clone(),
                                                            icon: group.icon_data_url.clone(),
                                                        }
                                                        span { class: "proc-main-name", "{gname}" }
                                                        if is_multi {
                                                            span { class: "monitor-group-count-pill", "{count} procs" }
                                                        }
                                                        if group.is_app {
                                                            span { class: "proc-kind-badge is-app", "App" }
                                                        }
                                                    }
                                                    if !role_summary.is_empty() {
                                                        div { class: "proc-cmd-sub", "{role_summary}" }
                                                    }
                                                }
                                                td { class: if group.is_root { "user-cell root" } else { "user-cell" }, "{p_user}" }
                                                td {
                                                    span { class: "{status_cls}", "{status_label}" }
                                                }
                                                ProcMetricCell {
                                                    value: format!("{:.1}%", group.cpu_percent),
                                                    heat_cls: cpu_heat_class(group.cpu_percent),
                                                    bar_cls: "cpu",
                                                    bar_pct: group.cpu_percent,
                                                    sub: String::new(),
                                                }
                                                ProcMetricCell {
                                                    value: format_bytes(group.memory_bytes),
                                                    heat_cls: mem_heat_class(group.mem_percent),
                                                    bar_cls: "mem",
                                                    bar_pct: group.mem_percent,
                                                    sub: format!("{:.1}% RAM", group.mem_percent),
                                                }
                                                td { class: "num mono col-secondary", "{group.total_threads}" }
                                                td { class: "num mono io-cell col-secondary",
                                                    if io_total > 0 {
                                                        "{format_rate(io_total)}"
                                                    } else {
                                                        span { class: "muted", "—" }
                                                    }
                                                }
                                                td { class: "num mono col-secondary", "{format_proc_time(group.max_run_time_secs)}" }
                                                td { class: "act",
                                                    button {
                                                        class: "proc-kill-mini",
                                                        title: if is_multi { "Quit entire group (SIGTERM)" } else { "Terminate (SIGTERM)" },
                                                        onclick: move |evt| {
                                                            evt.stop_propagation();
                                                            if is_multi {
                                                                request_signal_group(
                                                                    state,
                                                                    gname_quit.clone(),
                                                                    quit_pids.clone(),
                                                                    ProcessSignal::Term,
                                                                );
                                                            } else {
                                                                request_signal_process(
                                                                    state,
                                                                    gprimary,
                                                                    gname_quit.clone(),
                                                                    ProcessSignal::Term,
                                                                );
                                                            }
                                                        },
                                                        if is_multi { "Quit" } else { "End" }
                                                    }
                                                }
                                            }
                                            if is_exp {
                                                for (idx, member) in group.members.iter().enumerate() {
                                                    {
                                                        let prefix = if idx + 1 == count {
                                                            "└─ ".to_string()
                                                        } else {
                                                            "├─ ".to_string()
                                                        };
                                                        let cpu_share = if group.cpu_percent >= 1.0 && member.proc.cpu_percent > 0.0 {
                                                            let pct = (member.proc.cpu_percent / group.cpu_percent * 100.0).clamp(0.0, 100.0);
                                                            format!("{pct:.0}% of group")
                                                        } else {
                                                            String::new()
                                                        };
                                                        let mem_share = if group.memory_bytes > 0 && member.proc.memory_bytes > 0 {
                                                            let pct = (member.proc.memory_bytes as f64 / group.memory_bytes as f64 * 100.0).clamp(0.0, 100.0);
                                                            format!("{pct:.0}% of group")
                                                        } else {
                                                            String::new()
                                                        };
                                                        rsx! {
                                                            ProcLeafRow {
                                                                key: "sub-{gkey}-{member.proc.pid}",
                                                                state,
                                                                proc: member.proc.clone(),
                                                                selected_pid,
                                                                row_class: "monitor-tr monitor-sub-row",
                                                                tree_prefix: prefix,
                                                                role_badge: member.role.clone(),
                                                                cpu_share,
                                                                mem_share,
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                for row in flat_rows.into_iter().take(render_cap) {
                                    ProcLeafRow {
                                        key: "flat-{row.proc.pid}",
                                        state,
                                        proc: row.proc.clone(),
                                        selected_pid,
                                        row_class: "monitor-tr",
                                        tree_prefix: row.prefix,
                                        role_badge: String::new(),
                                        cpu_share: String::new(),
                                        mem_share: String::new(),
                                    }
                                }
                            }
                        }
                    }
                }
                if truncated || (show_all && total_matched > 120) {
                    div { class: "monitor-table-foot",
                        if truncated {
                            button {
                                class: "btn btn-ghost",
                                onclick: move |_| state.write().monitor_proc_show_all = true,
                                if view == ProcessViewMode::Grouped {
                                    "Show all {format_count(total_matched as u64)} matching groups"
                                } else {
                                    "Show all {format_count(total_matched as u64)} matching processes"
                                }
                            }
                        } else {
                            button {
                                class: "btn btn-ghost",
                                onclick: move |_| state.write().monitor_proc_show_all = false,
                                "Show top 120 only"
                            }
                        }
                    }
                }
            }

            if let Some(sel_pid) = selected_pid {
                MonitorProcessInspector {
                    state,
                    pid: sel_pid,
                    live_proc,
                    group_ctx,
                }
            }
        }
    }
}

#[component]
fn MonitorProcessInspector(
    mut state: Signal<AppState>,
    pid: u32,
    live_proc: Option<ProcessSnap>,
    group_ctx: Option<(String, String, u64, f32, usize)>,
) -> Element {
    let loading = state.read().monitor_detail_loading;
    let detail = state.read().monitor_process_detail.clone();

    let live_cpu = live_proc.as_ref().map(|p| p.cpu_percent);
    let live_rss = live_proc.as_ref().map(|p| p.memory_bytes);
    let live_threads = live_proc.as_ref().map(|p| p.threads);
    let live_runtime = live_proc.as_ref().map(|p| p.run_time_secs);
    let live_status = live_proc.as_ref().map(|p| p.status.clone());
    let live_name = live_proc.as_ref().map(|p| p.name.clone());
    let live_user = live_proc.as_ref().map(|p| p.user.clone());

    let detail_match = detail.filter(|d| d.pid == pid);

    rsx! {
        aside { class: "card monitor-inspector",
            div { class: "inspector-head",
                div {
                    div { class: "inspector-title",
                        if let Some(ref d) = detail_match {
                            "{d.name}"
                        } else if let Some(ref n) = live_name {
                            "{n}"
                        } else {
                            "PID {pid}"
                        }
                    }
                    div { class: "inspector-sub",
                        if let Some(ref d) = detail_match {
                            span {
                                "PID {d.pid} · {live_user.as_deref().unwrap_or(&d.user)} · {live_status.as_deref().unwrap_or(&d.status)}"
                            }
                        } else if let (Some(u), Some(st)) = (live_user.as_ref(), live_status.as_ref()) {
                            span { "PID {pid} · {u} · {st}" }
                        } else {
                            span { "Loading process details…" }
                        }
                        if let Some((_, ref role, _, _, count)) = group_ctx {
                            if count > 1 {
                                span { class: "{role_badge_class(role)}", "{role}" }
                            }
                        }
                    }
                }
                button {
                    class: "btn btn-ghost inspector-close",
                    title: "Close inspector",
                    onclick: move |_| {
                        let mut w = state.write();
                        w.monitor_selected_pid = None;
                        w.monitor_process_detail = None;
                    },
                    "×"
                }
            }

            if loading && detail_match.is_none() {
                div { class: "loading", div { class: "spinner" } "Inspecting PID {pid}…" }
            } else if let Some(d) = detail_match {
                {
                    let cpu_val = live_cpu.unwrap_or(d.cpu_percent);
                    let rss_val = live_rss.unwrap_or(d.memory_bytes);
                    let thr_val = live_threads.unwrap_or(d.threads);
                    let rt_val = live_runtime.unwrap_or(d.run_time_secs);
                    let cur_status = live_status.unwrap_or_else(|| d.status.clone());
                    let is_stopped = cur_status == "Stop";
                    let n_term = d.name.clone();
                    let n_kill = d.name.clone();
                    let n_pause = d.name.clone();
                    let exe_opt = d.exe.clone();
                    let cmd_copy = d.cmd.clone();

                    rsx! {
                        if let Some((gname, _, gmem, gcpu, gcount)) = group_ctx.clone() {
                            if gcount > 1 {
                                {
                                    let mem_pct = if gmem > 0 {
                                        rss_val as f64 / gmem as f64 * 100.0
                                    } else {
                                        0.0
                                    };
                                    let cpu_pct_share = if gcpu > 0.1 {
                                        format!("{:.0}%", (cpu_val / gcpu * 100.0).clamp(0.0, 100.0))
                                    } else {
                                        "—".to_string()
                                    };
                                    rsx! {
                                        div { class: "inspector-group-share",
                                            div { class: "inspector-label", "Share of {gname} ({gcount} procs)" }
                                            div { class: "inspector-share-row",
                                                span { "RAM: {mem_pct:.1}% ({format_bytes(rss_val)} / {format_bytes(gmem)})" }
                                                span { "CPU: {cpu_pct_share}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "inspector-kpi-grid",
                            div { class: "inspector-kpi",
                                span { class: "k", "CPU" }
                                span { class: "v", "{cpu_val:.1}%" }
                            }
                            div { class: "inspector-kpi",
                                span { class: "k", "RSS" }
                                span { class: "v", "{format_bytes(rss_val)}" }
                            }
                            div { class: "inspector-kpi",
                                span { class: "k", "Virtual" }
                                span { class: "v", "{format_bytes(d.virtual_bytes)}" }
                            }
                            div { class: "inspector-kpi",
                                span { class: "k", "Threads" }
                                span { class: "v", "{thr_val}" }
                            }
                            div { class: "inspector-kpi",
                                span { class: "k", "Nice" }
                                span { class: "v",
                                    if let Some(n) = d.nice { "{n}" } else { "—" }
                                }
                            }
                            div { class: "inspector-kpi",
                                span { class: "k", "Runtime" }
                                span { class: "v", "{format_proc_time(rt_val)}" }
                            }
                        }

                        div { class: "inspector-actions",
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| request_signal_process(state, d.pid, n_term.clone(), ProcessSignal::Term),
                                "Terminate"
                            }
                            button {
                                class: "btn btn-danger",
                                onclick: move |_| request_signal_process(state, d.pid, n_kill.clone(), ProcessSignal::Kill),
                                "Force Kill"
                            }
                            if is_stopped {
                                button {
                                    class: "btn btn-primary",
                                    onclick: move |_| request_signal_process(state, d.pid, n_pause.clone(), ProcessSignal::Cont),
                                    "Resume"
                                }
                            } else {
                                button {
                                    class: "btn btn-ghost",
                                    onclick: move |_| request_signal_process(state, d.pid, n_pause.clone(), ProcessSignal::Stop),
                                    "Pause"
                                }
                            }
                            if let Some(exe) = exe_opt {
                                button {
                                    class: "btn btn-ghost",
                                    onclick: move |_| reveal(&exe),
                                    "Reveal Binary"
                                }
                            }
                            button {
                                class: "btn btn-ghost",
                                onclick: move |_| {
                                    copy_to_clipboard(&d.pid.to_string());
                                    state.write().notice = Some(Notice {
                                        ok: true,
                                        text: format!("Copied PID {} to clipboard", d.pid),
                                        refresh: None,
                                    });
                                },
                                "Copy PID"
                            }
                            if !cmd_copy.is_empty() {
                                button {
                                    class: "btn btn-ghost",
                                    onclick: move |_| {
                                        copy_to_clipboard(&cmd_copy);
                                        state.write().notice = Some(Notice {
                                            ok: true,
                                            text: "Copied command line to clipboard".into(),
                                            refresh: None,
                                        });
                                    },
                                    "Copy Cmd"
                                }
                            }
                            button {
                                class: "btn btn-ghost",
                                onclick: move |_| inspect_process(state, d.pid),
                                "Refresh"
                            }
                        }

                        div { class: "inspector-section",
                            div { class: "inspector-label", "Priority (Renice)" }
                            div { class: "inspector-renice-row",
                                for (lbl, val) in [("Normal (0)", 0i32), ("Low (+10)", 10i32), ("Idle (+19)", 19i32)] {
                                    button {
                                        class: if d.nice == Some(val) { "chip active" } else { "chip" },
                                        onclick: move |_| renice_monitor_process(state, d.pid, val),
                                        "{lbl}"
                                    }
                                }
                            }
                        }

                        if !d.cmd.is_empty() {
                            div { class: "inspector-section",
                                div { class: "inspector-label", "Command Line" }
                                div { class: "inspector-code", "{d.cmd}" }
                            }
                        }

                        if let Some(ref exe) = d.exe {
                            div { class: "inspector-section",
                                div { class: "inspector-label", "Executable" }
                                div { class: "inspector-path", "{exe.display()}" }
                            }
                        }

                        if let Some(ref cwd) = d.cwd {
                            div { class: "inspector-section",
                                div { class: "inspector-label", "Working Directory" }
                                div { class: "inspector-path", "{cwd.display()}" }
                            }
                        }

                        if !d.ancestry.is_empty() {
                            div { class: "inspector-section",
                                div { class: "inspector-label", "Parent Ancestry" }
                                div { class: "inspector-chips",
                                    for (apid, aname) in d.ancestry.iter() {
                                        {
                                            let target = *apid;
                                            rsx! {
                                                button {
                                                    class: "monitor-proc-chip",
                                                    onclick: move |_| inspect_process(state, target),
                                                    "{aname} ({apid})"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if !d.children.is_empty() {
                            div { class: "inspector-section",
                                div { class: "inspector-label", "Child Processes ({d.children.len()})" }
                                div { class: "inspector-chips",
                                    for (cpid, cname) in d.children.iter() {
                                        {
                                            let target = *cpid;
                                            rsx! {
                                                button {
                                                    class: "monitor-proc-chip",
                                                    onclick: move |_| inspect_process(state, target),
                                                    "{cname} ({cpid})"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "inspector-section",
                            div { class: "inspector-label", "Open Network Sockets ({d.sockets.len()})" }
                            if d.sockets.is_empty() {
                                p { class: "row-sub", "No active network sockets" }
                            } else {
                                div { class: "inspector-sockets",
                                    for sock in d.sockets.iter() {
                                        div { class: "inspector-socket-line", "{sock}" }
                                    }
                                }
                            }
                        }

                        if !d.environ.is_empty() {
                            div { class: "inspector-section",
                                details {
                                    summary { class: "inspector-label", "Environment Variables ({d.environ.len()})" }
                                    div { class: "inspector-env",
                                        for (k, v) in d.environ.iter() {
                                            div { class: "inspector-env-row",
                                                span { class: "ek", "{k}" }
                                                span { class: "ev", "{v}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "empty",
                    h3 { "Process unavailable" }
                    p { "PID {pid} exited or access is restricted." }
                }
            }
        }
    }
}
