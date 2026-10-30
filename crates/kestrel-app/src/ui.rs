use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Div, ElementId, Entity, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, rems,
};
use gpui_component::button::Button;
use gpui_component::input::{Input, InputState};
use gpui_component::{ActiveTheme as _, h_flex, v_flex};
use kestrel_core::report::report_error;

use crate::appearance::{self, MONO_FONT, dp};

pub fn method_color(method: &str, cx: &App) -> Hsla {
    appearance::palette(cx).methods.of(method)
}

fn short_method(method: &str) -> String {
    match method {
        "DELETE" => "DEL".into(),
        "OPTIONS" => "OPT".into(),
        other => other.to_string(),
    }
}

pub fn method_badge(method: &str, cx: &App) -> impl IntoElement {
    div()
        .flex_none()
        .w(rems(2.2))
        .font_family(MONO_FONT)
        .text_size(rems(0.64))
        .font_weight(FontWeight::BOLD)
        .text_color(method_color(method, cx))
        .child(short_method(method))
}

pub fn method_label(method: &str, cx: &App) -> impl IntoElement {
    div()
        .flex_none()
        .font_family(MONO_FONT)
        .text_size(rems(0.66))
        .font_weight(FontWeight::BOLD)
        .text_color(method_color(method, cx))
        .child(short_method(method))
}

pub fn count_badge(count: usize, cx: &App) -> impl IntoElement {
    let palette = appearance::palette(cx);
    div().px(rems(0.4)).rounded_full().text_size(rems(0.7)).bg(palette.hover).text_color(palette.muted).child(count.to_string())
}

pub fn section_tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    count: Option<usize>,
    is_active: bool,
    cx: &App,
) -> gpui::Stateful<Div> {
    let palette = appearance::palette(cx);
    h_flex()
        .id(id)
        .flex_none()
        .h_full()
        .gap(rems(0.4))
        .px(rems(0.15))
        .items_center()
        .border_b_2()
        .border_color(if is_active { palette.accent } else { gpui::transparent_black() })
        .cursor_pointer()
        .text_size(rems(0.84))
        .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
        .text_color(if is_active { palette.text } else { palette.muted })
        .when(!is_active, |el| el.hover(|el| el.text_color(palette.text)))
        .child(label.into())
        .when_some(count.filter(|n| *n > 0), |el, n| el.child(count_badge(n, cx)))
}

pub fn section_tab_strip(id: &'static str, tabs: impl IntoIterator<Item = impl IntoElement>, cx: &App) -> impl IntoElement {
    h_flex()
        .id(id)
        .flex_none()
        .h(rems(2.6))
        .px(dp(16.))
        .gap(dp(18.))
        .border_b_1()
        .border_color(appearance::palette(cx).border_soft)
        .overflow_x_scroll()
        .children(tabs)
}

pub fn kbd(key: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let palette = appearance::palette(cx);
    div()
        .flex_none()
        .px(rems(0.35))
        .rounded(dp(5.))
        .border_1()
        .border_color(palette.border)
        .border_b_2()
        .bg(palette.raised)
        .font_family(MONO_FONT)
        .text_size(rems(0.68))
        .text_color(palette.muted)
        .child(key.into())
}

pub fn keys(combination: &str, cx: &App) -> impl IntoElement {
    h_flex().gap(dp(3.)).children(combination.split(' ').map(|key| kbd(key.to_string(), cx)))
}

pub fn icon_tile(icon: impl Into<gpui_component::Icon>, size: f32, cx: &App) -> impl IntoElement {
    let palette = appearance::palette(cx);
    div()
        .flex_none()
        .size(dp(size))
        .rounded(dp(size * 0.3))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(palette.border_soft)
        .bg(palette.raised)
        .shadow(appearance::elevation::lifted(cx))
        .text_color(palette.muted)
        .child(icon.into().size(dp(size * 0.45)))
}

pub fn chip(text: impl Into<SharedString>, color: Hsla, cx: &App) -> Div {
    let palette = appearance::palette(cx);
    h_flex()
        .flex_none()
        .gap(rems(0.38))
        .px(rems(0.65))
        .py(rems(0.25))
        .rounded_full()
        .bg(palette.tint(color))
        .text_color(color)
        .text_size(rems(0.78))
        .font_weight(FontWeight::SEMIBOLD)
        .child(div().size(dp(6.)).rounded_full().bg(color))
        .child(text.into())
}

pub fn segmented<T: Clone + PartialEq + 'static>(
    id: &str,
    options: Vec<(T, String)>,
    selected: &T,
    on_pick: impl Fn(T, &mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
    let palette = appearance::palette(cx);
    let selected_background = if cx.theme().is_dark() { palette.raised } else { palette.background };
    let shadow = appearance::elevation::lifted(cx);
    h_flex().flex_wrap().p(dp(3.)).gap(dp(2.)).rounded(dp(10.)).bg(palette.surface).children(options.into_iter().enumerate().map(
        |(index, (value, label))| {
            let is_selected = &value == selected;
            let on_pick = on_pick.clone();
            div()
                .id(ElementId::NamedInteger(SharedString::from(id.to_string()), index as u64))
                .px(rems(0.7))
                .py(rems(0.26))
                .rounded(dp(7.))
                .cursor_pointer()
                .text_size(rems(0.8))
                .font_weight(if is_selected { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                .when(is_selected, |el| el.bg(selected_background).text_color(palette.text).shadow(shadow.clone()))
                .when(!is_selected, |el| el.text_color(palette.muted).hover(|el| el.text_color(palette.text)))
                .child(label)
                .on_click(move |_, window, cx| on_pick(value.clone(), window, cx))
        },
    ))
}

pub fn code_box(content: impl IntoElement, cx: &App) -> Div {
    let palette = appearance::palette(cx);
    div()
        .size_full()
        .rounded(dp(12.))
        .overflow_hidden()
        .border_1()
        .border_color(palette.border_soft)
        .bg(palette.code)
        .py(dp(6.))
        .child(content)
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
    div().text_xs().text_color(appearance::palette(cx).faint).child(text.to_string())
}

pub fn error_message(id: &'static str, error: &str, cx: &App) -> Div {
    let text = error.to_string();
    v_flex().gap(dp(8.)).child(div().text_color(appearance::palette(cx).danger).child(text.clone())).child(h_flex().child(
        Button::new(id).label("Copy error").on_click(move |_, _, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
        }),
    ))
}

pub fn eyebrow(text: &str, cx: &App) -> Div {
    div().text_size(rems(0.7)).font_weight(FontWeight::SEMIBOLD).text_color(appearance::palette(cx).faint).child(text.to_uppercase())
}

pub fn scroll(id: &'static str, content: AnyElement) -> AnyElement {
    div().id(id).size_full().overflow_y_scroll().child(content).into_any_element()
}

pub fn labelled_input(label: &str, input: &Entity<InputState>, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(div().w(dp(140.)).flex_none().text_xs().text_color(cx.theme().muted_foreground).child(label.to_string()))
        .child(div().flex_1().child(Input::new(input)))
}

pub fn name_value_rows(rows: Vec<(String, String)>, cx: &App) -> impl IntoElement {
    let palette = appearance::palette(cx);
    v_flex().gap_1p5().text_xs().children(rows.into_iter().map(|(name, value)| {
        h_flex()
            .gap_3()
            .child(div().w(dp(200.)).flex_none().text_color(palette.muted).child(name))
            .child(div().flex_1().min_w_0().font_family(MONO_FONT).child(value))
    }))
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
