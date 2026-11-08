use dioxus::prelude::*;

use crate::actions::{load, run_tool};
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Confirm, ConfirmKind, Screen, Tool};
use crate::util::{pretty, reveal};

#[component]
pub fn PerformanceScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let tab = state.read().perf_tab;
    let home = state.read().home.clone();
    let agents = state.read().agents.clone();

    rsx! {
        PageHeader {
            screen: Screen::Performance,
            subtitle: "Startup items and maintenance tools.",
            safety: Safety::Admin,
            action_label: Some("Refresh".into()),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| load(state, Screen::Performance))),
        }
        NoticeBanner { state }

        div { class: "tabs",
            button {
                class: if tab == 0 { "tab active" } else { "tab" },
                onclick: move |_| state.write().perf_tab = 0,
                "Startup items"
            }
            button {
                class: if tab == 1 { "tab active" } else { "tab" },
                onclick: move |_| state.write().perf_tab = 1,
                "Tools"
            }
        }

        if tab == 0 {
            if agents.is_empty() {
                div { class: "card",
                    if busy {
                        div { class: "loading", div { class: "spinner" } "Reading launch agents..." }
                    } else {
                        div { class: "empty",
                            h3 { "No startup items" }
                            p { "Press Refresh to list LaunchAgents in your home folder." }
                        }
                    }
                }
            } else {
                div { class: "card",
                    for agent in agents {
                        div { class: "row",
                            div { class: "row-body",
                                div { class: "row-title", "{agent.label}" }
                                div { class: "row-sub", "{pretty(&home, &agent.path)}" }
                            }
                            div { class: "row-actions",
                                button {
                                    class: "btn btn-ghost",
                                    onclick: move |_| reveal(&agent.path),
                                    "Reveal in Finder"
                                }
                            }
                        }
                    }
                }
            }
        } else {
            div { class: "grid grid-2",
                ToolCard {
                    title: "Flush DNS cache",
                    body: "Clears the local DNS cache. May ask for your password.",
                    label: "Flush DNS",
                    disabled: busy,
                    on_click: move |_| run_tool(state, Tool::Dns),
                }
                ToolCard {
                    title: "Spotlight status",
                    body: "Shows whether Spotlight indexing is running.",
                    label: "Check status",
                    disabled: busy,
                    on_click: move |_| run_tool(state, Tool::SpotlightStatus),
                }
                ToolCard {
                    title: "Rebuild Spotlight index",
                    body: "Forces Spotlight to reindex. Takes a while and may ask for your password.",
                    label: "Reindex",
                    disabled: busy,
                    on_click: move |_| {
                        state.write().confirm = Some(Confirm {
                            heading: "Rebuild Spotlight index?".to_string(),
                            body: "macOS will reindex files. This can take a long time and may ask for your password.".to_string(),
                            action: "Reindex",
                            kind: ConfirmKind::Reindex,
                        });
                    },
                }
            }
        }
    }
}

#[component]
fn ToolCard(
    title: &'static str,
    body: &'static str,
    label: &'static str,
    disabled: bool,
    on_click: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "card",
            div { class: "task-title", "{title}" }
            p { class: "page-sub", style: "margin: 8px 0 16px;", "{body}" }
            button {
                class: "btn btn-secondary",
                disabled: disabled,
                onclick: move |_| on_click.call(()),
                "{label}"
            }
        }
    }
}
