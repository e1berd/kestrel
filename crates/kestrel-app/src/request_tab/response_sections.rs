use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled, div, rems};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit_assets::IconName;
use kestrel_core::Outcome;

use super::RequestTab;
use crate::appearance::{self, MONO_FONT, dp};
use crate::ui::{hint, name_value_rows, scroll};

impl RequestTab {
    pub(super) fn render_response_headers(&self, outcome: &Outcome, cx: &mut Context<Self>) -> AnyElement {
        let rows = outcome.response.as_ref().map(|response| response.headers.clone()).unwrap_or_default();
        scroll("response-headers", name_value_rows(rows, cx).into_any_element())
    }

    pub(super) fn render_tests(&self, outcome: &Outcome, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let items = v_flex().gap(dp(6.)).children(outcome.tests.iter().map(|test| {
            let (icon, color) = if test.passed { (IconName::CircleCheck, palette.success) } else { (IconName::CircleX, palette.danger) };
            v_flex()
                .p(dp(10.))
                .rounded(dp(9.))
                .bg(palette.raised)
                .gap(dp(4.))
                .child(h_flex().gap(dp(8.)).items_center().child(Icon::new(icon).small().text_color(color)).child(test.name.clone()))
                .when_some(test.error.clone(), |el, error| {
                    el.child(div().pl(dp(24.)).text_size(rems(0.78)).text_color(palette.danger).child(error))
                })
        }));
        let empty = outcome.tests.is_empty().then(|| hint("No tests ran. Write them in Scripts → Tests.", cx));
        scroll("tests", v_flex().gap_2().children(empty).child(items).into_any_element())
    }

    pub(super) fn render_console(&self, outcome: &Outcome, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let lines = v_flex().gap(dp(4.)).font_family(MONO_FONT).text_size(rems(0.78)).children(outcome.logs.iter().map(|line| {
            let color = match line.level.as_str() {
                "error" => palette.danger,
                "warn" => palette.warning,
                _ => palette.text,
            };
            div().text_color(color).child(line.text.clone())
        }));
        let empty = outcome.logs.is_empty().then(|| hint("Nothing was logged. console.log() in a script shows here.", cx));
        scroll("console", v_flex().gap_2().children(empty).child(lines).into_any_element())
    }

    pub(super) fn render_sent_request(&self, outcome: &Outcome, cx: &mut Context<Self>) -> AnyElement {
        let Some(sent) = &outcome.sent else { return hint("The request did not go out.", cx).into_any_element() };
        let palette = appearance::palette(cx);
        scroll(
            "sent",
            v_flex()
                .gap(dp(10.))
                .child(
                    div()
                        .font_family(MONO_FONT)
                        .text_size(rems(0.8))
                        .text_color(palette.text)
                        .child(format!("{} {}", sent.method, sent.url)),
                )
                .child(name_value_rows(sent.headers.clone(), cx))
                .when(!sent.body.is_empty(), |el| {
                    el.child(
                        div()
                            .p(dp(10.))
                            .rounded(dp(9.))
                            .bg(palette.code)
                            .font_family(MONO_FONT)
                            .text_size(rems(0.78))
                            .child(sent.body.clone()),
                    )
                })
                .child(
                    h_flex().child(
                        Button::new("copy-curl")
                            .small()
                            .ghost()
                            .icon(Icon::new(IconName::Copy))
                            .label("Copy as cURL")
                            .on_click(cx.listener(|tab, _, _, cx| tab.copy_as_curl(cx))),
                    ),
                )
                .into_any_element(),
        )
    }
}
