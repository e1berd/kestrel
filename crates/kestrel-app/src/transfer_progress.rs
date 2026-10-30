use gpui::{App, IntoElement, ParentElement as _, Styled as _, div};
use gpui_component::progress::Progress as ProgressBar;
use gpui_component::{h_flex, v_flex};
use kestrel_core::transfer::{Progress, Unit};

use crate::appearance::dp;
use crate::ui::{format_size, hint};

pub fn render(progress: &Progress, cx: &App) -> impl IntoElement {
    let state = progress.snapshot();
    let percent = if state.finished { Some(100.) } else { state.percent() };
    let count = |value: u64| if state.unit == Unit::Bytes { format_size(value as usize) } else { value.to_string() };
    let unit = match state.unit {
        Unit::Bytes => "",
        Unit::Requests => " requests",
        Unit::Files => " files",
        Unit::Paths => " paths",
    };
    let mut detail = match state.total {
        Some(total) => format!("{} / {}{unit}", count(state.completed), count(total)),
        None => format!("{}{unit}", count(state.completed)),
    };
    if state.output_bytes > 0 {
        detail.push_str(&format!(" · {} written", format_size(state.output_bytes as usize)));
    }
    v_flex().gap(dp(6.)).child(h_flex().justify_between().child(div().child(state.stage)).child(hint(&detail, cx))).child(
        ProgressBar::new("transfer-progress")
            .value(percent.unwrap_or(0.))
            .loading(percent.is_none())
            .accessibility_label("Current transfer stage progress"),
    )
}
