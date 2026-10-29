#![cfg_attr(windows, windows_subsystem = "windows")]

macro_rules! primary {
    ($keys:literal) => {
        if cfg!(target_os = "macos") { concat!("cmd-", $keys) } else { concat!("ctrl-", $keys) }
    };
}

mod appearance;
mod assets;
mod dialogs;
mod env_editor;
mod interact;
mod kv_table;
mod prefs;
mod project;
mod project_view;
mod request_tab;
mod settings;
mod ui;
mod workspace;

use gpui::{App, AppContext as _, Bounds, KeyBinding, WindowBounds, WindowDecorations, WindowOptions, size};
use gpui_component::{Root, TitleBar};
use volley_core::report::report_error;

use crate::appearance::dp;
use crate::workspace::{
    CloseProject, CloseTab, NewRequest, NextProject, NextTab, OpenCollection, OpenCommandPalette, OpenSettings, PreviousProject,
    PreviousTab, ReloadCollection, SaveRequest, SendRequest, Workspace,
};

fn main() {
    install_tls_provider();
    let initial = std::env::args().nth(1).map(std::path::PathBuf::from);
    let application = gpui_platform::application().with_assets(assets::AppAssets);
    application.run(move |cx: &mut App| {
        gpui_component::init(cx);
        appearance::init(cx);
        bind_keys(cx);
        let bounds = Bounds::centered(None, size(dp(1440.), dp(900.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some(APP_ID.into()),
                icon: window_icon(),
                window_decorations: Some(WindowDecorations::Client),
                window_min_size: Some(size(dp(860.), dp(520.))),
                ..TitleBar::window_options()
            },
            move |window, cx| {
                window.set_window_title("Kestrel");
                let workspace = cx.new(|cx| Workspace::new(initial.clone(), window, cx));
                cx.new(|cx| Root::new(workspace, window, cx))
            },
        );
        if let Err(error) = opened {
            report_error("open the main window", error);
            cx.quit();
        }
        cx.activate(true);
    });
}

const APP_ID: &str = "dev.kestrel.app";

fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    match image::load_from_memory_with_format(assets::WINDOW_ICON_PNG, image::ImageFormat::Png) {
        Ok(icon) => Some(std::sync::Arc::new(icon.into_rgba8())),
        Err(error) => {
            report_error("decode the window icon", error);
            None
        }
    }
}

fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(primary!("enter"), SendRequest, None),
        KeyBinding::new(primary!("s"), SaveRequest, None),
        KeyBinding::new(primary!("w"), CloseTab, None),
        KeyBinding::new(primary!("n"), NewRequest, None),
        KeyBinding::new(primary!("o"), OpenCollection, None),
        KeyBinding::new(primary!("r"), ReloadCollection, Some("Workspace")),
        KeyBinding::new(primary!(","), OpenSettings, None),
        KeyBinding::new(primary!("k"), OpenCommandPalette, None),
        KeyBinding::new(primary!("p"), OpenCommandPalette, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new(primary!("shift-]"), NextProject, None),
        KeyBinding::new(primary!("shift-["), PreviousProject, None),
        KeyBinding::new(primary!("shift-w"), CloseProject, None),
    ]);
}

fn install_tls_provider() {
    // reqwest and gpui each bring rustls; without a default provider the first TLS connection panics.
    if rustls::crypto::ring::default_provider().install_default().is_err() {
        report_error("install the TLS provider", "another provider was installed first");
    }
}
