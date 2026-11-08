use dioxus::prelude::*;

use crate::actions::start_scan;
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{format_bytes, format_count, pretty, reveal};

#[component]
pub fn ProtectionScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let root = state.read().root.clone();
    let filename_query = state.read().filename_query.clone();
    let watch = state.read().watch.clone();
    let home = state.read().home.clone();
    let detections = state.read().detections.clone();
    let scanned = state.read().scanned;
    let detection_count = state.read().detection_count;
    let errors = state.read().errors;
    let last_path = state.read().last_path.clone();
    let scan_ran = state.read().scan_ran;

    rsx! {
        PageHeader {
            screen: Screen::Protection,
            subtitle: "Find files by name and extension. Does not change files.",
            action_label: Some(if busy { "Scanning…".into() } else { "Scan".into() }),
            action_enabled: !busy,
            safety: Safety::ReadOnly,
            on_action: Some(EventHandler::new(move |_| start_scan(state))),
        }
        NoticeBanner { state }

        div { class: "card", style: "margin-bottom: 14px;",
            div { class: "toolbar",
                div { style: "flex: 1.2; min-width: 180px;",
                    div { class: "task-title", style: "margin-bottom: 6px;", "Folder" }
                    input {
                        class: "field",
                        value: "{root}",
                        disabled: busy,
                        oninput: move |e| state.write().root = e.value(),
                    }
                }
                div { style: "flex: 1; min-width: 140px;",
                    div { class: "task-title", style: "margin-bottom: 6px;", "Filename (fuzzy / pattern)" }
                    input {
                        class: "field",
                        value: "{filename_query}",
                        disabled: busy,
                        placeholder: "e.g. sable, setup, log",
                        oninput: move |e| state.write().filename_query = e.value(),
                    }
                }
                div { style: "flex: 0.8; min-width: 120px;",
                    div { class: "task-title", style: "margin-bottom: 6px;", "Extensions" }
                    input {
                        class: "field",
                        value: "{watch}",
                        disabled: busy,
                        placeholder: "dmg,pkg,zip",
                        oninput: move |e| state.write().watch = e.value(),
                    }
                }
            }
            if busy || scan_ran {
                div { class: "summary-bar", style: "margin: 12px 0 0; box-shadow: none;",
                    span { strong { "{format_count(scanned)}" } " checked" }
                    span { strong { "{format_count(detection_count)}" } " matches" }
                    span { strong { "{format_count(errors)}" } " errors" }
                }
                if !last_path.is_empty() {
                    p { class: "row-sub", style: "margin-top: 8px;", "Last: {last_path}" }
                }
            }
        }

        if detections.is_empty() {
            div { class: "card",
                if busy {
                    div { class: "loading", div { class: "spinner" } "Scanning files..." }
                } else if scan_ran {
                    div { class: "empty",
                        h3 { "No matches" }
                        p { "Nothing matching those names or extensions showed up in this folder." }
                    }
                } else {
                    div { class: "empty",
                        h3 { "Ready to scan" }
                        p { "Choose a folder and optional name or extensions, then press Scan." }
                    }
                }
            }
        } else {
            div { class: "card",
                for hit in detections {
                    div { class: "row",
                        div { class: "row-body",
                            div { class: "row-title", "{pretty(&home, &hit.path)}" }
                            div { class: "row-sub", "{hit.note} · {format_bytes(hit.size)}" }
                        }
                        div { class: "row-actions",
                            button {
                                class: "btn btn-ghost",
                                onclick: move |_| reveal(&hit.path),
                                "Reveal in Finder"
                            }
                        }
                    }
                }
            }
        }
    }
}
