use std::path::PathBuf;

use dioxus::prelude::*;

use crate::actions::load;
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{describe_junk, format_bytes, pretty, reveal};

#[component]
fn JunkList(home: PathBuf, items: Vec<engine::JunkRoot>, busy: bool) -> Element {
    if items.is_empty() {
        return rsx! {
            div { class: "card",
                if busy {
                    div { class: "loading", div { class: "spinner" } "Measuring folders..." }
                } else {
                    div { class: "empty",
                        h3 { "Nothing listed yet" }
                        p { "Press Refresh to measure known folders." }
                    }
                }
            }
        };
    }
    let total: u64 = items.iter().map(|j| j.bytes).sum();
    rsx! {
        div { class: "summary-bar",
            span { strong { "{format_bytes(total)}" } " across {items.len()} folders" }
        }
        div { class: "card",
            for item in items {
                {
                    let path = item.path.clone();
                    let path_show = path.clone();
                    let label = item.label.clone();
                    let bytes = item.bytes;
                    let exists = item.exists;
                    let pretty_path = pretty(&home, &path);
                    rsx! {
                        div { class: "row",
                            div { class: "row-body",
                                div { class: "row-title", "{label}" }
                                div { class: "row-sub",
                                    if exists { "{pretty_path}" } else { "Missing" }
                                    " · {describe_junk(&label)}"
                                }
                            }
                            div { class: "row-meta", "{format_bytes(bytes)}" }
                            div { class: "row-actions",
                                button {
                                    class: "btn btn-ghost",
                                    onclick: move |_| reveal(&path_show),
                                    "Reveal in Finder"
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
pub fn CleanupScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let home = state.read().home.clone();
    let tab = state.read().cleanup_tab;
    let junk = state.read().junk.clone();
    let dev_junk = state.read().dev_junk.clone();

    rsx! {
        PageHeader {
            screen: Screen::Cleanup,
            subtitle: "Caches, logs, and dev tool leftovers. Reveal in Finder to clean.",
            safety: Safety::ReadOnly,
            action_label: Some("Refresh".into()),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| {
                state.write().loaded.remove(&Screen::Cleanup);
                state.write().junk.clear();
                state.write().dev_junk.clear();
                load(state, Screen::Cleanup);
            })),
        }
        NoticeBanner { state }

        div { class: "tabs",
            button {
                class: if tab == 0 { "tab active" } else { "tab" },
                onclick: move |_| state.write().cleanup_tab = 0,
                "Caches & logs"
            }
            button {
                class: if tab == 1 { "tab active" } else { "tab" },
                onclick: move |_| state.write().cleanup_tab = 1,
                "Dev junk"
            }
        }

        if tab == 0 {
            JunkList { home: home.clone(), items: junk, busy }
        } else {
            JunkList { home, items: dev_junk, busy }
        }
    }
}
