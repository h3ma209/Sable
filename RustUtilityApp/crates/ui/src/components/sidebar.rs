use dioxus::prelude::*;

use crate::state::{rail_groups, AppState};

const APP_ICON: Asset = asset!("/assets/app_icon.png");
const ICON_DOWNLOAD: Asset = asset!("/assets/icons/download.svg");

#[component]
pub fn Sidebar(state: Signal<AppState>) -> Element {
    let screen = state.read().screen;
    let busy = state.read().busy;
    let status = state.read().status;
    let granted = state.read().access == engine::AccessStatus::Granted;
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
                            button {
                                class: if item == screen { "nav-item active" } else { "nav-item" },
                                onclick: move |_| state.write().go(item),
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
            div { class: "rail-footer-actions",
                button {
                    class: "rail-update-btn",
                    disabled: busy,
                    title: "Check for Sable updates",
                    onclick: move |_| crate::actions::check_for_updates(state),
                    img {
                        class: "update-icon",
                        src: ICON_DOWNLOAD,
                        alt: "",
                        width: "14",
                        height: "14",
                    }
                    span { class: "update-btn-text", "Check for Updates" }
                    span { class: "update-ver-tag", "v{env!(\"CARGO_PKG_VERSION\")}" }
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
