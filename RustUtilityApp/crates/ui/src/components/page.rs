use dioxus::prelude::*;

use crate::state::Screen;
use crate::util::screen_kicker;

#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum Safety {
    ReadOnly,
    Trash,
    Admin,
}

#[component]
pub fn PageHeader(
    screen: Screen,
    subtitle: &'static str,
    safety: Safety,
    action_label: Option<&'static str>,
    action_enabled: bool,
    on_action: Option<EventHandler<()>>,
) -> Element {
    let (badge_class, badge_text) = match safety {
        Safety::ReadOnly => ("safe", "Listen only — disk stays untouched"),
        Safety::Trash => ("trash", "Goes to Trash — easy undo"),
        Safety::Admin => ("admin", "May ask for your Mac password"),
    };

    rsx! {
        header { class: "page-head",
            div { class: "page-titles",
                div { class: "page-kicker", "{screen_kicker(screen)}" }
                h1 { class: "page-title", "{screen.label()}" }
                p { class: "page-sub", "{subtitle}" }
                div { class: "badge-row",
                    span { class: "badge {badge_class}", "{badge_text}" }
                }
            }
            if let Some(label) = action_label {
                button {
                    class: "btn btn-primary",
                    disabled: !action_enabled,
                    onclick: move |_| {
                        if let Some(handler) = on_action {
                            handler.call(());
                        }
                    },
                    "{label}"
                }
            }
        }
    }
}
