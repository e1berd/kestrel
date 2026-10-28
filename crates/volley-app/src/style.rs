//! Colours and small pieces the views share.

use gpui::{App, Hsla, IntoElement, ParentElement as _, SharedString, Styled as _, div, px};
use gpui_component::ActiveTheme as _;

/// Each method in the colour people know it by from other API clients.
pub fn method_color(method: &str, cx: &App) -> Hsla {
    let theme = cx.theme();
    match method {
        "GET" => theme.green,
        "POST" => theme.yellow,
        "PUT" => theme.blue,
        "PATCH" => theme.magenta,
        "DELETE" => theme.red,
        "HEAD" | "OPTIONS" => theme.cyan,
        _ => theme.muted_foreground,
    }
}

pub fn method_label(method: &str, cx: &App) -> impl IntoElement {
    let short = match method {
        "DELETE" => "DEL",
        "OPTIONS" => "OPT",
        "PATCH" => "PATCH",
        other => other,
    };
    div()
        .w(px(40.))
        .flex_none()
        .text_xs()
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(method_color(method, cx))
        .child(SharedString::from(short.to_string()))
}

pub fn status_color(status: u16, cx: &App) -> Hsla {
    let theme = cx.theme();
    match status {
        200..=299 => theme.green,
        300..=399 => theme.blue,
        400..=499 => theme.yellow,
        _ => theme.red,
    }
}

pub fn size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1048576.0)
    }
}

pub fn duration(elapsed: std::time::Duration) -> String {
    let ms = elapsed.as_millis();
    if ms < 1000 { format!("{ms} ms") } else { format!("{:.2} s", elapsed.as_secs_f64()) }
}
