use dioxus::prelude::*;

use crate::actions::perform_confirm;
use crate::state::{AppState, ConfirmKind};

#[component]
pub fn ConfirmModal(mut state: Signal<AppState>) -> Element {
    let Some(confirm) = state.read().confirm.clone() else {
        return rsx! {};
    };
    let heading = confirm.heading.clone();
    let body = confirm.body.clone();
    let action = confirm.action;
    let kind = confirm.kind.clone();
    let is_update = matches!(kind, ConfirmKind::InstallUpdate { .. });

    rsx! {
        div {
            class: "scrim",
            onclick: move |_| state.write().confirm = None,
            div {
                class: "modal",
                onclick: move |e| e.stop_propagation(),
                h2 { "{heading}" }
                p { style: "white-space: pre-line;", "{body}" }
                div { class: "modal-actions",
                    button {
                        class: if is_update { "btn btn-primary" } else { "btn btn-danger" },
                        onclick: move |_| {
                            state.write().confirm = None;
                            perform_confirm(state, kind.clone());
                        },
                        "{action}"
                    }
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| state.write().confirm = None,
                        "Not now"
                    }
                }
            }
        }
    }
}
