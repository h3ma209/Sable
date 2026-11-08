use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use engine::computer_root;

use crate::actions::{space_back, space_forward, space_open, start_space, stop_space};
use crate::assets::{ICON_FOLDER, ICON_HOUSE};
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{file_name, format_bytes, format_count, is_dev_artifact, pretty, reveal, space_category_style};

#[component]
pub fn SpaceScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let home = state.read().home.clone();
    let space_root = state.read().space_root.clone();
    let space = state.read().space.clone();
    let can_back = !state.read().space_history.is_empty();
    let can_forward = !state.read().space_future.is_empty();
    let filter = state.read().space_filter.clone();
    let sort_mode = state.read().space_sort;

    let done_bytes = state.read().space_progress_bytes;
    let total_bytes = state.read().space_progress_total_bytes.max(done_bytes.max(1));
    let items = state.read().space_progress_items;
    let current_path = state.read().space_progress_current.clone();
    let left_bytes = total_bytes.saturating_sub(done_bytes);
    let pct = if total_bytes > 0 {
        ((done_bytes as f64 / total_bytes as f64) * 100.0).clamp(1.0, 99.0)
    } else {
        5.0
    };

    let is_mac = space_root == computer_root().display().to_string() || space_root == "/";
    let is_home = !is_mac;

    let root_path = PathBuf::from(&space_root);
    let mut breadcrumbs: Vec<(String, PathBuf)> = Vec::new();
    let mut curr: Option<&Path> = Some(&root_path);
    while let Some(p) = curr {
        let name = if p == Path::new("/") {
            "Macintosh HD".to_string()
        } else if p == home {
            "~ (Home)".to_string()
        } else {
            file_name(p)
        };
        breadcrumbs.push((name, p.to_path_buf()));
        curr = p.parent();
    }
    breadcrumbs.reverse();

    rsx! {
        PageHeader {
            screen: Screen::Space,
            subtitle: "One scan builds a map. Click folders or breadcrumbs to drill in without rescanning.",
            safety: Safety::Trash,
            action_label: Some(if busy { "Stop".into() } else { "Rescan".into() }),
            action_enabled: true,
            on_action: Some(EventHandler::new(move |_| {
                if busy {
                    stop_space(state);
                } else {
                    start_space(state, true);
                }
            })),
        }
        NoticeBanner { state }

        // Folder Selection Row (distinguished with icons, paths, default Home active)
        div { class: "space-folder-selector",
            span { class: "space-selector-label", "Folder:" }
            div { class: "space-folder-pills",
                button {
                    class: if is_home { "space-folder-pill active" } else { "space-folder-pill" },
                    title: "Scan user home directory (~)",
                    disabled: busy,
                    onclick: move |_| {
                        state.write().space_root = home.display().to_string();
                        state.write().space_history.clear();
                        state.write().space_future.clear();
                        start_space(state, true);
                    },
                    img {
                        class: "folder-pill-icon",
                        src: ICON_HOUSE,
                        alt: "",
                        width: "14",
                        height: "14",
                    }
                    span { class: "folder-pill-name", "Home" }
                    span { class: "folder-pill-path", "~" }
                }
                button {
                    class: if is_mac { "space-folder-pill active" } else { "space-folder-pill" },
                    title: "Scan entire Mac disk (/)",
                    disabled: busy,
                    onclick: move |_| {
                        state.write().space_root = computer_root().display().to_string();
                        state.write().space_history.clear();
                        state.write().space_future.clear();
                        start_space(state, true);
                    },
                    img {
                        class: "folder-pill-icon",
                        src: ICON_FOLDER,
                        alt: "",
                        width: "14",
                        height: "14",
                    }
                    span { class: "folder-pill-name", "This Mac" }
                    span { class: "folder-pill-path", "/" }
                }
            }
            input {
                class: "field",
                style: "max-width: 220px; margin-left: auto;",
                placeholder: "Filter items…",
                value: "{filter}",
                oninput: move |e| state.write().space_filter = e.value(),
            }
        }

        // Navigation Row below folder buttons: Back / Forward history + Breadcrumbs trail
        div { class: "space-nav-row",
            div { class: "space-history-btns",
                button {
                    class: "btn btn-secondary space-nav-btn",
                    disabled: !can_back || busy,
                    title: "Go back",
                    onclick: move |_| space_back(state),
                    "← Back"
                }
                button {
                    class: "btn btn-secondary space-nav-btn",
                    disabled: !can_forward || busy,
                    title: "Go forward",
                    onclick: move |_| space_forward(state),
                    "Forward →"
                }
            }
            div { class: "space-breadcrumbs",
                for (idx, (bname, bpath)) in breadcrumbs.iter().enumerate() {
                    {
                        let bpath_clone = bpath.clone();
                        let is_last = idx == breadcrumbs.len() - 1;
                        rsx! {
                            button {
                                class: if is_last { "chip active" } else { "chip" },
                                style: "padding: 2px 8px; font-size: 12px; cursor: pointer; border-radius: 4px;",
                                onclick: move |_| {
                                    if !is_last {
                                        space_open(state, bpath_clone.clone());
                                    }
                                },
                                "{bname}"
                            }
                            if !is_last {
                                span { class: "breadcrumb-sep", "›" }
                            }
                        }
                    }
                }
            }
        }

        if let Some(report) = space {
            {
                let total = report.total.max(1);
                let mut children: Vec<_> = report.children
                    .iter()
                    .filter(|c| {
                        filter.is_empty()
                            || file_name(&c.path).to_lowercase().contains(&filter.to_lowercase())
                    })
                    .cloned()
                    .collect();

                match sort_mode {
                    0 => children.sort_by(|a, b| b.bytes.cmp(&a.bytes)),
                    1 => children.sort_by(|a, b| file_name(&a.path).to_lowercase().cmp(&file_name(&b.path).to_lowercase())),
                    _ => children.sort_by(|a, b| {
                        let (c_a, _) = space_category_style(a.is_directory, &file_name(&a.path));
                        let (c_b, _) = space_category_style(b.is_directory, &file_name(&b.path));
                        c_a.cmp(c_b).then_with(|| b.bytes.cmp(&a.bytes))
                    }),
                }

                rsx! {
                    div { class: "summary-bar", style: "display: flex; justify-content: space-between; align-items: center;",
                        div { style: "display: flex; gap: 12px; align-items: center;",
                            span { "Folder: " strong { "{format_bytes(report.total)}" } }
                            span { strong { "{children.len()}" } " visible" }
                        }
                        div { class: "tabs", style: "margin: 0;",
                            button {
                                class: if sort_mode == 0 { "tab active" } else { "tab" },
                                style: "padding: 3px 10px; font-size: 11px;",
                                onclick: move |_| state.write().space_sort = 0,
                                "Size"
                            }
                            button {
                                class: if sort_mode == 1 { "tab active" } else { "tab" },
                                style: "padding: 3px 10px; font-size: 11px;",
                                onclick: move |_| state.write().space_sort = 1,
                                "Name"
                            }
                            button {
                                class: if sort_mode == 2 { "tab active" } else { "tab" },
                                style: "padding: 3px 10px; font-size: 11px;",
                                onclick: move |_| state.write().space_sort = 2,
                                "Type"
                            }
                        }
                    }

                    // Full-width clean list
                    div { class: "card",
                        for child in children {
                            {
                                let path = child.path.clone();
                                let path_open = child.path.clone();
                                let path_reveal = child.path.clone();
                                let bytes = child.bytes;
                                let is_dir = child.is_directory;
                                let pct = ((child.bytes as f64 / total as f64) * 100.0).clamp(0.0, 100.0);
                                let name = file_name(&child.path);
                                let (cat_color, cat_label) = space_category_style(is_dir, &name);
                                let is_dev = is_dev_artifact(&name);

                                rsx! {
                                    div { class: "row", style: "align-items: center;",
                                        div {
                                            class: "row-body",
                                            style: "cursor: pointer;",
                                            onclick: move |_| {
                                                if is_dir {
                                                    space_open(state, path_open.clone());
                                                } else {
                                                    reveal(&path_open);
                                                }
                                            },
                                            div { style: "display: flex; align-items: center; gap: 8px;",
                                                if is_dir {
                                                    svg {
                                                        width: "15",
                                                        height: "15",
                                                        view_box: "0 0 24 24",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        style: "opacity: 0.7; flex-shrink: 0;",
                                                        path { d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" }
                                                    }
                                                } else {
                                                    svg {
                                                        width: "15",
                                                        height: "15",
                                                        view_box: "0 0 24 24",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        style: "opacity: 0.6; flex-shrink: 0;",
                                                        path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                                        polyline { points: "14 2 14 8 20 8" }
                                                    }
                                                }
                                                span { class: "row-title", "{name}" }
                                                span {
                                                    style: "font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: 3px; background: {cat_color}22; color: {cat_color}; border: 1px solid {cat_color}55;",
                                                    "{cat_label}"
                                                }
                                                if is_dev {
                                                    span {
                                                        style: "font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: 3px; background: rgba(245, 158, 11, 0.15); color: #f59e0b; border: 1px solid rgba(245, 158, 11, 0.4);",
                                                        "DEV ARTIFACT"
                                                    }
                                                }
                                                if is_dir {
                                                    span { style: "opacity: 0.4; font-size: 11px; margin-left: auto;", "Drill in ›" }
                                                }
                                            }
                                            div { class: "row-sub", "{pretty(&home, &path)} · {pct:.1}%" }
                                            div { class: "meter",
                                                span { style: "width: {pct}%; background: {cat_color};" }
                                            }
                                        }
                                        div { class: "row-meta", style: "display: flex; align-items: center; gap: 8px;",
                                            span { "{format_bytes(bytes)}" }
                                            button {
                                                class: "btn btn-ghost",
                                                style: "display: inline-flex; align-items: center; gap: 4px; font-size: 11px; padding: 4px 8px; border-radius: 6px; background: rgba(255, 255, 255, 0.05); border: 1px solid rgba(255, 255, 255, 0.1);",
                                                title: "Reveal in Finder",
                                                onclick: move |e| {
                                                    e.stop_propagation();
                                                    reveal(&path_reveal);
                                                },
                                                svg {
                                                    width: "14",
                                                    height: "14",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    path { d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" }
                                                }
                                                span { "Finder" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else if busy {
            div { class: "space-progress-card",
                    div { class: "space-progress-header",
                        div { class: "space-progress-title-group",
                            div { class: "spinner", style: "width: 18px; height: 18px; border-width: 2px;" }
                            div { class: "space-progress-title", "Scanning & mapping space…" }
                        }
                        div { style: "display: flex; align-items: center; gap: 10px;",
                            span { class: "space-progress-pct", "{pct:.0}%" }
                            button {
                                class: "btn btn-secondary",
                                style: "padding: 3px 10px; font-size: 11px; border-radius: 6px;",
                                onclick: move |_| stop_space(state),
                                "Stop"
                            }
                        }
                    }

                    // Progress bar
                    div { class: "space-progress-track",
                        div {
                            class: "space-progress-bar",
                            style: "width: {pct}%;",
                        }
                    }

                    // Stats: Done vs Left vs Items
                    div { class: "space-progress-stats",
                        div { class: "stat-box",
                            span { class: "stat-label", "Amount done" }
                            span { class: "stat-val done", "{format_bytes(done_bytes)}" }
                        }
                        div { class: "stat-box",
                            span { class: "stat-label", "Amount left" }
                            span { class: "stat-val left", "{format_bytes(left_bytes)}" }
                        }
                        div { class: "stat-box",
                            span { class: "stat-label", "Items mapped" }
                            span { class: "stat-val", "{format_count(items)}" }
                        }
                    }

                    if !current_path.is_empty() {
                        div { class: "space-progress-path",
                            span { class: "path-prefix", "Scanning:" }
                            span { class: "path-text", "{current_path}" }
                        }
                    }
                }
        } else {
            div { class: "card empty",
                h3 { "Scan to map this folder" }
                p { "Press Scan once. Then click folders to drill in without waiting again." }
            }
        }
    }
}
