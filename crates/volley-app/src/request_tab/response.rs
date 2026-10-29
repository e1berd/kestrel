use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled, div, rems,
};
use gpui_component::input::Editor;
use gpui_component::spinner::Spinner;
use gpui_component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit_assets::IconName;
use volley_core::Outcome;

use super::{RESPONSE_SECTIONS, RequestTab, ResponseSection};
use crate::appearance::{self, MONO_FONT, dp};
use crate::ui::{chip, code_box, format_duration, format_size, hint, icon_tile, keys, section_tab, section_tab_strip};

impl RequestTab {
    pub(super) fn render_response(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let frame = v_flex().size_full().bg(palette.background);
        if self.sending.is_some() {
            return frame
                .items_center()
                .justify_center()
                .gap(dp(12.))
                .text_color(palette.muted)
                .child(Spinner::new().large().color(palette.accent))
                .child("Waiting for the response…")
                .into_any_element();
        }
        let Some(outcome) = &self.outcome else {
            return self.render_response_placeholder(frame, cx);
        };
        let error = outcome.error.clone().or_else(|| outcome.script_error.clone().map(|error| format!("Script: {error}")));
        frame
            .child(self.render_status_line(outcome, cx))
            .when_some(error, |el, error| {
                el.child(
                    div()
                        .mx(dp(16.))
                        .mb(dp(10.))
                        .p(dp(10.))
                        .rounded(dp(10.))
                        .bg(palette.tint(palette.danger))
                        .text_size(rems(0.84))
                        .text_color(palette.danger)
                        .child(error),
                )
            })
            .child(self.render_response_pills(outcome, cx))
            .child(div().flex_1().min_h_0().px(dp(16.)).pt(dp(14.)).pb(dp(16.)).child(self.render_response_content(outcome, cx)))
            .into_any_element()
    }

    fn render_response_placeholder(&self, frame: gpui::Div, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let relative = self.path.strip_prefix(self.project.read(cx).root()).unwrap_or(&self.path).display().to_string();
        frame
            .items_center()
            .justify_center()
            .gap(dp(10.))
            .child(icon_tile(IconName::Send, 44., cx))
            .child(div().pt(dp(4.)).font_weight(FontWeight::MEDIUM).text_color(palette.text).child("No response yet"))
            .child(
                h_flex()
                    .gap(dp(6.))
                    .items_center()
                    .text_size(rems(0.8))
                    .text_color(palette.muted)
                    .child("Press")
                    .child(keys(&crate::workspace::shortcut("Enter"), cx))
                    .child("to send it"),
            )
            .child(hint(&relative, cx).font_family(MONO_FONT))
            .into_any_element()
    }

    fn render_status_line(&self, outcome: &Outcome, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let mut line = h_flex().flex_wrap().px(dp(16.)).py(dp(12.)).gap(dp(12.)).items_center();
        line = match (&outcome.response, outcome.skipped) {
            (_, true) => line.child(chip("Skipped by a script", palette.faint, cx)),
            (Some(response), _) => line
                .child(chip(format!("{} {}", response.status, response.status_text), status_color(response.status, &palette), cx))
                .child(
                    h_flex()
                        .gap(dp(5.))
                        .text_size(rems(0.8))
                        .text_color(palette.muted)
                        .child(Icon::new(IconName::Clock).xsmall())
                        .child(format_duration(response.elapsed)),
                )
                .child(div().text_size(rems(0.8)).text_color(palette.muted).child(format_size(response.body.len()))),
            (None, _) => line.child(chip("No response", palette.danger, cx)),
        };
        if !outcome.tests.is_empty() {
            let passed = outcome.tests.iter().filter(|test| test.passed).count();
            let color = if passed == outcome.tests.len() { palette.success } else { palette.danger };
            line = line.child(div().flex_1()).child(chip(format!("Tests {passed}/{}", outcome.tests.len()), color, cx));
        }
        line
    }

    fn render_response_pills(&self, outcome: &Outcome, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<_> = RESPONSE_SECTIONS
            .iter()
            .enumerate()
            .map(|(index, section)| {
                let (label, count) = match section {
                    ResponseSection::Body => ("Body", None),
                    ResponseSection::Headers => ("Headers", outcome.response.as_ref().map(|response| response.headers.len())),
                    ResponseSection::Tests => ("Tests", Some(outcome.tests.len())),
                    ResponseSection::Console => ("Console", Some(outcome.logs.len())),
                    ResponseSection::Request => ("Request", None),
                };
                let section = *section;
                section_tab(("response-section", index), label, count, section == self.response_section, cx).on_click(cx.listener(
                    move |tab, _, _, cx| {
                        tab.response_section = section;
                        cx.notify();
                    },
                ))
            })
            .collect();
        section_tab_strip("response-sections", tabs, cx)
    }

    fn render_response_content(&self, outcome: &Outcome, cx: &mut Context<Self>) -> AnyElement {
        match self.response_section {
            ResponseSection::Body => match &outcome.response {
                Some(response) => {
                    let text = response.pretty();
                    let palette = appearance::palette(cx);
                    code_box(Editor::new(&self.response_body).bordered(false).readonly(true).h_full(), cx)
                        .relative()
                        .child(
                            div()
                                .id("copy-response")
                                .absolute()
                                .top(dp(8.))
                                .right(dp(8.))
                                .size(dp(28.))
                                .rounded(dp(8.))
                                .border_1()
                                .border_color(palette.border_soft)
                                .bg(palette.raised)
                                .shadow(appearance::elevation::lifted(cx))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .text_color(palette.muted)
                                .hover(|el| el.text_color(palette.text))
                                .child(Icon::new(IconName::Copy).xsmall())
                                .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))),
                        )
                        .into_any_element()
                }
                None => div().into_any_element(),
            },
            ResponseSection::Headers => self.render_response_headers(outcome, cx),
            ResponseSection::Tests => self.render_tests(outcome, cx),
            ResponseSection::Console => self.render_console(outcome, cx),
            ResponseSection::Request => self.render_sent_request(outcome, cx),
        }
    }
}

pub(super) fn status_color(status: u16, palette: &appearance::palette::Palette) -> gpui::Hsla {
    match status {
        200..=299 => palette.success,
        300..=399 => palette.info,
        400..=499 => palette.warning,
        _ => palette.danger,
    }
}
