use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Div, ElementId, Entity, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use gpui_component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use volley_core::report::report_error;

use crate::appearance::{self, MONO_FONT, dp};

pub fn method_color(method: &str, cx: &App) -> Hsla {
    appearance::palette(cx).methods.of(method)
}

pub fn method_label(method: &str, cx: &App) -> impl IntoElement {
    let short = match method {
        "DELETE" => "DEL",
        "OPTIONS" => "OPT",
        other => other,
    };
    div()
        .w(rems(2.9))
        .flex_none()
        .text_xs()
        .font_family(MONO_FONT)
        .font_weight(FontWeight::BOLD)
        .text_color(method_color(method, cx))
        .child(SharedString::from(short.to_string()))
}

pub fn status_color(status: u16, cx: &App) -> Hsla {
    let palette = appearance::palette(cx);
    match status {
        200..=299 => palette.success,
        300..=399 => palette.info,
        400..=499 => palette.warning,
        _ => palette.danger,
    }
}

pub fn format_size(bytes: usize) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

pub fn format_duration(elapsed: Duration) -> String {
    let millis = elapsed.as_millis();
    if millis < 1000 { format!("{millis} ms") } else { format!("{:.2} s", elapsed.as_secs_f64()) }
}

pub fn hint(text: &str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text.to_string())
}

pub fn scroll(id: &'static str, content: AnyElement) -> AnyElement {
    div().id(id).size_full().overflow_y_scroll().child(content).into_any_element()
}

pub fn labelled_input(label: &str, input: &Entity<InputState>, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(div().w(dp(140.)).flex_none().text_xs().text_color(cx.theme().muted_foreground).child(label.to_string()))
        .child(div().flex_1().child(Input::new(input).small()))
}

pub fn name_value_rows(rows: Vec<(String, String)>, cx: &App) -> impl IntoElement {
    v_flex().gap_1().text_xs().children(rows.into_iter().map(|(name, value)| {
        h_flex()
            .gap_3()
            .child(div().w(dp(200.)).flex_none().text_color(cx.theme().muted_foreground).child(name))
            .child(div().flex_1().min_w_0().font_family(MONO_FONT).child(value))
    }))
}

pub fn segmented<T: Clone + PartialEq + 'static>(
    id: &str,
    options: Vec<(T, String)>,
    selected: &T,
    on_pick: impl Fn(T, &mut Window, &mut App) + Clone + 'static,
) -> impl IntoElement {
    h_flex().gap_1().flex_wrap().children(options.into_iter().enumerate().map(|(index, (value, label))| {
        let is_selected = &value == selected;
        let on_pick = on_pick.clone();
        Button::new(ElementId::NamedInteger(SharedString::from(id.to_string()), index as u64))
            .small()
            .label(label)
            .when(is_selected, |button| button.primary())
            .when(!is_selected, |button| button.outline())
            .on_click(move |_, window, cx| on_pick(value.clone(), window, cx))
    }))
}

pub fn section_title(text: &str) -> impl IntoElement {
    div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text.to_string())
}

pub trait ReportUnlessGone {
    fn report_unless_gone(self, context: &str);
}

impl<T> ReportUnlessGone for anyhow::Result<T> {
    fn report_unless_gone(self, context: &str) {
        if let Err(error) = self {
            let text = error.to_string();
            let is_gone = text.contains("released") || text.contains("window not found");
            if !is_gone {
                report_error(context, error);
            }
        }
    }
}
