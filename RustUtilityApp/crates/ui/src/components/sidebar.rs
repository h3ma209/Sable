use dioxus::prelude::*;

use crate::assets::{APP_ICON, ICON_DOWNLOAD};
use crate::state::{rail_groups, AppState};

#[component]
pub fn Sidebar(state: Signal<AppState>) -> Element {
    let screen = state.read().screen;
    let busy = state.read().busy;
    let nav_locked = state.read().navigation_locked();
    let status = state.read().status;
    let granted = state.read().access == engine::AccessStatus::Granted;
    let available_update = state.read().available_update.clone();
    let (dot, label, hint) = if busy {
        ("busy", "Working", status)
    } else if granted {
        ("ok", "Ready to play", "Full access unlocked")
    } else {
        ("warn", "Limited mode", "Some folders need permission")
    };

    rsx! {
        aside { class: "rail",
            div { class: "brand",
                img {
                    class: "brand-mark",
                    src: APP_ICON,
                    alt: "Sable logo",
                    width: "40",
                    height: "40",
                }
                div { class: "brand-text",
                    span { class: "brand-name", "Sable" }
                    span { class: "brand-sub", "Your Mac, remixed" }
                }
            }
            nav { class: "nav-groups",
                for (group, items) in rail_groups() {
                    div { class: "nav-group",
                        div { class: "nav-group-label", "{group}" }
                        for item in items.iter().copied() {
                            {
                                let is_active = item == screen;
                                let nav_disabled = nav_locked && !is_active;
                                rsx! {
                            button {
                                class: if is_active { "nav-item active" } else { "nav-item" },
                                disabled: nav_disabled,
                                title: if nav_disabled {
                                    "Finish or stop the current task before switching sections"
                                } else {
                                    ""
                                },
                                onclick: move |_| state.write().try_go(item),
                                img {
                                    class: "nav-icon",
                                    src: item.icon(),
                                    alt: "",
                                    width: "22",
                                    height: "22",
                                }
                                span { "{item.label()}" }
                            }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "rail-footer-actions",
                button {
                    class: if available_update.is_some() { "rail-update-btn update-ready" } else { "rail-update-btn" },
                    disabled: busy,
                    title: if let Some(ref tag) = available_update {
                        format!("Update available: {} (running v{}). Click to install.", tag, env!("CARGO_PKG_VERSION"))
                    } else {
                        "Check for Sable updates".to_string()
                    },
                    onclick: move |_| crate::actions::check_for_updates(state),
                    if available_update.is_some() {
                        span { class: "update-dot" }
                    } else {
                        img {
                            class: "update-icon",
                            src: ICON_DOWNLOAD,
                            alt: "",
                            width: "14",
                            height: "14",
                        }
                    }
                    span { class: "update-btn-text",
                        if available_update.is_some() { "Update Available" } else { "Check for Updates" }
                    }
                    if let Some(ref tag) = available_update {
                        span { class: "update-ver-tag update-ver-ready", "{tag}" }
                    } else {
                        span { class: "update-ver-tag", "v{env!(\"CARGO_PKG_VERSION\")}" }
                    }
                }
            }
            div { class: "rail-foot glass",
                span { class: "status-dot {dot}" }
                div { class: "status-meta",
                    span { class: "status-label", "{label}" }
                    span { class: "status-hint", "{hint}" }
                }
            }
        }
    }
}
