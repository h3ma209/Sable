use std::path::PathBuf;
use std::sync::Once;

use dioxus::desktop::trayicon::{Icon, TrayIconBuilder};

static TRAY_INIT: Once = Once::new();

/// Menu bar icon. Left-click shows the window (dioxus). Quit with Cmd+Q or **Sable → Quit**.
pub fn ensure_tray_icon() {
    TRAY_INIT.call_once(|| {
        let icon = load_tray_icon();
        let _tray = TrayIconBuilder::new()
            .with_tooltip("Sable — click to open · Cmd+Q to quit")
            .with_icon(icon)
            .build()
            .expect("tray icon");
    });
}

fn load_tray_icon() -> Icon {
    let bytes = tray_icon_png_bytes();
    let img = image::load_from_memory(&bytes)
        .expect("tray png")
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).expect("tray rgba icon")
}

fn tray_icon_png_bytes() -> Vec<u8> {
    if let Some(bytes) = read_bundle_png() {
        return bytes;
    }
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/app_icon.png"))
        .expect("dev tray icon png")
}

fn read_bundle_png() -> Option<Vec<u8>> {
    let exe = std::env::current_exe().ok()?;
    let bundled = exe
        .parent()?
        .parent()?
        .join("Resources/assets/app_icon.png");
    std::fs::read(bundled).ok()
}
