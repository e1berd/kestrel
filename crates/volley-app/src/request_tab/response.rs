use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Context, FontWeight, IntoElement, ParentElement as _, Styled, div};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Editor;
use gpui_component::tab::{Tab, TabBar};
use gpui_component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};

use super::{RESPONSE_SECTIONS, RequestTab, ResponseSection};
use crate::appearance::dp;
use crate::ui::*;

impl RequestTab {
    pub(super) fn render_response(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        if self.sending.is_some() {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Sending…")
                .into_any_element();
        }
        let Some(outcome) = &self.outcome else {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_1()
                .text_color(theme.muted_foreground)
                .child("Press Send or Ctrl+Enter")
                .child(
                    div().text_xs().child(self.path.strip_prefix(self.project.read(cx).root()).unwrap_or(&self.path).display().to_string()),
                )
                .into_any_element();
        };
        let status: AnyElement = match (&outcome.response, outcome.skipped) {
            (_, true) => div().text_color(theme.muted_foreground).child("Skipped by a script").into_any_element(),
            (Some(r), _) => h_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .text_color(status_color(r.status, cx))
                        .child(format!("{} {}", r.status, r.status_text)),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(format_duration(r.elapsed)))
                .child(div().text_xs().text_color(theme.muted_foreground).child(format_size(r.body.len())))
                .into_any_element(),
            (None, _) => div().font_weight(FontWeight::BOLD).text_color(theme.red).child("No response").into_any_element(),
        };
        let passed = outcome.tests.iter().filter(|t| t.passed).count();
        let labels: Vec<String> = RESPONSE_SECTIONS
            .iter()
            .map(|s| match s {
                ResponseSection::Body => "Body".to_string(),
                ResponseSection::Headers => format!("Headers {}", outcome.response.as_ref().map(|r| r.headers.len()).unwrap_or(0)),
                ResponseSection::Tests if outcome.tests.is_empty() => "Tests".to_string(),
                ResponseSection::Tests => format!("Tests {passed}/{}", outcome.tests.len()),
                ResponseSection::Console if outcome.logs.is_empty() => "Console".to_string(),
                ResponseSection::Console => format!("Console {}", outcome.logs.len()),
                ResponseSection::Request => "Request".to_string(),
            })
            .collect();
        let selected = RESPONSE_SECTIONS.iter().position(|s| *s == self.response_section).unwrap_or(0);
        let tabs = TabBar::new("response-sections")
            .underline()
            .small()
            .selected_index(selected)
            .on_click(cx.listener(|tab, index: &usize, _, cx| {
                tab.response_section = RESPONSE_SECTIONS[*index];
                cx.notify();
            }))
            .children(labels.into_iter().map(|l| Tab::new().label(l)));
        let error = outcome.error.clone().or_else(|| outcome.script_error.clone().map(|e| format!("Script: {e}")));
        let content: AnyElement = match self.response_section {
            ResponseSection::Body => match &outcome.response {
                Some(_) => Editor::new(&self.response_body).h_full().readonly(true).into_any_element(),
                None => div().into_any_element(),
            },
            ResponseSection::Headers => {
                let rows = outcome.response.as_ref().map(|r| r.headers.clone()).unwrap_or_default();
                scroll("res-headers", name_value_rows(rows, cx).into_any_element())
            }
            ResponseSection::Tests => {
                let items = v_flex().gap_1().children(outcome.tests.iter().map(|t| {
                    let color = if t.passed { cx.theme().green } else { cx.theme().red };
                    v_flex()
                        .child(
                            h_flex().gap_2().child(div().text_color(color).child(if t.passed { "✓" } else { "✗" })).child(t.name.clone()),
                        )
                        .when_some(t.error.clone(), |el, e| el.child(div().pl_5().text_xs().text_color(cx.theme().red).child(e)))
                }));
                let empty = outcome.tests.is_empty().then(|| hint("No tests ran. Write them in Scripts → Tests.", cx));
                scroll("tests", v_flex().gap_2().children(empty).child(items).into_any_element())
            }
            ResponseSection::Console => {
                let lines = v_flex().gap_1().font_family("monospace").text_xs().children(outcome.logs.iter().map(|line| {
                    let color = match line.level.as_str() {
                        "error" => cx.theme().red,
                        "warn" => cx.theme().yellow,
                        _ => cx.theme().foreground,
                    };
                    div().text_color(color).child(line.text.clone())
                }));
                let empty = outcome.logs.is_empty().then(|| hint("Nothing was logged. console.log() in a script shows here.", cx));
                scroll("console", v_flex().gap_2().children(empty).child(lines).into_any_element())
            }
            ResponseSection::Request => match &outcome.sent {
                Some(sent) => scroll(
                    "sent",
                    v_flex()
                        .gap_2()
                        .child(div().font_family("monospace").text_xs().child(format!("{} {}", sent.method, sent.url)))
                        .child(name_value_rows(sent.headers.clone(), cx))
                        .when(!sent.body.is_empty(), |el| el.child(div().font_family("monospace").text_xs().child(sent.body.clone())))
                        .child(
                            h_flex().child(
                                Button::new("copy-curl")
                                    .xsmall()
                                    .ghost()
                                    .icon(IconName::Copy)
                                    .label("Copy as cURL")
                                    .on_click(cx.listener(|tab, _, _, cx| tab.copy_as_curl(cx))),
                            ),
                        )
                        .into_any_element(),
                ),
                None => hint("The request did not go out.", cx).into_any_element(),
            },
        };
        v_flex()
            .size_full()
            .child(h_flex().px_2().py_1().min_h(dp(36.)).items_center().justify_between().child(status))
            .when_some(error, |el, error| {
                el.child(
                    div()
                        .mx_2()
                        .mb_1()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().danger.opacity(0.12))
                        .text_sm()
                        .text_color(cx.theme().red)
                        .child(error),
                )
            })
            .child(div().px_2().child(tabs))
            .child(div().flex_1().min_h_0().p_2().child(content))
            .into_any_element()
    }
}
