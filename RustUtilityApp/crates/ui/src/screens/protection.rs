use dioxus::prelude::*;

use crate::actions::{start_scan, stop_scan};
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{file_name, format_bytes, format_count, pretty, reveal, space_category_style};

#[component]
pub fn ProtectionScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let root = state.read().root.clone();
    let filename_query = state.read().filename_query.clone();
    let watch = state.read().watch.clone();
    let exclude_dirs = state.read().exclude_dirs.clone();
    let home = state.read().home.clone();
    let detections = state.read().detections.clone();
    let scanned = state.read().scanned;
    let detection_count = state.read().detection_count;
    let errors = state.read().errors;
    let last_path = state.read().last_path.clone();
    let scan_ran = state.read().scan_ran;
    let total_bytes = detections.iter().map(|d| d.size).sum::<u64>();

    rsx! {
        PageHeader {
            screen: Screen::Protection,
            subtitle: "Find files by name and extension with directory exclusion. Does not change files.",
            action_label: Some(if busy { "Stop".into() } else { "Scan".into() }),
            action_enabled: true,
            safety: Safety::ReadOnly,
            on_action: Some(EventHandler::new(move |_| {
                if busy {
                    stop_scan(state);
                } else {
                    start_scan(state);
                }
            })),
        }
        NoticeBanner { state }

        div { class: "card", style: "margin-bottom: 14px; padding: 16px;",
            div { style: "display: grid; grid-template-columns: 1.2fr 1fr; gap: 12px; margin-bottom: 12px;",
                div {
                    div { class: "task-title", style: "margin-bottom: 6px; font-size: 12px;", "Folder" }
                    input {
                        class: "field",
                        value: "{root}",
                        disabled: busy,
                        placeholder: "/path/to/search",
                        oninput: move |e| state.write().root = e.value(),
                    }
                }
                div {
                    div { class: "task-title", style: "margin-bottom: 6px; font-size: 12px;", "Filename (fuzzy / pattern)" }
                    input {
                        class: "field",
                        value: "{filename_query}",
                        disabled: busy,
                        placeholder: "e.g. sable, setup, log",
                        oninput: move |e| state.write().filename_query = e.value(),
                    }
                }
            }
            div { style: "display: grid; grid-template-columns: 1fr 1.2fr; gap: 12px;",
                div {
                    div { class: "task-title", style: "margin-bottom: 6px; font-size: 12px;", "Extensions" }
                    input {
                        class: "field",
                        value: "{watch}",
                        disabled: busy,
                        placeholder: "dmg, pkg, zip (leave blank for any)",
                        oninput: move |e| state.write().watch = e.value(),
                    }
                }
                div {
                    div { class: "task-title", style: "margin-bottom: 6px; font-size: 12px;", "Exclude Folders" }
                    input {
                        class: "field",
                        value: "{exclude_dirs}",
                        disabled: busy,
                        placeholder: "e.g. node_modules, .gradle, npm, target",
                        oninput: move |e| state.write().exclude_dirs = e.value(),
                    }
                }
            }
            if busy || scan_ran {
                div { class: "summary-bar", style: "margin: 14px 0 0; box-shadow: none; display: flex; justify-content: space-between; align-items: center;",
                    div { style: "display: flex; gap: 16px; align-items: center;",
                        if busy {
                            div { class: "spinner", style: "width: 14px; height: 14px; border-width: 2px;" }
                        }
                        span { strong { "{format_count(scanned)}" } " checked" }
                        span { strong { "{format_count(detection_count)}" } " matches" }
                        if errors > 0 {
                            span { style: "color: #ef4444;", strong { "{format_count(errors)}" } " errors" }
                        }
                    }
                    if busy {
                        button {
                            class: "btn btn-secondary",
                            style: "padding: 3px 10px; font-size: 11px; border-radius: 6px;",
                            onclick: move |_| stop_scan(state),
                            "Stop Scan"
                        }
                    }
                }
                if busy && !last_path.is_empty() {
                    p { class: "row-sub", style: "margin-top: 8px; font-family: monospace; font-size: 11px;",
                        "Scanning: {last_path}"
                    }
                }
            }
        }

        if detections.is_empty() {
            div { class: "card",
                if busy {
                    div { class: "loading",
                        div { class: "spinner" }
                        div { style: "margin-top: 8px;", "Scanning files..." }
                        button {
                            class: "btn btn-secondary",
                            style: "margin-top: 12px; padding: 4px 14px; font-size: 12px;",
                            onclick: move |_| stop_scan(state),
                            "Stop"
                        }
                    }
                } else if scan_ran {
                    div { class: "empty",
                        h3 { "No matches" }
                        p { "Nothing matching those names or extensions showed up in this folder." }
                    }
                } else {
                    div { class: "empty",
                        h3 { "Ready to scan" }
                        p { "Choose a folder and optional name, extensions, and exclusions, then press Scan." }
                    }
                }
            }
        } else {
            div { class: "summary-bar", style: "margin-bottom: 8px; display: flex; justify-content: space-between; align-items: center;",
                div { style: "display: flex; gap: 14px; align-items: center;",
                    span { strong { "{detections.len()}" } " matches found" }
                    span { "Total size: " strong { "{format_bytes(total_bytes)}" } }
                }
            }
            div { class: "card",
                for hit in detections {
                    {
                        let path_reveal = hit.path.clone();
                        let name = file_name(&hit.path);
                        let (cat_color, cat_label) = space_category_style(false, &name);
                        let parent_dir = hit.path.parent().map(|p| pretty(&home, p)).unwrap_or_default();

                        rsx! {
                            div {
                                class: "row",
                                style: "align-items: center; padding: 10px 12px; cursor: pointer;",
                                onclick: move |_| reveal(&path_reveal),
                                div {
                                    style: "width: 32px; height: 32px; border-radius: 6px; background: {cat_color}18; display: flex; align-items: center; justify-content: center; flex-shrink: 0; border: 1px solid {cat_color}33;",
                                    svg {
                                        width: "16",
                                        height: "16",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "{cat_color}",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                        polyline { points: "14 2 14 8 20 8" }
                                    }
                                }
                                div { class: "row-body", style: "min-width: 0;",
                                    div { style: "display: flex; align-items: center; gap: 8px; flex-wrap: wrap;",
                                        span { class: "row-title", style: "font-weight: 600; font-size: 13px;", "{name}" }
                                        if !hit.extension.is_empty() {
                                            span {
                                                style: "font-size: 10px; font-weight: 700; padding: 1px 6px; border-radius: 4px; background: {cat_color}22; color: {cat_color}; border: 1px solid {cat_color}55; text-transform: uppercase;",
                                                ".{hit.extension}"
                                            }
                                        }
                                        span {
                                            style: "font-size: 10px; font-weight: 500; padding: 1px 6px; border-radius: 4px; background: rgba(255, 255, 255, 0.06); color: var(--ink-mute);",
                                            "{cat_label}"
                                        }
                                    }
                                    div { class: "row-sub", style: "display: flex; align-items: center; gap: 6px; margin-top: 3px;",
                                        span { style: "opacity: 0.6; font-size: 11px;", "{parent_dir}" }
                                        span { style: "opacity: 0.3;", "·" }
                                        span { style: "color: var(--ink-mute); font-size: 11px;", "{hit.note}" }
                                    }
                                }
                                div { class: "row-meta", style: "display: flex; align-items: center; gap: 12px;",
                                    span { style: "font-weight: 600; font-size: 13px;", "{format_bytes(hit.size)}" }
                                    button {
                                        class: "btn btn-ghost",
                                        style: "padding: 4px 8px; font-size: 11px; display: flex; align-items: center; gap: 5px;",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            reveal(&hit.path);
                                        },
                                        svg {
                                            width: "13",
                                            height: "13",
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
}
