use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Window, div,
    rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{Disableable as _, WindowExt as _, h_flex, v_flex};
use kestrel_import::export::{Encoding, ExportFormat};

use super::ExportDialog;
use crate::appearance::{self, dp};
use crate::ui::{ReportUnlessGone as _, eyebrow, format_size, hint, segmented};

impl ExportDialog {
    fn render_format(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let options = ExportFormat::ALL.into_iter().map(|format| (format, format.name().to_string())).collect();
        v_flex().gap(dp(8.)).child(eyebrow("Format", cx)).child(segmented(
            "export-format",
            options,
            &self.format,
            move |format, _, cx| {
                weak.update(cx, |view, cx| {
                    if !view.busy {
                        view.format = format;
                        view.destination = None;
                        view.report = None;
                        if format == ExportFormat::Apidog {
                            view.encoding = Encoding::Json;
                        }
                        cx.notify();
                    }
                })
                .report_unless_gone("choose an export format");
            },
            cx,
        ))
    }

    fn render_encoding(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        v_flex().gap(dp(8.)).child(eyebrow("File type", cx)).child(segmented(
            "export-encoding",
            vec![(Encoding::Json, "JSON".into()), (Encoding::Yaml, "YAML".into())],
            &self.encoding,
            move |encoding, _, cx| {
                weak.update(cx, |view, cx| {
                    if !view.busy {
                        view.encoding = encoding;
                        view.destination = None;
                        cx.notify();
                    }
                })
                .report_unless_gone("choose export file type");
            },
            cx,
        ))
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap(dp(8.))
            .justify_end()
            .child(
                Button::new("export-cancel")
                    .label(if self.report.is_some() { "Done" } else { "Cancel" })
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                Button::new("export-start")
                    .primary()
                    .label("Export collection")
                    .loading(self.busy)
                    .disabled(self.busy || self.destination.is_none())
                    .on_click(cx.listener(|view, _, window, cx| view.export(window, cx))),
            )
    }
}

impl Render for ExportDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        v_flex()
            .gap(dp(18.))
            .child(
                v_flex()
                    .id("export-content")
                    .max_h(rems(25.))
                    .overflow_y_scroll()
                    .gap(dp(18.))
                    .child(hint("Exports saved requests from disk. Save your edits before exporting.", cx))
                    .child(self.render_format(cx))
                    .when(self.format != ExportFormat::Apidog, |element| element.child(self.render_encoding(cx)))
                    .child(
                        v_flex().gap(dp(8.)).child(eyebrow("Destination", cx)).child(
                            h_flex()
                                .gap(dp(12.))
                                .items_center()
                                .child(
                                    div().flex_1().min_w_0().child(
                                        self.destination
                                            .as_ref()
                                            .map(|path| path.display().to_string())
                                            .unwrap_or_else(|| "Choose an export file".into()),
                                    ),
                                )
                                .child(
                                    Button::new("export-file")
                                        .label("Choose file…")
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|view, _, window, cx| view.choose_destination(window, cx))),
                                ),
                        ),
                    )
                    .when(self.busy || self.report.is_some(), |element| element.child(crate::transfer_progress::render(&self.progress, cx)))
                    .children(self.report.as_ref().map(|report| {
                        v_flex()
                            .gap(dp(6.))
                            .child(div().child(format!("Exported {} requests · {}", report.requests, format_size(report.bytes as usize))))
                            .children(
                                report
                                    .warnings
                                    .iter()
                                    .map(|warning| div().text_size(rems(0.8)).text_color(palette.warning).child(warning.clone())),
                            )
                    }))
                    .children(self.error.as_ref().map(|error| div().text_color(palette.danger).child(error.clone()))),
            )
            .child(self.render_footer(cx))
    }
}
