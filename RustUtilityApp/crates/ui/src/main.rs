mod actions;
pub mod assets;
mod components;
mod screens;
mod state;
mod tray;
mod util;

use dioxus::desktop::{use_wry_event_handler, Config, LogicalSize, WindowBuilder, WindowCloseBehaviour};
use dioxus::desktop::tao::event::{Event, StartCause};
use dioxus::prelude::*;

use actions::ensure_auto_load;
use components::{ConfirmModal, Sidebar};
use screens::*;
use state::{AppState, Screen};
use tray::ensure_tray_icon;

fn main() {
    LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_close_behaviour(WindowCloseBehaviour::LastWindowHides)
                .with_custom_head(format!(
                    r#"<meta name="color-scheme" content="dark"><style>{}</style>"#,
                    assets::STYLESHEET
                ))
                .with_window(
                    WindowBuilder::new()
                        .with_title("Sable")
                        .with_inner_size(LogicalSize::new(1180.0, 800.0))
                        .with_min_inner_size(LogicalSize::new(720.0, 520.0)),
                ),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    let mut state = use_signal(AppState::new);
    let mut checked_updates = use_signal(|| false);

    use_wry_event_handler(move |event, _| {
        if let Event::NewEvents(StartCause::Init) = event {
            ensure_tray_icon();
        }
    });

    use_effect(move || {
        if !*checked_updates.read() {
            checked_updates.set(true);
            actions::check_for_updates_on_startup(state);
        }
        ensure_auto_load(state);
    });

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
            let want_sample = {
                let s = state.read();
                (s.monitor_log_enabled
                    || (s.screen == Screen::Assistant
                        && s.auto_poll_monitor
                        && s.monitor_tab == 0))
                    && !s.busy
            };
            if want_sample {
                actions::sample_machine(state);
            }
        }
    });

    let screen = state.read().screen;
    let busy = state.read().busy;
    // Remount stage on nav so entrance motion plays each screen.
    let _stage_key = format!("{screen:?}");

    rsx! {
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
                            Screen::Duplicates => rsx! { DuplicatesScreen { state } },
                            Screen::Snapshots => rsx! { SnapshotsScreen { state } },
                            Screen::Assistant => rsx! { AssistantScreen { state } },
                        }
                    }
                }
            }
            ConfirmModal { state }
        }
    }
}
