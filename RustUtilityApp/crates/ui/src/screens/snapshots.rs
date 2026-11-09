use dioxus::prelude::*;

use crate::actions::start_snapshots;
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};

#[component]
pub fn SnapshotsScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let report = state.read().snapshots.clone();

    rsx! {
        PageHeader {
            screen: Screen::Snapshots,
            subtitle: "APFS local snapshots (Time Machine / macOS updates). Read-only list.",
            safety: Safety::ReadOnly,
            action_label: Some(if busy { "Listing…".into() } else { "Refresh".into() }),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| start_snapshots(state))),
        }
        NoticeBanner { state }

        if let Some(report) = report {
            div { class: "summary-bar",
                span { "Volume: " strong { "{report.volume}" } }
                span { strong { "{report.snapshots.len()}" } " snapshots" }
            }
            if report.snapshots.is_empty() {
                div { class: "card empty",
                    h3 { "No local snapshots" }
                    p { "Either none exist or tmutil could not list them." }
                }
            } else {
                div { class: "card",
                    for snap in report.snapshots {
                        div { class: "row",
                            div { class: "row-body",
                                div { class: "row-title", "{snap.name}" }
                            }
                        }
                    }
                }
            }
            div { class: "card", style: "margin-top: 14px;",
                div { class: "task-title", "Thin snapshots" }
                p { class: "row-sub", style: "margin-top: 8px;", "{report.thin_hint}" }
            }
        } else if busy {
            div { class: "card loading", div { class: "spinner" } "Reading tmutil…" }
        } else {
            div { class: "card empty",
                h3 { "Not checked yet" }
                p { "Press Refresh to list local APFS snapshots on /." }
            }
        }
    }
}
