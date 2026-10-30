use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, rems,
};
use gpui_component::{h_flex, v_flex};

use super::{card, page_header, row};
use crate::appearance::{self, MONO_FONT, dp, zoom};
use crate::prefs::Prefs;
use crate::ui::{hint, segmented};

pub fn render(prefs: &Prefs, cx: &App) -> AnyElement {
    let text_steps = zoom::TEXT_STEPS.iter().map(|(step, label)| (*step, label.to_string())).collect();
    v_flex()
        .gap(dp(16.))
        .child(page_header(
            "Zoom and sizes",
            "The zoom makes everything larger at once; the text size changes only the text and the fields around it.",
            cx,
        ))
        .child(card(
            vec![
                row("Interface zoom", Some("Ctrl = · Ctrl - · Ctrl 0"), zoom_steps(prefs.interface_scale, cx), cx),
                row(
                    "Text size",
                    None,
                    segmented("text-size", text_steps, &prefs.text_scale, |percent, _, cx| appearance::set_text_scale(percent, cx), cx),
                    cx,
                ),
                row("Code font", Some("Bodies, scripts and responses"), code_font_stepper(prefs.code_font_size, cx), cx),
            ],
            cx,
        ))
        .into_any_element()
}

fn zoom_steps(current: u32, cx: &App) -> impl IntoElement {
    let palette = appearance::palette(cx);
    let position = zoom::ZOOM_STEPS.iter().position(|step| *step >= current).unwrap_or(0);
    h_flex()
        .gap(dp(12.))
        .items_center()
        .child(hint("80%", cx))
        .child(h_flex().w(dp(260.)).h(dp(22.)).items_center().gap(dp(4.)).children(zoom::ZOOM_STEPS.iter().enumerate().map(
            |(index, step)| {
                let step = *step;
                let is_filled = index <= position;
                let is_current = index == position;
                div()
                    .id(("zoom-step", index))
                    .flex_1()
                    .h(if is_current { dp(18.) } else { dp(6.) })
                    .rounded_full()
                    .cursor_pointer()
                    .bg(if is_filled { palette.accent } else { palette.border })
                    .when(is_current, |el| el.border_2().border_color(palette.text))
                    .hover(|el| el.opacity(0.8))
                    .on_click(move |_, _, cx| appearance::set_interface_scale(step, cx))
            },
        )))
        .child(hint("200%", cx))
        .child(value_box(format!("{current}%"), cx))
}

fn code_font_stepper(current: u32, cx: &App) -> impl IntoElement {
    let palette = appearance::palette(cx);
    let steps = zoom::CODE_FONT_STEPS;
    let index = steps.iter().position(|size| *size == current).unwrap_or(2);
    let smaller = steps[index.saturating_sub(1)];
    let larger = steps[(index + 1).min(steps.len() - 1)];
    let button = |id: &'static str, label: &'static str, size: u32| {
        div()
            .id(id)
            .size(dp(30.))
            .rounded(dp(8.))
            .border_1()
            .border_color(palette.border)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(palette.muted)
            .hover(|el| el.bg(palette.hover).text_color(palette.text))
            .child(label)
            .on_click(move |_, _, cx| appearance::set_code_font_size(size, cx))
    };
    h_flex()
        .gap(dp(8.))
        .items_center()
        .child(button("code-smaller", "−", smaller))
        .child(value_box(format!("{current} px"), cx))
        .child(button("code-larger", "+", larger))
}

fn value_box(text: String, cx: &App) -> impl IntoElement {
    let palette = appearance::palette(cx);
    div()
        .min_w(dp(56.))
        .text_center()
        .px(dp(8.))
        .py(dp(4.))
        .rounded(dp(7.))
        .bg(palette.raised)
        .font_family(MONO_FONT)
        .text_size(rems(0.8))
        .child(text)
}
