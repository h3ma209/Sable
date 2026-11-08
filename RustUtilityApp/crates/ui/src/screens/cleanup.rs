use dioxus::prelude::*;

use crate::actions::load;
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{describe_junk, format_bytes, pretty, reveal};

#[component]
pub fn CleanupScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let home = state.read().home.clone();
    let junk = state.read().junk.clone();
    let total: u64 = junk.iter().map(|j| j.bytes).sum();
    // let junk_for_action = junk.clone();

    rsx! {
        PageHeader {
            screen: Screen::Cleanup,
            subtitle: "Caches and logs. Reveal folders in Finder to inspect and clean manually.",
            safety: Safety::ReadOnly,
            action_label: Some("Refresh".into()),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| {
                state.write().loaded.remove(&Screen::Cleanup);
                state.write().junk.clear();
                load(state, Screen::Cleanup);
            })),
        }
        NoticeBanner { state }

        if junk.is_empty() {
            div { class: "card",
                if busy {
                    div { class: "loading", div { class: "spinner" } "Measuring caches..." }
                } else {
                    div { class: "empty",
                        h3 { "Nothing to clean yet" }
                        p { "Press Refresh to measure caches and logs." }
                    }
                }
            }
        } else {
            div { class: "summary-bar", style: "display: flex; justify-content: space-between; align-items: center;",
                span { strong { "{format_bytes(total)}" } " reclaimable across {junk.len()} folders" }
                // Deleting/trashing disabled for safety — users manage in Finder
                /*
                button {
                    class: "btn btn-danger",
                    disabled: busy || junk_for_action.is_empty(),
                    onclick: move |_| {
                        let existing_paths: Vec<_> = junk_for_action.iter().filter(|j| j.exists).map(|j| j.path.clone()).collect();
                        let count = existing_paths.len();
                        state.write().confirm = Some(Confirm {
                            heading: format!("Clean All ({count} folders)?"),
                            body: format!(
                                "Move {count} cache and log folders ({}) to Trash? You can restore them from Trash if needed.",
                                format_bytes(total)
                            ),
                            action: "Clean All to Trash",
                            kind: ConfirmKind::TrashRoots(existing_paths),
                        });
                    },
                    "Clean All"
                }
                */
            }
            div { class: "card",
                for item in junk {
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
                                    // Move to Trash disabled — users delete manually in Finder
                                    /*
                                    button {
                                        class: "btn btn-ghost",
                                        disabled: busy || !exists,
                                        onclick: move |_| {
                                            let home = state.read().home.clone();
                                            state.write().confirm = Some(Confirm {
                                                heading: format!("Move {label} to Trash?"),
                                                body: format!(
                                                    "{} will be moved to the Trash. You can put it back from there.",
                                                    pretty(&home, &path)
                                                ),
                                                action: "Move to Trash",
                                                kind: ConfirmKind::TrashRoot(path.clone()),
                                            });
                                        },
                                        "Trash"
                                    }
                                    */
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
