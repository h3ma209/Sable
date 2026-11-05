mod actions;
mod components;
mod screens;
mod state;
mod util;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

use actions::ensure_auto_load;
use components::{ConfirmModal, Sidebar};
use screens::*;
use state::{AppState, Screen};

fn main() {
    LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_window(
                WindowBuilder::new()
                    .with_title("Sable")
                    .with_inner_size(LogicalSize::new(1180.0, 800.0))
                    .with_min_inner_size(LogicalSize::new(720.0, 520.0)),
            ),
        )
        .launch(App);
}

const CSS: Asset = asset!("/assets/styles.css");
const _FONT_REGULAR: Asset = asset!("/assets/Sable-Regular.ttf");
const _FONT_SEMIBOLD: Asset = asset!("/assets/Sable-SemiBold.ttf");

#[component]
fn App() -> Element {
    let mut state = use_signal(AppState::new);

    use_effect(move || {
        ensure_auto_load(state);
    });

    let screen = state.read().screen;
    let busy = state.read().busy;
    // Remount stage on nav so entrance motion plays each screen.
    let _stage_key = format!("{screen:?}");

    rsx! {
        document::Stylesheet { href: CSS }
        div {
            class: "app",
            tabindex: "0",
            autofocus: true,
            onkeydown: move |evt: KeyboardEvent| {
                if evt.key() == Key::Escape {
                    state.write().confirm = None;
                } else if evt.modifiers().meta() {
                    let target = match evt.key() {
                        Key::Character(ref s) if s == "1" => Some(Screen::Care),
                        Key::Character(ref s) if s == "2" => Some(Screen::Cleanup),
                        Key::Character(ref s) if s == "3" => Some(Screen::Apps),
                        Key::Character(ref s) if s == "4" => Some(Screen::Performance),
                        Key::Character(ref s) if s == "5" => Some(Screen::Protection),
                        Key::Character(ref s) if s == "6" => Some(Screen::Malware),
                        Key::Character(ref s) if s == "7" => Some(Screen::Clutter),
                        Key::Character(ref s) if s == "8" => Some(Screen::Space),
                        Key::Character(ref s) if s == "9" => Some(Screen::Cloud),
                        Key::Character(ref s) if s == "0" => Some(Screen::Assistant),
                        _ => None,
                    };
                    if let Some(scr) = target {
                        state.write().go(scr);
                        actions::load(state, scr);
                    }
                }
            },
            div { class: "app-ambient", aria_hidden: "true",
                div { class: "blob blob-a" }
                div { class: "blob blob-b" }
                div { class: "blob blob-c" }
            }
            div { class: "app-noise", aria_hidden: "true" }
            Sidebar { state }
            main { class: "main",
                div { class: if busy { "busy-strip on" } else { "busy-strip" } }
                div { class: "main-scroll",
                    div { class: "screen-stage", key: "{_stage_key}",
                        match screen {
                            Screen::Care => rsx! { CareScreen { state } },
                            Screen::Cleanup => rsx! { CleanupScreen { state } },
                            Screen::Apps => rsx! { AppsScreen { state } },
                            Screen::Performance => rsx! { PerformanceScreen { state } },
                            Screen::Protection => rsx! { ProtectionScreen { state } },
                            Screen::Malware => rsx! { MalwareScreen { state } },
                            Screen::Clutter => rsx! { ClutterScreen { state } },
                            Screen::Space => rsx! { SpaceScreen { state } },
                            Screen::Cloud => rsx! { CloudScreen { state } },
                            Screen::Assistant => rsx! { AssistantScreen { state } },
                        }
                    }
                }
            }
            ConfirmModal { state }
        }
    }
}
