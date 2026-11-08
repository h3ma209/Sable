use dioxus::prelude::*;

use crate::assets::DEFAULT_APP_ICON;
use crate::actions::{load, survey_selected};
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{pretty, reveal};

#[component]
pub fn AppsScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let tab = state.read().apps_tab;
    let filter = state.read().app_filter.clone();
    let home = state.read().home.clone();
    let apps: Vec<_> = state
        .read()
        .apps
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            filter.is_empty()
                || a.name.to_lowercase().contains(&filter.to_lowercase())
                || a.bundle_id.to_lowercase().contains(&filter.to_lowercase())
        })
        .map(|(i, a)| (i, a.clone()))
        .collect();
    let leftovers = state.read().leftovers.clone();
    let leftovers_done = state.read().leftovers_done;
    let selected = state.read().selected_app;
    let selected_app_obj = selected.and_then(|idx| state.read().apps.get(idx).cloned());

    rsx! {
        PageHeader {
            screen: Screen::Apps,
            subtitle: "Installed apps and files they may have left behind.",
            safety: Safety::ReadOnly,
            action_label: Some("Scan apps".into()),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| load(state, Screen::Apps))),
        }
        NoticeBanner { state }

        div { class: "tabs",
            button {
                class: if tab == 0 { "tab active" } else { "tab" },
                onclick: move |_| state.write().apps_tab = 0,
                "Installed"
            }
            button {
                class: if tab == 1 { "tab active" } else { "tab" },
                onclick: move |_| state.write().apps_tab = 1,
                "Leftovers"
            }
        }

        if tab == 0 {
            div { class: "toolbar",
                input {
                    class: "field",
                    style: "max-width: 320px;",
                    placeholder: "Filter by name or bundle id",
                    value: "{filter}",
                    oninput: move |e| state.write().app_filter = e.value(),
                }
            }
            if apps.is_empty() {
                div { class: "card",
                    if busy {
                        div { class: "loading", div { class: "spinner" } "Listing apps..." }
                    } else {
                        div { class: "empty",
                            h3 { "No apps listed" }
                            p { "Press Scan apps to load /Applications and ~/Applications." }
                        }
                    }
                }
            } else {
                div { class: "card",
                    for (index, app) in apps {
                        {
                            let active = selected == Some(index);
                            rsx! {
                                div {
                                    class: "row",
                                    style: if active { "background: var(--accent-soft); border-radius: 10px; padding-left: 8px; padding-right: 8px;" } else { "" },
                                    if let Some(ref icon) = app.icon_data_url {
                                        img {
                                            class: "app-row-icon",
                                            src: "{icon}",
                                            alt: "{app.name}",
                                        }
                                    } else {
                                        img {
                                            class: "app-row-icon",
                                            src: DEFAULT_APP_ICON,
                                            alt: "{app.name}",
                                        }
                                    }
                                    div { class: "row-body",
                                        div { class: "row-title", "{app.name}" }
                                        div { class: "row-sub", "{app.bundle_id}" }
                                    }
                                    div { class: "row-actions",
                                        button {
                                            class: "btn btn-ghost",
                                            onclick: move |_| {
                                                state.write().selected_app = Some(index);
                                                state.write().leftovers.clear();
                                                state.write().leftovers_done = false;
                                                state.write().apps_tab = 1;
                                                survey_selected(state);
                                            },
                                            "Leftovers"
                                        }
                                        button {
                                            class: "btn btn-ghost",
                                            onclick: move |_| reveal(&app.path),
                                            "Reveal in Finder"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            if let Some(ref sel) = selected_app_obj {
                div { class: "selected-app-card",
                    if let Some(ref icon) = sel.icon_data_url {
                        img {
                            class: "selected-app-icon",
                            src: "{icon}",
                            alt: "{sel.name}",
                        }
                    } else {
                        img {
                            class: "selected-app-icon",
                            src: DEFAULT_APP_ICON,
                            alt: "{sel.name}",
                        }
                    }
                    div {
                        div { class: "selected-app-title", "{sel.name}" }
                        div { class: "selected-app-sub", "{sel.bundle_id}" }
                    }
                }
            }

            if selected.is_none() {
                div { class: "card empty",
                    h3 { "Pick an app first" }
                    p { "Open Installed, then choose Leftovers on an app." }
                }
            } else if !leftovers_done && busy {
                div { class: "card loading", div { class: "spinner" } "Searching leftovers..." }
            } else if leftovers.is_empty() {
                div { class: "card empty",
                    h3 { "No leftovers found" }
                    p { "Nothing obvious left behind for this app." }
                }
            } else {
                div { class: "card",
                    for hit in leftovers {
                        div { class: "row",
                            div { class: "row-body",
                                div { class: "row-title", "{pretty(&home, &hit.path)}" }
                                div { class: "row-sub", "{hit.root}" }
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
}
