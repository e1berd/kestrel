//! Kestrel (code name volley): a free, open-source API client that keeps collections as plain files
//! in git, on GPUI.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod env_editor;
mod interact;
mod kv_table;
mod prefs;
mod project;
mod request_tab;
mod style;
mod workspace;

use gpui::{App, AppContext as _, Bounds, KeyBinding, WindowBounds, WindowDecorations, WindowOptions, px, size};
use gpui_component::{Root, Theme, ThemeMode, TitleBar};

use crate::workspace::{
    CloseTab, NewRequest, NextTab, OpenCollection, PreviousTab, ReloadCollection, SaveRequest, SendRequest, ToggleTheme, Workspace,
};

fn main() {
    // reqwest and gpui each bring rustls; without a default provider the first TLS connection panics.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let initial = std::env::args().nth(1).map(std::path::PathBuf::from);

    let application = gpui_platform::application().with_assets(gpui_kit_assets::Assets);
    application.run(move |cx: &mut App| {
        gpui_component::init(cx);
        let dark = prefs::Prefs::load().dark.unwrap_or(true);
        Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
        cx.bind_keys([
            KeyBinding::new("secondary-enter", SendRequest, None),
            KeyBinding::new("secondary-s", SaveRequest, None),
            KeyBinding::new("secondary-w", CloseTab, None),
            KeyBinding::new("secondary-n", NewRequest, None),
            KeyBinding::new("secondary-o", OpenCollection, None),
            KeyBinding::new("secondary-r", ReloadCollection, Some("Workspace")),
            KeyBinding::new("secondary-shift-t", ToggleTheme, None),
            KeyBinding::new("ctrl-tab", NextTab, None),
            KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        ]);
        let bounds = Bounds::centered(None, size(px(1400.), px(880.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("dev.kestrel.app".into()),
                window_decorations: Some(WindowDecorations::Client),
                window_min_size: Some(size(px(800.), px(500.))),
                ..TitleBar::window_options()
            },
            move |window, cx| {
                window.set_window_title("Kestrel");
                let view = cx.new(|cx| Workspace::new(initial.clone(), window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("the main window");
        cx.activate(true);
    });
}
