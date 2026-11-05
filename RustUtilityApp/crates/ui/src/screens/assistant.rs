use dioxus::prelude::*;

use crate::actions::sample_machine;
use crate::components::{NoticeBanner, PageHeader, Safety};
use crate::state::{AppState, Screen};
use crate::util::{format_bytes, gauge_class};

#[component]
pub fn AssistantScreen(mut state: Signal<AppState>) -> Element {
    let busy = state.read().busy;
    let sample = state.read().sample;
    let auto_poll = state.read().auto_poll_monitor;

    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
            if state.read().screen == Screen::Assistant && state.read().auto_poll_monitor && !state.read().busy {
                sample_machine(state);
            }
        }
    });

    rsx! {
        PageHeader {
            screen: Screen::Assistant,
            subtitle: "Live telemetry of processor and memory. Auto-refreshes every 2 seconds.",
            safety: Safety::ReadOnly,
            action_label: Some(if auto_poll { "Pause Live" } else { "Resume Live" }.into()),
            action_enabled: !busy,
            on_action: Some(EventHandler::new(move |_| {
                let current = state.read().auto_poll_monitor;
                state.write().auto_poll_monitor = !current;
            })),
        }
        NoticeBanner { state }

        if let Some(sample) = sample {
            {
                let mem_pct = if sample.total_memory > 0 {
                    sample.used_memory as f32 / sample.total_memory as f32
                } else {
                    0.0
                };
                let cpu = (sample.cpu_percent / 100.0).clamp(0.0, 1.0);
                let cpu_class = gauge_class(cpu);
                let mem_class = gauge_class(mem_pct);
                let verdict_text = if mem_pct > 0.85 {
                    "Memory is nearly full. Quit apps you are not using."
                } else if cpu > 0.8 {
                    "The processor is very busy. Something heavy is running."
                } else {
                    "Everything looks fine."
                };
                let verdict_ok = mem_pct <= 0.85 && cpu <= 0.8;
                let cpu_style = match cpu_class {
                    "danger" => "color: var(--danger)",
                    "warn" => "color: var(--warn)",
                    _ => "color: var(--success)",
                };
                let mem_style = match mem_class {
                    "danger" => "color: var(--danger)",
                    "warn" => "color: var(--warn)",
                    _ => "color: var(--success)",
                };
                let cpu_width = format!("{:.0}%", cpu * 100.0);
                let mem_width = format!("{:.0}%", mem_pct * 100.0);
                let mem_caption = format!(
                    "{} of {} in use",
                    format_bytes(sample.used_memory),
                    format_bytes(sample.total_memory)
                );
                rsx! {
                    div { class: "grid grid-2",
                        div { class: "card",
                            div { class: "task-title", "PROCESSOR" }
                            div { class: "stat-value", style: "{cpu_style}", "{sample.cpu_percent:.0}%" }
                            div { class: "meter {cpu_class}",
                                span { style: "width: {cpu_width};" }
                            }
                            p { class: "row-sub", style: "margin-top: 10px;", "How busy the CPU is." }
                        }
                        div { class: "card",
                            div { class: "task-title", "MEMORY" }
                            div { class: "stat-value", style: "{mem_style}", "{(mem_pct * 100.0):.0}%" }
                            div { class: "meter {mem_class}",
                                span { style: "width: {mem_width};" }
                            }
                            p { class: "row-sub", style: "margin-top: 10px;", "{mem_caption}" }
                        }
                    }
                    div {
                        class: if verdict_ok { "notice ok" } else { "notice err" },
                        style: "margin-top: 14px;",
                        "{verdict_text}"
                    }
                }
            }
        } else if busy {
            div { class: "card loading", div { class: "spinner" } "Taking a reading..." }
        } else {
            div { class: "card empty",
                h3 { "No reading yet" }
                p { "Press Refresh for a live CPU and memory sample." }
            }
        }
    }
}
