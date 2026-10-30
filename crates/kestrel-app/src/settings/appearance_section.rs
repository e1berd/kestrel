use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div, rems,
};
use gpui_component::{h_flex, v_flex};

use super::{card, page_header, row, schemes};
use crate::appearance::palette::Palette;
use crate::appearance::{self, dp};
use crate::prefs::{InterfaceFont, Prefs, ThemeChoice};
use crate::ui::segmented;

pub fn render(prefs: &Prefs, cx: &App) -> AnyElement {
    let fonts = vec![(InterfaceFont::Inter, "Inter".to_string()), (InterfaceFont::System, "System font".to_string())];
    v_flex()
        .gap(dp(22.))
        .child(page_header("Appearance", "Theme, colour scheme and the interface font.", cx))
        .child(theme_cards(prefs, cx))
        .child(schemes::section(prefs, cx))
        .child(card(
            vec![row(
                "Interface font",
                None,
                segmented("ui-font", fonts, &prefs.interface_font, |font, _, cx| appearance::set_interface_font(font, cx), cx),
                cx,
            )],
            cx,
        ))
        .into_any_element()
}

fn theme_cards(prefs: &Prefs, cx: &App) -> impl IntoElement {
    let scheme = appearance::schemes(cx).iter().find(|scheme| scheme.id == prefs.scheme).or(appearance::schemes(cx).first()).cloned();
    let Some(scheme) = scheme else { return div().into_any_element() };
    let choices = [
        (ThemeChoice::System, "Like the system", split_preview(&scheme.light, &scheme.dark)),
        (ThemeChoice::Light, "Light", preview(&scheme.light)),
        (ThemeChoice::Dark, "Dark", preview(&scheme.dark)),
    ];
    let palette = appearance::palette(cx);
    h_flex()
        .gap(dp(12.))
        .children(choices.into_iter().map(|(choice, label, picture)| {
            let is_selected = prefs.theme == choice;
            v_flex()
                .id(gpui::ElementId::Name(format!("theme-{label}").into()))
                .flex_1()
                .rounded(dp(12.))
                .overflow_hidden()
                .border_1()
                .cursor_pointer()
                .bg(palette.surface)
                .border_color(if is_selected { palette.accent } else { palette.border })
                .child(picture)
                .child(
                    h_flex().px(dp(12.)).py(dp(10.)).justify_between().font_weight(FontWeight::MEDIUM).child(label).child(
                        div()
                            .size(dp(14.))
                            .rounded_full()
                            .border_2()
                            .border_color(if is_selected { palette.accent } else { palette.border })
                            .when(is_selected, |el| el.bg(palette.accent)),
                    ),
                )
                .on_click(move |_, _, cx| appearance::set_theme(choice, cx))
        }))
        .into_any_element()
}

fn bar(color: Hsla, width: f32) -> Div {
    div().h(dp(7.)).w(dp(width)).rounded_full().bg(color)
}

pub(super) fn preview(palette: &Palette) -> AnyElement {
    h_flex()
        .h(dp(86.))
        .bg(palette.surface)
        .child(
            v_flex()
                .w(dp(42.))
                .h_full()
                .p(dp(8.))
                .gap(dp(6.))
                .child(bar(palette.methods.get, 24.))
                .child(bar(palette.methods.post, 20.))
                .child(bar(palette.faint, 22.)),
        )
        .child(
            v_flex()
                .flex_1()
                .h_full()
                .my(dp(6.))
                .mr(dp(6.))
                .p(dp(8.))
                .gap(dp(7.))
                .rounded(dp(7.))
                .bg(palette.background)
                .child(
                    h_flex()
                        .gap(dp(5.))
                        .child(bar(palette.text, 48.))
                        .child(div().h(dp(11.)).w(dp(26.)).rounded(dp(4.)).bg(palette.accent)),
                )
                .child(bar(palette.muted, 64.))
                .child(bar(palette.methods.get, 36.)),
        )
        .into_any_element()
}

fn split_preview(light: &Palette, dark: &Palette) -> AnyElement {
    h_flex()
        .h(dp(86.))
        .child(div().flex_1().h_full().overflow_hidden().child(preview(light)))
        .child(div().flex_1().h_full().overflow_hidden().child(preview(dark)))
        .into_any_element()
}

pub(super) fn swatch(color: Hsla) -> impl IntoElement {
    div().size(dp(14.)).rounded_full().bg(color).text_size(rems(0.1))
}
