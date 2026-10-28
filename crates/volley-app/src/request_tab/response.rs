use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled,
    div, rems,
};
use gpui_component::input::Editor;
use gpui_component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit_assets::IconName;
use volley_core::Outcome;

use super::{RESPONSE_SECTIONS, RequestTab, ResponseSection};
use crate::appearance::{self, dp};
use crate::ui::{chip, code_box, format_duration, format_size, hint, pill_tab};

impl RequestTab {
    pub(super) fn render_response(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let frame = v_flex().size_full().bg(palette.background);
        if self.sending.is_some() {
            return frame.items_center().justify_center().text_color(palette.muted).child("Sending…").into_any_element();
        }
        let Some(outcome) = &self.outcome else {
            let relative = self.path.strip_prefix(self.project.read(cx).root()).unwrap_or(&self.path).display().to_string();
            return frame
                .items_center()
                .justify_center()
                .gap_1()
                .text_color(palette.muted)
                .child(format!("Press Send or {}", crate::workspace::shortcut("Enter")))
                .child(hint(&relative, cx))
                .into_any_element();
        };
        let error = outcome.error.clone().or_else(|| outcome.script_error.clone().map(|error| format!("Script: {error}")));
        frame
            .child(self.render_status_line(outcome, cx))
            .when_some(error, |el, error| {
                el.child(
                    div()
                        .mx(dp(14.))
                        .mt(dp(10.))
                        .p(dp(10.))
                        .rounded(dp(9.))
                        .bg(palette.tint(palette.danger))
                        .text_size(rems(0.84))
                        .text_color(palette.danger)
                        .child(error),
                )
            })
            .child(self.render_response_pills(outcome, cx))
            .child(div().flex_1().min_h_0().px(dp(14.)).pt(dp(12.)).pb(dp(14.)).child(self.render_response_content(outcome, cx)))
            .into_any_element()
    }

    fn render_status_line(&self, outcome: &Outcome, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let mut line = h_flex().flex_wrap().px(dp(14.)).pt(dp(12.)).gap(dp(10.)).items_center();
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
        h_flex().flex_wrap().px(dp(12.)).pt(dp(10.)).gap(dp(2.)).children(RESPONSE_SECTIONS.iter().enumerate().map(|(index, section)| {
            let (label, count) = match section {
                ResponseSection::Body => ("Body", None),
                ResponseSection::Headers => ("Headers", outcome.response.as_ref().map(|response| response.headers.len())),
                ResponseSection::Tests => ("Tests", Some(outcome.tests.len())),
                ResponseSection::Console => ("Console", Some(outcome.logs.len())),
                ResponseSection::Request => ("Request", None),
            };
            let section = *section;
            pill_tab(("response-section", index), label, count, section == self.response_section, cx).on_click(cx.listener(
                move |tab, _, _, cx| {
                    tab.response_section = section;
                    cx.notify();
                },
            ))
        }))
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
                                .rounded(dp(7.))
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.background)
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
