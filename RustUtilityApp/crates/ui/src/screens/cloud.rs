use dioxus::prelude::*;

use crate::actions::load;
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{format_bytes, pretty, reveal};

#[component]
pub fn CloudScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let home = state.read().home.clone();
    let cloud = state.read().cloud.clone();
    let biggest = cloud.iter().map(|c| c.bytes).max().unwrap_or(1).max(1);

    rsx! {
        PageHeader {
            screen: Screen::Cloud,
            subtitle: "How much space iCloud Drive and other cloud services use on this Mac. Nothing is signed in, uploaded, or deleted.",
            safety: Safety::ReadOnly,
            action_label: Some("Refresh".into()),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| {
                state.write().loaded.remove(&Screen::Cloud);
                state.write().cloud.clear();
                load(state, Screen::Cloud);
            })),
        }
        NoticeBanner { state }

        if cloud.is_empty() {
            div { class: "card",
                if busy {
                    div { class: "loading", div { class: "spinner" } "Measuring cloud folders..." }
                } else {
                    div { class: "empty",
                        h3 { "No cloud folders found" }
                        p { "Press Refresh to measure again." }
                    }
                }
            }
        } else {
            for folder in cloud {
                {
                    let path = folder.path.clone();
                    let pct = (folder.bytes as f64 / biggest as f64) * 100.0;
                    rsx! {
                        div { class: "card",
                            div { class: "row", style: "border: none; padding: 0;",
                                div { class: "row-body",
                                    div { class: "row-title", "{folder.label}" }
                                    div { class: "row-sub",
                                        if folder.exists {
                                            "{pretty(&home, &folder.path)}"
                                        } else {
                                            "This service is not set up on this Mac."
                                        }
                                    }
                                    if folder.exists {
                                        div { class: "meter",
                                            span { style: "width: {pct}%;" }
                                        }
                                    }
                                }
                                if folder.exists {
                                    div { class: "row-meta", "{format_bytes(folder.bytes)}" }
                                    div { class: "row-actions",
                                        button {
                                            class: "btn btn-ghost",
                                            onclick: move |_| reveal(&path),
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
    }
}
