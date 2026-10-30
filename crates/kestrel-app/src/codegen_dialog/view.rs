use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Window, div,
    rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::{Disableable as _, WindowExt as _, h_flex, v_flex};
use kestrel_codegen::QueryAdapter;

use super::CodegenDialog;
use crate::appearance::{self, dp};
use crate::ui::{eyebrow, hint};

impl CodegenDialog {
    fn render_output_options(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap(dp(10.))
            .child(self.render_query_adapter("codegen-tanstack-react", "TanStack Query for React", QueryAdapter::React, cx))
            .child(self.render_query_adapter("codegen-tanstack-vue", "TanStack Query for Vue", QueryAdapter::Vue, cx))
            .child(
                Checkbox::new("codegen-single-file")
                    .label("Bundle JavaScript into one file (+ .d.ts)")
                    .checked(self.output.single_file)
                    .disabled(self.busy)
                    .on_click(cx.listener(|view, checked: &bool, _, cx| {
                        view.output.single_file = *checked;
                        view.destination = None;
                        view.report = None;
                        view.error = None;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("codegen-archive")
                    .label("Pack into a ZIP archive")
                    .checked(self.output.archive)
                    .disabled(self.busy)
                    .on_click(cx.listener(|view, checked: &bool, _, cx| {
                        view.output.archive = *checked;
                        view.destination = None;
                        view.report = None;
                        view.error = None;
                        cx.notify();
                    })),
            )
    }

    fn render_query_adapter(
        &self,
        id: &'static str,
        label: &'static str,
        adapter: QueryAdapter,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Checkbox::new(id).label(label).checked(self.output.query == adapter).disabled(self.busy).on_click(cx.listener(
            move |view, checked: &bool, _, cx| {
                view.output.query = if *checked { adapter } else { QueryAdapter::None };
                view.report = None;
                view.error = None;
                cx.notify();
            },
        ))
    }

    fn render_destination(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap(dp(8.)).child(eyebrow("Destination", cx)).child(
            h_flex()
                .gap(dp(12.))
                .items_center()
                .child(div().flex_1().min_w_0().child(self.destination.as_ref().map(|path| path.display().to_string()).unwrap_or_else(
                    || {
                        if self.output.archive {
                            "Choose a ZIP file".into()
                        } else if self.output.single_file {
                            "Choose a JavaScript file".into()
                        } else {
                            "Choose an SDK folder".into()
                        }
                    },
                )))
                .child(
                    Button::new("codegen-file")
                        .label(if self.output.single_file || self.output.archive { "Choose file…" } else { "Choose folder…" })
                        .disabled(self.busy)
                        .on_click(cx.listener(|view, _, window, cx| view.choose_destination(window, cx))),
                ),
        )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap(dp(8.))
            .justify_end()
            .child(
                Button::new("codegen-cancel")
                    .label(if self.report.is_some() { "Done" } else { "Cancel" })
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                Button::new("codegen-start")
                    .primary()
                    .label("Generate SDK")
                    .loading(self.busy)
                    .disabled(self.busy || self.destination.is_none())
                    .on_click(cx.listener(|view, _, window, cx| view.generate(window, cx))),
            )
    }
}

impl Render for CodegenDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        v_flex()
            .gap(dp(18.))
            .child(
                v_flex()
                    .id("codegen-content")
                    .max_h(rems(25.))
                    .overflow_y_scroll()
                    .gap(dp(18.))
                    .child(hint("Turn saved requests into a JavaScript client with resource classes and async iterable lists.", cx))
                    .child(hint("Save your edits first. The SDK uses fetch and includes separate .d.ts types. Supply variables and credentials when creating the client.", cx))
                    .child(self.render_output_options(cx))
                    .child(self.render_destination(cx))
                    .when(self.busy || self.report.is_some(), |element| element.child(crate::transfer_progress::render(&self.progress, cx)))
                    .children(self.report.as_ref().map(|report| {
                        v_flex()
                            .gap(dp(6.))
                            .child(div().child(format!("Generated {} methods · {} source files", report.requests, report.files.len())))
                            .children(report.methods.iter().map(|method| div().text_size(rems(0.85)).child(format!("client.{method}()"))))
                            .children(report.warnings.iter().map(|warning| {
                                div().text_size(rems(0.8)).text_color(palette.warning).child(warning.clone())
                            }))
                    }))
                    .children(self.error.as_ref().map(|error| crate::ui::error_message("codegen-copy-error", error, cx))),
            )
            .child(self.render_footer(cx))
    }
}
