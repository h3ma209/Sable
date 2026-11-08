use dioxus::prelude::*;

use crate::actions::start_clutter;
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{file_name, format_bytes, pretty, reveal};

#[component]
pub fn ClutterScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let home = state.read().home.clone();
    let clutter = state.read().clutter.clone();
    let loaded = state.read().loaded.contains(&Screen::Clutter);
    let tab = state.read().clutter_tab;

    let filter = state.read().clutter_filter.clone();

    let mut files: Vec<_> = clutter
        .into_iter()
        .filter(|f| {
            filter.is_empty()
                || file_name(&f.path).to_lowercase().contains(&filter.to_lowercase())
                || f.path.display().to_string().to_lowercase().contains(&filter.to_lowercase())
        })
        .collect();
    if tab == 1 {
        files.sort_by(|a, b| {
            file_name(&a.path)
                .to_lowercase()
                .cmp(&file_name(&b.path).to_lowercase())
        });
    } else {
        files.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    }

    rsx! {
        PageHeader {
            screen: Screen::Clutter,
            subtitle: "Large files in Downloads (50 MB+). Reveal in Finder to inspect and manage.",
            safety: Safety::ReadOnly,
            action_label: Some(if busy { "Scanning…".into() } else { "Scan".into() }),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| {
                state.write().loaded.insert(Screen::Clutter);
                state.write().clutter.clear();
                start_clutter(state);
            })),
        }
        NoticeBanner { state }

        div { class: "tabs",
            button {
                class: if tab == 0 { "tab active" } else { "tab" },
                onclick: move |_| state.write().clutter_tab = 0,
                "Largest"
            }
            button {
                class: if tab == 1 { "tab active" } else { "tab" },
                onclick: move |_| state.write().clutter_tab = 1,
                "Name"
            }
        }

        div { class: "toolbar",
            input {
                class: "field",
                style: "max-width: 320px;",
                placeholder: "Filter downloads by name…",
                value: "{filter}",
                oninput: move |e| state.write().clutter_filter = e.value(),
            }
        }

        if files.is_empty() {
            div { class: "card",
                if busy {
                    div { class: "loading", div { class: "spinner" } "Looking through Downloads..." }
                } else if loaded {
                    div { class: "empty",
                        h3 { "No large downloads" }
                        p { "Nothing over 50 MB showed up in Downloads." }
                    }
                } else {
                    div { class: "empty",
                        h3 { "Ready to scan" }
                        p { "Press Scan to find large files in Downloads." }
                    }
                }
            }
        } else {
            div { class: "card",
                for file in files {
                    {
                        let path = file.path.clone();
                        // let path2 = file.path.clone();
                        let name = file_name(&file.path);
                        rsx! {
                            div { class: "row",
                                div { class: "row-body",
                                    div { class: "row-title", "{name}" }
                                    div { class: "row-sub", "{pretty(&home, &file.path)}" }
                                }
                                div { class: "row-meta", "{format_bytes(file.bytes)}" }
                                div { class: "row-actions",
                                    button {
                                        class: "btn btn-ghost",
                                        onclick: move |_| reveal(&path),
                                        "Reveal in Finder"
                                    }
                                    // Move to Trash disabled — users delete manually in Finder
                                    /*
                                    button {
                                        class: "btn btn-ghost",
                                        disabled: busy,
                                        onclick: move |_| {
                                            state.write().confirm = Some(Confirm {
                                                heading: format!("Move {name} to Trash?"),
                                                body: "It will go to the Trash so you can restore it.".to_string(),
                                                action: "Move to Trash",
                                                kind: ConfirmKind::TrashDownload(path2.clone()),
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
