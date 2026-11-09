use dioxus::prelude::*;

use crate::actions::{load, start_duplicates};
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{file_name, format_bytes, pretty, reveal};

#[component]
pub fn DuplicatesScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let home = state.read().home.clone();
    let root = state.read().duplicates_root.clone();
    let report = state.read().duplicates.clone();

    rsx! {
        PageHeader {
            screen: Screen::Duplicates,
            subtitle: "Walks the full tree under your folder (build/, Documents/, Library/, …). Skips only node_modules, .git, target, Gradle/npm caches. 64KB+ files.",
            safety: Safety::ReadOnly,
            action_label: Some(if busy { "Scanning…".into() } else { "Scan".into() }),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| start_duplicates(state))),
        }
        NoticeBanner { state }

        div { class: "card", style: "margin-bottom: 14px;",
            div { class: "task-title", style: "margin-bottom: 6px;", "Folder to scan" }
            input {
                class: "field",
                value: "{root}",
                disabled: busy,
                oninput: move |e| state.write().duplicates_root = e.value(),
            }
        }

        if let Some(report) = report {
            div { class: "summary-bar",
                span { strong { "{report.groups.len()}" } " duplicate groups" }
                span { strong { "{format_bytes(report.wasted_bytes)}" } " reclaimable (keep 1 copy each)" }
                span { "{report.scanned} files scanned" }
                if report.cap_hit {
                    span { style: "color: var(--warn);", "Capped at 120k files — use a smaller folder or free space and rescan" }
                }
            }
            if report.groups.is_empty() {
                div { class: "card empty",
                    h3 { "No duplicates found" }
                    p { "Nothing identical over 64KB under this folder." }
                }
            } else {
                for group in report.groups {
                    {
                        let paths = group.paths.clone();
                        let bytes = group.bytes;
                        let count = paths.len();
                        rsx! {
                            div { class: "card", style: "margin-bottom: 12px;",
                                div { class: "task-title", "{count} copies · {format_bytes(bytes)} each" }
                                for path in paths {
                                    {
                                        let path_show = path.clone();
                                        let name = file_name(&path);
                                        rsx! {
                                            div { class: "row",
                                                div { class: "row-body",
                                                    div { class: "row-title", "{name}" }
                                                    div { class: "row-sub", "{pretty(&home, &path)}" }
                                                }
                                                div { class: "row-actions",
                                                    button {
                                                        class: "btn btn-ghost",
                                                        onclick: move |_| reveal(&path_show),
                                                        "Reveal"
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
        } else if busy {
            div { class: "card loading", div { class: "spinner" } "Hashing and comparing…" }
        } else {
            div { class: "card empty",
                h3 { "No scan yet" }
                p { "Set a parent path like /Users/you or ~ — scan walks the whole tree under it." }
                button {
                    class: "btn btn-primary",
                    style: "margin-top: 12px;",
                    onclick: move |_| load(state, Screen::Duplicates),
                    "Load"
                }
            }
        }
    }
}
